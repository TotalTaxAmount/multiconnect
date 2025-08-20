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
use log::{debug, error, info, trace, warn};
use multiconnect_config::CONFIG;
use multiconnect_core::{Device, Packet};
use protocols::{BehaviourEvent, PairingCodec};
use tokio::{
  fs::File,
  io::{AsyncReadExt, AsyncWriteExt},
  sync::{broadcast, mpsc},
};
use tracing_subscriber::EnvFilter;

use crate::networking::protocols::StreamProtocolBehavior;

/// Has to be seperate because `ResponseChannel` is not `Clone`
#[derive(Debug)]
pub enum PairingProtocolEvent {
  /// Receive pairing request from a peer
  RecvRequest(PeerId, Packet, ResponseChannel<Packet>),
  /// Receivie a pairing resposne from a peer
  RecvResponse(PeerId, Packet),
}

/// Diffrent swarm events that get passed to modules
#[derive(Debug, Clone)]
pub enum NetworkEvent {
  /// A peer is discoverd on the network
  PeerDiscoverd(PeerId),
  /// A peer expires on the network
  PeerExpired(PeerId),
  /// A packet is recived from a peer
  PacketReceived(PeerId, Packet),
  /// A connectio to a peer is closed
  ConnectionClosed(PeerId),
}

/// Commands that can be sent to the swarm
#[derive(Debug)]
pub enum NetworkCommand {
  /// Send a packet to a peer
  SendPacket(PeerId, Packet),
  OpenStream(PeerId),
  /// Close a stream to a peer
  CloseStream(PeerId),
  /// Send a pairing protocol request (pairing module only)
  SendPairingProtocolRequest(PeerId, Packet),
  /// Send a pairing protocol response (pairing module only)
  SendPairingProtocolResponse(ResponseChannel<Packet>, Packet),
  /// Update whitelist for a peer
  UpdateWhitelist(PeerId, bool),
}

#[derive(NetworkBehaviour)]
struct MulticonnectBehavior {
  mdns: mdns::tokio::Behaviour,
  pairing_protocol: request_response::Behaviour<PairingCodec>,
  stream_protocol: protocols::StreamProtocolBehavior,
}

impl MulticonnectBehavior {
  pub fn new(key: &libp2p::identity::Keypair) -> Result<Self, Box<dyn Error>> {
    let pairing_protocol = request_response::Behaviour::<PairingCodec>::new(
      vec![("/mc-pairing/0.0.1".into(), ProtocolSupport::Full)],
      Config::default(),
    );

    let stream_protocol = StreamProtocolBehavior::new();

    let mdns_config: mdns::Config = mdns::Config {
      ttl: Duration::from_secs(4),
      query_interval: std::time::Duration::from_secs(1),
      ..Default::default()
    };

    let mdns = mdns::tokio::Behaviour::new(mdns_config, key.public().to_peer_id())?;

    Ok(Self { mdns, pairing_protocol, stream_protocol })
  }
}

pub struct NetworkManager {
  send_command_tx: mpsc::Sender<NetworkCommand>,
  send_command_rx: Option<mpsc::Receiver<NetworkCommand>>,

  network_event_rx: broadcast::Receiver<NetworkEvent>,
  network_event_tx: broadcast::Sender<NetworkEvent>,

  pairing_protocol_event_rx: Option<mpsc::Receiver<PairingProtocolEvent>>,
  pairing_protocol_event_tx: mpsc::Sender<PairingProtocolEvent>,

  keys: Keypair,
}

impl NetworkManager {
  pub async fn new() -> Self {
    let (send_command_tx, send_command_rx) = mpsc::channel::<NetworkCommand>(10_000);
    let (network_event_tx, network_event_rx) = broadcast::channel::<NetworkEvent>(10_000);
    let (pairing_protocol_event_tx, pairing_protocol_event_rx) = mpsc::channel::<PairingProtocolEvent>(100);

    let keys = Self::get_keys().await.unwrap();

    Self {
      send_command_tx,
      send_command_rx: Some(send_command_rx),

      network_event_rx,
      network_event_tx,

      pairing_protocol_event_tx,
      pairing_protocol_event_rx: Some(pairing_protocol_event_rx),
      keys,
    }
  }
  pub async fn start(&mut self) -> Result<(), Box<dyn Error>> {
    let _ = tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).try_init();

    let mut command_rx = self.send_command_rx.take().unwrap();
    let network_event_tx = self.network_event_tx.clone();
    let pairing_protocol_event_tx = self.pairing_protocol_event_tx.clone();

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
              for (peer_id, multiaddr) in discoverd {
                if !discovered_peers.contains(&peer_id) {
                  discovered_peers.insert(peer_id);
                  swarm.behaviour_mut().stream_protocol.add_peer_address(peer_id, multiaddr);
                  info!("Discovered peer: {}", peer_id);
                  let _ = network_event_tx.send(NetworkEvent::PeerDiscoverd(peer_id));
                }
              }
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::Mdns(mdns::Event::Expired(expired))) => {
              for (peer_id, multiaddr) in expired {
                if discovered_peers.remove(&peer_id) {
                  swarm.behaviour_mut().stream_protocol.remove_peer_address(&peer_id);
                  info!("Expired peer: id = {}, multiaddr = {}", peer_id, multiaddr);
                  let _ = network_event_tx.send(NetworkEvent::PeerExpired(peer_id));
                }
              }
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::PairingProtocol(request_response::Event::Message { peer, message, .. })) => {
              match message {
                Message::Request { request, channel, .. } => {
                  debug!("Recv request");
                  if let Err(e) = pairing_protocol_event_tx.send(PairingProtocolEvent::RecvRequest(peer, request, channel)).await {
                    warn!("Error sending request on channel: {}", e);
                  }
                },
                Message::Response { response, .. } => {
                  debug!("Recv response");
                  if let Err(e) = pairing_protocol_event_tx.send(PairingProtocolEvent::RecvResponse(peer, response)).await {
                    warn!("Error sending response on channel: {}", e);
                  }
                }
              }
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::StreamProtocol(BehaviourEvent::PacketReceived { peer_id, packet })) => {
              trace!("Received multiconnect protocol event from {}", peer_id);
              if let Err(e) = network_event_tx.send(NetworkEvent::PacketReceived(peer_id, packet)) {
                warn!("Error sending packet received event: {}", e);
              }
            }
            _ => {
              trace!("Event: {:?}", event);
            },
          },
          cmd = command_rx.recv() => if let Some(cmd) = cmd {
            Self::process_command(&mut swarm, cmd);

            while let Ok(cmd) = command_rx.try_recv() {
              debug!("Processing batch command");
              Self::process_command(&mut swarm, cmd);
            }
          },
        }
      }
    });

    Ok(())
  }

  fn process_command(swarm: &mut libp2p::Swarm<MulticonnectBehavior>, cmd: NetworkCommand) {
    match cmd {
      NetworkCommand::SendPacket(peer_id, packet) => {
        trace!("Sending {:?} to {}", packet, peer_id);
        if let Err(e) = swarm.behaviour_mut().stream_protocol.send_packet(peer_id, packet) {
          error!("Error sending packet: {}", e);
        }
      }
      NetworkCommand::OpenStream(peer_id) => {
        debug!("Opening stream to {}", peer_id);
        if let Err(e) = swarm.behaviour_mut().stream_protocol.open_stream(peer_id) {
          error!("Error opening stream: {}", e);
        }
      }
      NetworkCommand::CloseStream(peer_id) => {
        debug!("Closing stream for {}", peer_id);
        if let Err(e) = swarm.behaviour_mut().stream_protocol.close_stream(peer_id) {
          error!("Error closing stream: {}", e);
        }
      }
      NetworkCommand::SendPairingProtocolRequest(peer_id, packet) => {
        debug!("Sending pairing protocol request to {}", peer_id);
        swarm.behaviour_mut().pairing_protocol.send_request(&peer_id, packet);
      }
      NetworkCommand::SendPairingProtocolResponse(channel, packet) =>
        if channel.is_open() {
          debug!("Sending pairing protocol response");
          if let Err(_packet) = swarm.behaviour_mut().pairing_protocol.send_response(channel, packet) {
            error!("Error sending pairing protocol response");
          }
        } else {
          warn!("Cannot send response on closed channel");
        },
      NetworkCommand::UpdateWhitelist(peer_id, is_whitelisted) => {
        debug!("Updating whitelist for {}: {}", peer_id, is_whitelisted);
        if is_whitelisted {
          swarm.behaviour_mut().stream_protocol.add_whitelisted_peer(&peer_id);
        } else {
          swarm.behaviour_mut().stream_protocol.remove_peer_address(&peer_id);
        }
      }
    }
  }
  /**
   * Get saved keys or generate new ones if they don't exist
   */
  async fn get_keys() -> std::io::Result<Keypair> {
    let cfg = CONFIG.get().unwrap();
    let mut path = cfg.read().await.get_config_dir().clone();
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

  pub fn recv_event_channel(&self) -> broadcast::Receiver<NetworkEvent> {
    self.network_event_rx.resubscribe()
  }

  pub fn get_mc_event_recv(&mut self) -> Option<mpsc::Receiver<PairingProtocolEvent>> {
    if let Some(ch) = self.pairing_protocol_event_rx.take() {
      return Some(ch);
    }
    warn!("Attempted to get channel when it has already been taken");
    None
  }
}
#[cfg(test)]
mod tests {}
