mod protocols;
mod store;

use std::{
  collections::{HashMap, HashSet},
  error::Error,
  io,
  str::FromStr,
  time::Duration,
};

use bincode::de;
use lazy_static::lazy_static;
use libp2p::{
  futures::StreamExt,
  identity::Keypair,
  mdns, noise,
  request_response::{self, Config, OutboundRequestId, ProtocolSupport},
  swarm::{ConnectionId, NetworkBehaviour, SwarmEvent},
  tcp, yamux, Multiaddr, PeerId, SwarmBuilder,
};
use log::{debug, error, info, trace, warn};
use multiconnect_config::CONFIG;
use multiconnect_protocol::{
  local::peer::{L0PeerFound, L1PeerExpired, L2PeerPairRequest},
  p2p::peer::{P2PeerPairRequest, P3PeerPairResponse},
  shared::peer::{DeviceType, S1PeerMeta},
  Device, Packet, Peer,
};
use protocols::PacketCodec;
use store::Store;
use tokio::{
  fs::File,
  io::{AsyncReadExt, AsyncWriteExt},
  time::{interval, Instant},
};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use crate::SharedDaemon;

#[derive(NetworkBehaviour)]
struct MulticonnectBehavior {
  mnds: mdns::tokio::Behaviour,
  packet_protocol: request_response::Behaviour<PacketCodec>,
}

impl MulticonnectBehavior {
  pub fn new(key: &libp2p::identity::Keypair) -> Result<Self, Box<dyn Error>> {
    let packet_protocol = request_response::Behaviour::<PacketCodec>::new(
      vec![("/multiconnect/1".into(), ProtocolSupport::Full)],
      Config::default(),
    );

    let mdns_config: mdns::Config = mdns::Config {
      ttl: Duration::from_secs(5),
      query_interval: std::time::Duration::from_secs(1),
      ..Default::default()
    };

    Ok(Self { mnds: mdns::tokio::Behaviour::new(mdns_config, key.public().to_peer_id())?, packet_protocol })
  }
}

pub struct NetworkManager {}

impl NetworkManager {
  pub async fn start(daemon: SharedDaemon) -> Result<(), Box<dyn Error>> {
    let _ = tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).try_init();

    let mut pending_requests: HashMap<Uuid, (Instant, Packet, PeerId)> = HashMap::new();
    let mut timeout = interval(Duration::from_secs(30));

    let mut devices: HashMap<PeerId, Device> = HashMap::new();

    let mut keystore = Store::new();

    debug!("Initializing new swarm");

    let keys = Self::get_keys().await?;
    let local_peer_id = keys.public().to_peer_id();

    let this_device = Device::this(local_peer_id);
    debug!("Current device: {:?}", this_device);

    let mut swarm = SwarmBuilder::with_existing_identity(keys.clone())
      .with_tokio()
      .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)?
      .with_behaviour(|key| MulticonnectBehavior::new(key).unwrap())?
      .build();

    info!("Local peer id: {}", local_peer_id);

    if let Some(port) = port_check::free_local_ipv4_port_in_range(1590..=1600) {
      let addr: Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", port).parse()?;
      if swarm.listen_on(addr.clone()).is_ok() {
        info!("Listening on {:?}", addr);
      }
    } else {
      return Err(Box::new(io::Error::new(
        io::ErrorKind::AddrNotAvailable,
        "Could not find a port to bind to in rage 1590-1600",
      )));
    }

    let _ = tokio::spawn(async move {
      let mut packet_stream = daemon.packet_stream();
      loop {
        tokio::select! {
          event = swarm.select_next_some() => match event {
            SwarmEvent::NewListenAddr { address, ..} => {
              info!("Multiconnect listing on {:?}", address);
            }
            SwarmEvent::OutgoingConnectionError { connection_id, error, .. } => {
              warn!("Connection failed for `{}` : {}", connection_id, error);
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::Mnds(mdns::Event::Discovered(discoverd))) => {
              for (peer_id, multiaddr) in discoverd {
                info!("Discovered peer: id = {}, multiaddr = {}", peer_id, multiaddr);
                if local_peer_id > peer_id {
                  debug!("Sending metadata to: {}", peer_id);
                  let _ = swarm.behaviour_mut().packet_protocol.send_request(&peer_id, Packet::S1PeerMeta(S1PeerMeta::from_device(&this_device)));
                }
              }
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::Mnds(mdns::Event::Expired(expired))) => {
              for (peer_id, multiaddr) in expired {
                info!("Expired peer: id = {}, multiaddr = {}", peer_id, multiaddr);

                devices.remove(&peer_id);
                daemon.send_packet(Packet::L1PeerExpired(L1PeerExpired::new(&peer_id))).await
              }
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::PacketProtocol(request_response::Event::Message { peer, message, .. })) => {
              let packet_source = peer;
              match message {
                request_response::Message::Request { request_id: _, request, channel } => {
                  debug!("Received multiconnect protocol event from {}", peer);
                  match request {
                    Packet::P0Ping(ping) => todo!(),
                    Packet::P1Acknowledge(acknowledge) => todo!(),
                    Packet::P2PeerPairRequest(peer_pair_request) => {
                      info!("Received pairing request from {:?}, req_id = {}", packet_source, peer_pair_request.req_uuid);
                      let uuid = Uuid::from_str(&peer_pair_request.req_uuid).unwrap();
                      let device = bincode::deserialize::<Device>(&peer_pair_request.device).unwrap();
                      pending_requests.insert(uuid, (Instant::now(), Packet::P2PeerPairRequest(peer_pair_request.clone()), packet_source));

                      daemon.send_packet(Packet::L2PeerPairRequest(L2PeerPairRequest::new(&device, uuid))).await; // TODO: Replace
                    },
                    Packet::P3PeerPairResponse(peer_pair_response) => {
                      info!("Received paring response: id = {}, res = {:?}", peer_pair_response.req_uuid, peer_pair_response.accepted);
                      let uuid = Uuid::from_str(&peer_pair_response.req_uuid).unwrap();
                      debug!("uuid = {}, pending = {:?}", uuid, pending_requests);
                      if let Some((_, Packet::L2PeerPairRequest(req), _)) = pending_requests.remove(&uuid) {
                        debug!("Found a valid request for a response");
                        if peer_pair_response.accepted {
                          let device = bincode::deserialize::<Device>(&req.device).unwrap();
                          info!("Successfully paired with: {}", device.peer);
                        } else {
                          info!("Pairing request denied")
                        }
                      }
                    },
                    Packet::S1PeerMeta(peer_meta) => {
                      let device = Device::from_meta(peer_meta, packet_source);
                      debug!("Received meta for device: {:?}", device);

                      if local_peer_id < packet_source {
                        debug!("Sending metadata to: {}", packet_source);
                        let _ = swarm.behaviour_mut().packet_protocol.send_request(&packet_source, Packet::S1PeerMeta(S1PeerMeta::from_device(&this_device)));
                      }

                      daemon.send_packet(Packet::L0PeerFound(L0PeerFound::new(&device))).await;
                      devices.insert(device.peer, device);
                    }
                    _ => {
                      warn!("Received unexpected packet over network")
                    }
                  };
                  let _ = swarm.behaviour_mut().packet_protocol.send_response(channel, ());
                },
                request_response::Message::Response { request_id: _, response: _ } => (),
              }
            }

            _ => {
              trace!("Event: {:?}", event);
            }
          },

          packet = packet_stream.next() => if let Some(p) = packet {
            match p {
              Ok(Packet::L2PeerPairRequest(packet)) => { // Receive a request to pair to a peer from the frontend
                let device = bincode::deserialize::<Device>(&packet.device).unwrap(); // Get the target peer to send the request to from the frontend
                debug!("Sending pair request to: {}, id = {}", device.peer, packet.req_uuid);
                let uuid = Uuid::from_str(&packet.req_uuid).unwrap();
                pending_requests.insert(uuid, (Instant::now(), Packet::L2PeerPairRequest(packet.clone()), local_peer_id)); // Add the request to pending requests so we know if we get a response
                let _ = swarm.behaviour_mut().packet_protocol.send_request(&device.peer, Packet::P2PeerPairRequest(P2PeerPairRequest::new(&device, uuid))); // Send the request to the peer
              },
              Ok(Packet::L3PeerPairResponse(packet)) => { // Receive a response from the frontend to a pair request
                let uuid = Uuid::from_str(&packet.req_uuid).unwrap();
                if let Some((_, Packet::P2PeerPairRequest(_req), source)) = pending_requests.remove(&uuid) { // Check if there is a pending request for that id
                  debug!("Sending back response");
                  let _ = swarm.behaviour_mut().packet_protocol.send_request(&source, Packet::P3PeerPairResponse(P3PeerPairResponse::new(uuid, packet.accepted))); // Send the response
                }
              },
              Ok(Packet::L4Refresh(_)) => {
                for (_, device) in devices.iter() {
                  daemon.send_packet(Packet::L0PeerFound(L0PeerFound::new(&device))).await;
                }
              },
              Ok(_) => {},
              Err(e) => {
                error!("Error decoding packet: {}", e)
              },
            }
          } else {
            warn!("Stream closed");
            break;
          },

          _ = timeout.tick() => {
            pending_requests.retain(|_, (instant, _, _)| instant.elapsed().as_secs() < 60);
          }
        }
      }
    });

    Ok(())
  }
  /**
   * Get saved keys or generate new ones if they don't exist
   */
  async fn get_keys() -> std::io::Result<Keypair> {
    let mut path = CONFIG.get_config_dir().clone();
    path.push("keys");

    if let Ok(mut file) = File::open(&path).await {
      let mut buf = Vec::new();
      file.read_to_end(&mut buf).await?;
      return Keypair::from_protobuf_encoding(&buf)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid key format"));
    }

    info!("Creating new keypair and keyfile in {:?}", path);

    let keypair = Keypair::generate_ed25519();
    let encoded =
      keypair.to_protobuf_encoding().map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to encode keypair"))?;

    let mut file = File::create(&path).await?;
    file.write_all(&encoded).await?;

    Ok(keypair)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
}
