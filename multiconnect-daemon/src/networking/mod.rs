mod protocols;

use std::{collections::HashSet, error::Error, io, time::Duration};

use libp2p::{
  futures::StreamExt,
  identity::Keypair,
  mdns, noise,
  request_response::{self, Config, Message, ProtocolSupport, ResponseChannel},
  swarm::{NetworkBehaviour, SwarmEvent},
  tcp, yamux, Multiaddr, PeerId, SwarmBuilder,
};
use log::{debug, info, trace, warn};
use multiconnect_config::CONFIG;
use multiconnect_protocol::{
  local::peer::{L0PeerFound, L1PeerExpired, L6PeerDiscovered},
  p2p::P0Ping,
  shared::peer::*,
  Device, Packet, Peer,
};
use protocols::{BehaviourEvent, MulticonnectDataBehaviour, PairingCodec};
use tokio::{
  fs::File,
  io::{AsyncReadExt, AsyncWriteExt},
  sync::{broadcast, mpsc},
};
use tracing_subscriber::EnvFilter;

#[derive(Debug)]
pub enum MulticonnectNetworkEvent {
  RecvRequest(PeerId, Packet, ResponseChannel<Packet>),
  RecvResponse(PeerId, Packet),
  ConnectionOpenRequest(PeerId),
  ConnectionClosed(PeerId),
}

pub enum NetworkCommand {
  SendPacket(PeerId, Packet),
  ApproveStream(PeerId),
  DenyStream(PeerId),
  OpenStream(PeerId),
  CloseStream(PeerId),
  SendPacketProtocolRequest(PeerId, Packet),
  SendPacketProtocolResponse(ResponseChannel<Packet>, Packet),
}

#[derive(NetworkBehaviour)]
struct MulticonnectBehavior {
  mdns: mdns::tokio::Behaviour,
  pairing_protocol: request_response::Behaviour<PairingCodec>,
  packet_protocol: protocols::MulticonnectDataBehaviour,
}

impl MulticonnectBehavior {
  pub fn new(key: &libp2p::identity::Keypair) -> Result<Self, Box<dyn Error>> {
    let pairing_protocol = request_response::Behaviour::<PairingCodec>::new(
      vec![("/mc-pairing/0.0.1".into(), ProtocolSupport::Full)],
      Config::default(),
    );

    let packet_protocol = MulticonnectDataBehaviour::new();

    let mdns_config: mdns::Config = mdns::Config {
      ttl: Duration::from_secs(2),
      query_interval: std::time::Duration::from_secs(1),
      ..Default::default()
    };

    let mdns = mdns::tokio::Behaviour::new(mdns_config, key.public().to_peer_id())?;

    Ok(Self { mdns, pairing_protocol, packet_protocol })
  }
}

pub struct NetworkManager {
  send_command_tx: mpsc::Sender<NetworkCommand>,
  send_command_rx: Option<mpsc::Receiver<NetworkCommand>>,

  recv_peer_packet_rx: broadcast::Receiver<(PeerId, Packet)>,
  recv_peer_packet_tx: broadcast::Sender<(PeerId, Packet)>,

  mc_network_event_recv_rx: Option<mpsc::Receiver<MulticonnectNetworkEvent>>,
  mc_network_event_recv_tx: mpsc::Sender<MulticonnectNetworkEvent>,

  keys: Keypair,
}

impl NetworkManager {
  pub async fn new() -> Self {
    let (send_command_tx, send_command_rx) = mpsc::channel::<NetworkCommand>(100);
    let (recv_peer_packet_tx, recv_peer_packet_rx) = broadcast::channel::<(PeerId, Packet)>(100);
    let (mc_network_event_recv_tx, mc_network_event_recv_rx) = mpsc::channel::<MulticonnectNetworkEvent>(10);

    let keys = Self::get_keys().await.unwrap();

    Self {
      send_command_tx,
      send_command_rx: Some(send_command_rx),

      recv_peer_packet_rx,
      recv_peer_packet_tx,

      mc_network_event_recv_tx,
      mc_network_event_recv_rx: Some(mc_network_event_recv_rx),
      keys,
    }
  }
  pub async fn start(&mut self) -> Result<(), Box<dyn Error>> {
    let _ = tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).try_init();

    let mut send_packet_rx = self.send_command_rx.take().unwrap();
    let recv_peer_packet_tx = self.recv_peer_packet_tx.clone();
    let mc_network_event_recv_tx = self.mc_network_event_recv_tx.clone();

    debug!("Initializing new swarm");
    let mut discovered_peers: HashSet<PeerId> = HashSet::new();

    let this_device = Device::this(self.get_local_peer_id());
    debug!("Current device: {:?}", this_device);

    let mut swarm = SwarmBuilder::with_existing_identity(self.keys.clone())
      .with_tokio()
      .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)?
      .with_behaviour(|key| MulticonnectBehavior::new(key).unwrap())?
      .build();

    info!("Local peer id: {}", self.get_local_peer_id());

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
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::Mdns(mdns::Event::Discovered(discoverd))) => {
              for (peer_id, _multiaddr) in discoverd {
                if !discovered_peers.contains(&peer_id) {
                  discovered_peers.insert(peer_id);
                  info!("Discovered peer: {}", peer_id);
                  let _ = recv_peer_packet_tx.send((this_device.peer, Packet::L6PeerDiscovered(L6PeerDiscovered::new(&peer_id)))); // Notify modules peer found
                }
              }
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::Mdns(mdns::Event::Expired(expired))) => {
              for (peer_id, multiaddr) in expired {
                if discovered_peers.remove(&peer_id) {
                  info!("Expired peer: id = {}, multiaddr = {}", peer_id, multiaddr);
                  let _ = recv_peer_packet_tx.send((this_device.peer, Packet::L1PeerExpired(L1PeerExpired::new(&peer_id)))); // Notify modules that peer expired
                }
              }
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::PairingProtocol(request_response::Event::Message { peer, message, .. })) => {
              match message {
                Message::Request { request, channel, .. } => {
                  debug!("Recv request");
                  if let Err(e) = mc_network_event_recv_tx.send(MulticonnectNetworkEvent::RecvRequest(peer, request, channel)).await {
                    warn!("Error sending request on channel: {}", e);
                  }
                },
                Message::Response { response, .. } => {
                  debug!("Recv response");
                  if let Err(e) = mc_network_event_recv_tx.send(MulticonnectNetworkEvent::RecvResponse(peer, response)).await {
                    warn!("Error sending response on channel: {}", e);
                  }
                }
              }
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::PacketProtocol(BehaviourEvent::PacketRecived(source, packet))) => {
              debug!("Received multiconnect protocol event from {}", source);
              let _ = recv_peer_packet_tx.send((source, packet));
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::PacketProtocol(BehaviourEvent::ConnectionOpenRequest(peer))) => {
              debug!("Stream open request from {}", peer);
              let _ = mc_network_event_recv_tx.send(MulticonnectNetworkEvent::ConnectionOpenRequest(peer)).await;
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::PacketProtocol(BehaviourEvent::ConnectionClosed(peer))) => {
              info!("Channel to {} closed", peer);
              let _ = mc_network_event_recv_tx.send(MulticonnectNetworkEvent::ConnectionClosed(peer));
            }
            _ => {
              trace!("Event: {:?}", event);
            },
          },
          cmd = send_packet_rx.recv() => if let Some(cmd) = cmd {
            match cmd {
                NetworkCommand::SendPacket(peer_id, packet) => {
                  debug!("Sending {:?} to {}", packet, peer_id);
                  let _ = swarm.behaviour_mut().packet_protocol.send_packet(&peer_id, packet).await;
                },
                NetworkCommand::ApproveStream(peer_id) => {
                  debug!("Approving stream for {}", peer_id);
                  let _ = swarm.behaviour_mut().packet_protocol.approve_inbound_stream(peer_id);
                },
                NetworkCommand::DenyStream(peer_id) => {
                  debug!("Denying stream for {}", peer_id);
                  let _ = swarm.behaviour_mut().packet_protocol.deny_inbound_stream(&peer_id);
                },
                NetworkCommand::OpenStream(peer_id) => {
                  debug!("Initation stream open for {}", peer_id);
                  let _ = swarm.behaviour_mut().packet_protocol.open_stream(peer_id);
                },
                NetworkCommand::CloseStream(peer_id) => {
                  debug!("Closing stream for {}", peer_id);
                  todo!()
                },
                NetworkCommand::SendPacketProtocolRequest(peer_id, packet) => {
                  debug!("Sending pairing protocol request to {}", peer_id);
                  let _ = swarm.behaviour_mut().pairing_protocol.send_request(&peer_id, packet);
                },
                NetworkCommand::SendPacketProtocolResponse(ch, packet) => {
                  if ch.is_open() {
                    debug!("Sending pairing protocl response");
                    let _ = swarm.behaviour_mut().pairing_protocol.send_response(ch, packet);
                  } else {
                    warn!("Cannot send response on closed channel");
                  }
                }
            }
          },
        }
      }
    });

    Ok(())
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
    self.keys.public().to_peer_id()
  }

  pub fn send_command_channel(&self) -> mpsc::Sender<NetworkCommand> {
    self.send_command_tx.clone()
  }

  pub fn recv_packet_channel(&self) -> broadcast::Receiver<(PeerId, Packet)> {
    self.recv_peer_packet_rx.resubscribe()
  }

  pub fn get_mc_event_recv(&mut self) -> Option<mpsc::Receiver<MulticonnectNetworkEvent>> {
    if let Some(ch) = self.mc_network_event_recv_rx.take() {
      return Some(ch);
    }
    warn!("Attempted to get channel when it has already been taken");
    None
  }
}
#[cfg(test)]
mod tests {
  use super::*;
}
