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
  sync::{broadcast, mpsc},
  time::{interval, Instant},
};
use tokio_stream::wrappers::{errors::BroadcastStreamRecvError, BroadcastStream};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use crate::{
  modules::{ModuleManager, MulticonnectCtx},
  SharedDaemon,
};

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

pub struct NetworkManager {
  send_packet_tx: mpsc::Sender<(PeerId, Packet)>,
  recv_peer_packet_rx: broadcast::Receiver<(PeerId, Packet)>,
  local_peer_id: PeerId,
}

impl NetworkManager {
  pub async fn start() -> Result<Self, Box<dyn Error>> {
    let _ = tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).try_init();

    let (send_packet_tx, mut send_packet_rx) = mpsc::channel::<(PeerId, Packet)>(100);
    let (recv_peer_packet_tx, recv_peer_packet_rx) = broadcast::channel::<(PeerId, Packet)>(100);

    debug!("Initializing new swarm");

    let keys = Self::get_keys().await?;
    let local_peer_id = keys.public().to_peer_id();
    let mut discovered_peers: Vec<PeerId> = Vec::new();

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
                if !discovered_peers.contains(&peer_id) {
                  discovered_peers.push(peer_id);
                  info!("Discovered peer: id = {}, multiaddr = {}", peer_id, multiaddr);
                  if local_peer_id > peer_id {
                    debug!("Sending metadata to: {}", peer_id);
                    let _ = swarm.behaviour_mut().packet_protocol.send_request(&peer_id, Packet::S1PeerMeta(S1PeerMeta::from_device(&this_device)));
                  }
                }
              }
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::Mnds(mdns::Event::Expired(expired))) => {
              for (peer_id, multiaddr) in expired {
                info!("Expired peer: id = {}, multiaddr = {}", peer_id, multiaddr);

                let _ = recv_peer_packet_tx.send((this_device.peer, Packet::L1PeerExpired(L1PeerExpired::new(&peer_id)))); // Super non jank way to show the peer expired
              }
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::PacketProtocol(request_response::Event::Message { peer, message, .. })) => {
              let packet_source = peer;
              match message {
                request_response::Message::Request { request_id: _, request, channel } => {
                  debug!("Received multiconnect protocol event from {}", peer);
                  let _ = recv_peer_packet_tx.send((packet_source, request.clone()));
                  let _ = swarm.behaviour_mut().packet_protocol.send_response(channel, ());
                },
                request_response::Message::Response { request_id: _, response: _ } => (),
              }
            }
            _ => {
              trace!("Event: {:?}", event);
            },
          },
          res = send_packet_rx.recv() => if let Some((target, packet)) = res {
            debug!("Sending {:?} to {}", packet, target);
            swarm.behaviour_mut().packet_protocol.send_request(&target, packet);
          }
        }
      }
    });

    Ok(Self { send_packet_tx, recv_peer_packet_rx, local_peer_id })
  }
  /**
   * Get saved keys or generate new ones if they don't exist
   */
  async fn get_keys() -> std::io::Result<Keypair> {
    let mut path = CONFIG.read().await.get_config_dir().clone();
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

  pub fn get_local_peer_id(&self) -> PeerId {
    self.local_peer_id
  }

  pub fn send_packet_channel(&self) -> mpsc::Sender<(PeerId, Packet)> {
    self.send_packet_tx.clone()
  }

  pub fn recv_packet_channel(&self) -> broadcast::Receiver<(PeerId, Packet)> {
    self.recv_peer_packet_rx.resubscribe()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
}
