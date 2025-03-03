mod protocols;
mod store;

use std::{
  collections::{HashMap, HashSet},
  error::Error,
  hash::Hash,
  io,
  time::Duration,
};

use libp2p::{
  futures::{channel::mpsc, StreamExt},
  identify,
  identity::{self, Keypair},
  mdns, noise,
  request_response::{self, Config, ProtocolSupport, ResponseChannel},
  swarm::{NetworkBehaviour, SwarmEvent},
  tcp, yamux, Multiaddr, PeerId, Stream, StreamProtocol, SwarmBuilder,
};
use log::{debug, error, info, trace, warn};
use multiconnect_config::CONFIG;
use multiconnect_protocol::{
  peer::{PeerExpired, PeerFound},
  Packet, Peer,
};
use protocols::PairingCodec;
use store::Store;
use tokio::{
  fs::{File, OpenOptions},
  io::{AsyncReadExt, AsyncWriteExt},
  time::{interval, Instant, Interval},
};
use tracing_subscriber::EnvFilter;

use crate::SharedDaemon;

#[derive(NetworkBehaviour)]
struct MulticonnectBehavior {
  mnds: mdns::tokio::Behaviour,
  pairing: request_response::Behaviour<PairingCodec>,
  stream: libp2p_stream::Behaviour,
}

impl MulticonnectBehavior {
  pub fn new(key: &libp2p::identity::Keypair) -> Result<Self, Box<dyn Error>> {
    let mnds_cfg = mdns::Config {
      ttl: Duration::from_secs(5),
      query_interval: std::time::Duration::from_secs(1),
      ..Default::default()
    };

    let pairing_protocol = request_response::Behaviour::<PairingCodec>::new(
      vec![("/pairing/1".into(), ProtocolSupport::Full)],
      Config::default(),
    );

    Ok(Self {
      mnds: mdns::tokio::Behaviour::new(mnds_cfg, key.public().to_peer_id())?,
      pairing: pairing_protocol,
      stream: libp2p_stream::Behaviour::new(),
    })
  }
}

pub struct NetworkManager {}

impl NetworkManager {
  pub async fn start(daemon: SharedDaemon) -> Result<(), Box<dyn Error>> {
    let _ = tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).try_init();

    let mut pending_requests: HashMap<u32, (ResponseChannel<_>, Instant)> = HashMap::new();
    let mut timeout = interval(Duration::from_secs(30));

    let mut peers: HashMap<PeerId, Peer> = HashMap::new();

    let mut keystore = Store::new();

    debug!("Initializing new swarm");

    let keys = Self::get_keys().await?;
    let peer_id = keys.public().to_peer_id();

    let mut swarm = SwarmBuilder::with_existing_identity(keys)
      .with_tokio()
      .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)?
      .with_behaviour(|key| MulticonnectBehavior::new(key).unwrap())?
      .build();

    info!("Local peer id: {}", peer_id);

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
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::Mnds(mdns::Event::Discovered(discoverd))) => {
              for (peer_id, multiaddr) in discoverd {
                info!("Discoverd peer: id = {}, multiaddr = {}", peer_id, multiaddr);
                let peer = Peer { peer_id, multiaddr: multiaddr.clone() };
                peers.insert(peer_id, peer.clone());

                daemon.send_packet(Packet::PeerFound(PeerFound::new(peer))).await;
              }
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::Mnds(mdns::Event::Expired(expired))) => {
              for (peer_id, multiaddr) in expired {
                info!("Expired peer: id = {}, multiaddr = {}", peer_id, multiaddr);

                peers.remove(&peer_id);

                daemon.send_packet(Packet::PeerExpired(PeerExpired::new(&peer_id))).await
              }
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::Pairing(request_response::Event::Message { peer, connection_id, message })) => {
              debug!("Received pairing protocol event from {}", peer);
              match message {
                request_response::Message::Request { request_id: _, request, channel } => {
                  info!("Received pairing request from {:?}", bincode::deserialize::<Peer>(&request.peer).unwrap().peer_id);

                  pending_requests.insert(request.id, (channel, Instant::now()));

                  daemon.send_packet(Packet::PeerPairRequest(request)).await;

                },
                request_response::Message::Response { request_id, response } => {
                  info!("Received paring response: id = {}, res = {:?}", request_id, response);

                  if pending_requests.get(&response.req_id).is_some() {
                    if response.accepted {
                      let peer = bincode::deserialize::<Peer>(&response.peer).unwrap();
                      keystore.store_peer(peer.peer_id, peer);
                    }
                    daemon.send_packet(Packet::PeerPairResponse(response)).await;
                    // swarm.behaviour_mut().stream;
                  };
                },
              }
            }

            _ => {
              trace!("Event: {:?}", event);
            }
          },

          packet = packet_stream.next() => if let Some(p) = packet {
            match p {
              Ok(Packet::PeerPairRequest(packet)) => {
                let peer = bincode::deserialize::<Peer>(&packet.peer).unwrap();
                debug!("Sending pair request to: {}", peer.peer_id);
                swarm.behaviour_mut().pairing.send_request(&peer.peer_id, packet);
              },
              Ok(Packet::PeerPairResponse(packet)) => {
                if let Some((ch, _)) = pending_requests.remove(&packet.req_id) {
                  let _ = swarm.behaviour_mut().pairing.send_response(ch, packet);
                }
              },
              Ok(_) => {}
              Err(e) => {
                error!("Error decoding packet: {}", e)
              },
            }
          } else {
            warn!("Stream closed");
            break;
          },

          _ = timeout.tick() => {
            pending_requests.retain(|_, (_, instant)| instant.elapsed().as_secs() < 60);
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
