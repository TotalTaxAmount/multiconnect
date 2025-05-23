// TODO:
// - NetworkBehavior
// - ConnectionHandler
// - Todo: Error type

use std::{
  collections::{HashMap, VecDeque},
  fmt::Debug,
  io::{self, ErrorKind},
  pin::Pin,
  sync::Arc,
  task::Poll,
};

use async_trait::async_trait;
use libp2p::{
  core::UpgradeInfo,
  futures::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
  request_response,
  swarm::{ConnectionHandler, ConnectionHandlerEvent, NetworkBehaviour, SubstreamProtocol, ToSwarm},
  InboundUpgrade, OutboundUpgrade, PeerId,
};
use log::{debug, error, trace, warn};
use multiconnect_protocol::Packet;
use tokio::sync::{mpsc, Mutex};

pub trait AsyncReadWrite: AsyncRead + AsyncWrite + Debug {}
impl<T: AsyncRead + AsyncWrite + ?Sized + Debug> AsyncReadWrite for T {}
type Stream = Box<dyn AsyncReadWrite + Send + Unpin>;
type SharedStream = Arc<Mutex<Stream>>;

#[derive(Debug, Clone)]
pub enum BehaviourEvent {
  PacketRecived(PeerId, Packet),
  ConnectionOpenRequest(PeerId),
  ConnectionClosed(PeerId),
}

#[derive(Debug)]
pub enum HandlerCommand {
  OpenStream,
  CloseStream,
}

#[derive(Debug)]
pub enum HandlerEvent {
  PendingStream { stream: Stream, inbound: bool },
  ConnectionClosed,
  Wake,
}

pub struct Proto;

impl UpgradeInfo for Proto {
  type Info = String;

  type InfoIter = std::iter::Once<Self::Info>;

  fn protocol_info(&self) -> Self::InfoIter {
    std::iter::once("/mc-proto/0.0.1".to_string())
  }
}

impl<TSocket> InboundUpgrade<TSocket> for Proto
where
  TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static + Debug,
{
  type Output = Stream;

  type Error = std::io::Error;

  type Future = futures::future::Ready<Result<Self::Output, Self::Error>>;

  fn upgrade_inbound(self, socket: TSocket, _: Self::Info) -> Self::Future {
    futures::future::ready(Ok(Box::new(socket)))
  }
}

impl<TSocket> OutboundUpgrade<TSocket> for Proto
where
  TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static + Debug,
{
  type Output = Stream;

  type Error = std::io::Error;

  type Future = futures::future::Ready<Result<Self::Output, Self::Error>>;

  fn upgrade_outbound(self, socket: TSocket, _: Self::Info) -> Self::Future {
    futures::future::ready(Ok(Box::new(socket)))
  }
}

pub struct StreamConnectionHandler {
  pending_open: bool,
  pending_events: VecDeque<ConnectionHandlerEvent<Proto, (), HandlerEvent>>,
}

impl StreamConnectionHandler {
  pub fn new() -> Self {
    Self { pending_open: false, pending_events: VecDeque::new() }
  }
}

impl ConnectionHandler for StreamConnectionHandler {
  type FromBehaviour = HandlerCommand;

  type ToBehaviour = HandlerEvent;

  type InboundProtocol = Proto;

  type OutboundProtocol = Proto;

  type InboundOpenInfo = ();

  type OutboundOpenInfo = ();

  fn listen_protocol(&self) -> libp2p::swarm::SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
    SubstreamProtocol::new(Proto, ())
  }

  fn poll(
    &mut self,
    _cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<
    libp2p::swarm::ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
  > {
    if let Some(e) = self.pending_events.pop_front() {
      Poll::Ready(e)
    } else if self.pending_open {
      debug!("Sending outbount substream request");
      self.pending_open = false;
      Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest { protocol: SubstreamProtocol::new(Proto, ()) })
    } else {
      Poll::Pending
    }
  }

  fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
    match event {
      HandlerCommand::OpenStream => {
        self.pending_open = true;
        self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(HandlerEvent::Wake));
      }
      HandlerCommand::CloseStream => todo!(),
    }
  }

  fn on_connection_event(
    &mut self,
    event: libp2p::swarm::handler::ConnectionEvent<
      Self::InboundProtocol,
      Self::OutboundProtocol,
      Self::InboundOpenInfo,
      Self::OutboundOpenInfo,
    >,
  ) {
    match event {
      libp2p::swarm::handler::ConnectionEvent::FullyNegotiatedInbound(fni) => {
        let stream = fni.protocol;
        self
          .pending_events
          .push_back(ConnectionHandlerEvent::NotifyBehaviour(HandlerEvent::PendingStream { stream, inbound: true }));
      }
      libp2p::swarm::handler::ConnectionEvent::FullyNegotiatedOutbound(fno) => {
        let stream = fno.protocol;
        self
          .pending_events
          .push_back(ConnectionHandlerEvent::NotifyBehaviour(HandlerEvent::PendingStream { stream, inbound: false }));
      }
      _ => {}
    }
  }

  fn poll_close(&mut self, _: &mut std::task::Context<'_>) -> Poll<Option<Self::ToBehaviour>> {
    Poll::Ready(None)
  }
}

pub struct MulticonnectDataBehaviour {
  open_streams: HashMap<PeerId, SharedStream>,
  pending_streams: HashMap<PeerId, Stream>,
  queued_events: VecDeque<ToSwarm<BehaviourEvent, HandlerCommand>>,
  packet_rec_rx: mpsc::Receiver<(PeerId, Packet)>,
  packet_rec_tx: mpsc::Sender<(PeerId, Packet)>,

  control_tx: mpsc::Sender<BehaviourEvent>,
  control_rx: mpsc::Receiver<BehaviourEvent>,
}

impl MulticonnectDataBehaviour {
  pub fn new() -> Self {
    let (tx, rx) = mpsc::channel(10);
    let (control_tx, control_rx) = mpsc::channel(10);
    Self {
      open_streams: HashMap::new(),
      pending_streams: HashMap::new(),
      queued_events: VecDeque::new(),
      packet_rec_rx: rx,
      packet_rec_tx: tx,
      control_tx,
      control_rx,
    }
  }

  pub fn open_stream(&mut self, peer_id: PeerId) -> std::io::Result<()> {
    self.queued_events.push_back(ToSwarm::NotifyHandler {
      peer_id,
      handler: libp2p::swarm::NotifyHandler::Any,
      event: HandlerCommand::OpenStream,
    });
    Ok(())
  }

  pub fn check_connection(&self, peer_id: &PeerId) -> bool {
    self.open_streams.contains_key(peer_id)
  }

  pub async fn send_packet(&mut self, peer_id: &PeerId, packet: Packet) -> std::io::Result<()> {
    if let Some(s) = self.open_streams.get_mut(peer_id) {
      let bytes = Packet::to_bytes(&packet).unwrap();
      debug!("Sending packet to {}", peer_id);
      let mut guard = s.lock().await;
      debug!("Got lock to  send to {}", peer_id);
      guard.write_all(&bytes).await?;
      debug!("Wrote");
      guard.flush().await?;
      debug!("Flushed");
      Ok(())
    } else {
      warn!("Attempted to send a packet to a peer that is not connected");
      Err(ErrorKind::HostUnreachable.into())
    }
  }

  pub fn close_stream(&mut self, peer_id: PeerId) {
    self.open_streams.remove(&peer_id);
    self.queued_events.push_back(ToSwarm::GenerateEvent(BehaviourEvent::ConnectionClosed(peer_id)));
  }

  pub fn approve_inbound_stream(&mut self, peer_id: PeerId) {
    if let Some(s) = self.pending_streams.remove(&peer_id) {
      debug!("Approving stream request from: {}", peer_id);
      let shared = Arc::new(Mutex::new(s));
      self.spawn_handler(shared, &peer_id);
    } else {
      warn!("Attempted to approve a non-pending stream");
    }
  }

  pub fn deny_inbound_stream(&mut self, peer_id: &PeerId) {
    // TODO: Maybe send something for graceful close?
    self.pending_streams.remove(peer_id);
    debug!("Denied stream request from: {}", peer_id);
  }

  fn spawn_handler(&mut self, stream: SharedStream, peer_id: &PeerId) {
    debug!("Starting stream handler for {}", peer_id);
    let tx = self.packet_rec_tx.clone();

    let control_tx = self.control_tx.clone();
    self.open_streams.insert(peer_id.clone(), stream.clone());

    let peer_id = peer_id.clone();
    tokio::spawn(async move {
      loop {
        let mut buf = [0u8; 2];
        if let Err(e) = stream.lock().await.read_exact(&mut buf).await {
          error!("Error reading stream: {}", e);
        };
        let len = u16::from_be_bytes(buf);
        let mut buf = vec![0u8; len as usize];
        debug!("Packet len: {}", len);

        if len == 0 {
          warn!("Stream closed");
          let _ = control_tx.send(BehaviourEvent::ConnectionClosed(peer_id)).await;
          break;
        }

        match stream.lock().await.read_exact(&mut buf).await {
          Ok(_) => {
            debug!("Decoding packet");
            let packet = match Packet::from_bytes(&buf) {
              Ok(p) => p,
              Err(e) => {
                error!("Error decoding packet: {}", e);
                break;
              }
            };

            trace!("Received {:?} from peer", packet);
            let _ = tx.send((peer_id.clone(), packet)).await;
          }
          Err(e) => {
            error!("Read error: {}", e);
            break;
          }
        }
      }
    });
  }
}

impl NetworkBehaviour for MulticonnectDataBehaviour {
  type ConnectionHandler = StreamConnectionHandler;

  type ToSwarm = BehaviourEvent;

  fn handle_established_inbound_connection(
    &mut self,
    _: libp2p::swarm::ConnectionId,
    _: libp2p::PeerId,
    _: &libp2p::Multiaddr,
    _: &libp2p::Multiaddr,
  ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
    debug!("Creating inbound conneciton handler");
    Ok(StreamConnectionHandler::new())
  }

  fn handle_established_outbound_connection(
    &mut self,
    _: libp2p::swarm::ConnectionId,
    _: libp2p::PeerId,
    _: &libp2p::Multiaddr,
    _: libp2p::core::Endpoint,
    _: libp2p::core::transport::PortUse,
  ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
    debug!("Creating outbound connection handler");
    Ok(StreamConnectionHandler::new())
  }

  fn on_swarm_event(&mut self, _: libp2p::swarm::FromSwarm) {}

  fn on_connection_handler_event(
    &mut self,
    peer_id: libp2p::PeerId,
    _connection_id: libp2p::swarm::ConnectionId,
    event: libp2p::swarm::THandlerOutEvent<Self>,
  ) {
    match event {
      HandlerEvent::PendingStream { stream, inbound: false } => {
        let shared = Arc::new(Mutex::new(stream));
        debug!("I am the requester");
        self.spawn_handler(shared, &peer_id);
      }
      HandlerEvent::PendingStream { stream, inbound: true } => {
        self.pending_streams.insert(peer_id, stream);
        self.queued_events.push_back(ToSwarm::GenerateEvent(BehaviourEvent::ConnectionOpenRequest(peer_id)));
      }
      HandlerEvent::ConnectionClosed => {
        self.open_streams.remove(&peer_id);
        self.pending_streams.remove(&peer_id);
        self.queued_events.push_back(ToSwarm::GenerateEvent(BehaviourEvent::ConnectionClosed(peer_id)));
      }
      HandlerEvent::Wake => {}
    }
  }

  fn poll(
    &mut self,
    cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<libp2p::swarm::ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>> {
    while let Poll::Ready(Some((peer_id, packet))) = Pin::new(&mut self.packet_rec_rx).poll_recv(cx) {
      self.queued_events.push_back(ToSwarm::GenerateEvent(BehaviourEvent::PacketRecived(peer_id, packet)));
    }

    while let Poll::Ready(Some(event)) = Pin::new(&mut self.control_rx).poll_recv(cx) {
      if let BehaviourEvent::ConnectionClosed(peer_id) = event.clone() {
        // TODO: This is stupid
        self.open_streams.remove(&peer_id);
      }
      self.queued_events.push_back(ToSwarm::GenerateEvent(event));
    }

    if let Some(e) = self.queued_events.pop_front() {
      // debug!("{:?}", e);
      return Poll::Ready(e);
    }

    Poll::Pending
  }
}

// TODO: Make this more confined to pairing requests and responses
#[derive(Clone, Copy, Default)]
pub struct PairingCodec;

#[async_trait]
impl request_response::Codec for PairingCodec {
  #[doc = " The type of protocol(s) or protocol versions being negotiated."]
  type Protocol = String;

  #[doc = " The type of inbound and outbound requests."]
  type Request = Packet;

  #[doc = " The type of inbound and outbound responses."]
  type Response = Packet;

  #[doc = " Reads a request from the given I/O stream according to the"]
  #[doc = " negotiated protocol."]
  async fn read_request<T>(&mut self, protocol: &Self::Protocol, io: &mut T) -> io::Result<Self::Request>
  where
    T: AsyncRead + Unpin + Send,
  {
    let mut len_buf = [0u8; 2];
    io.read_exact(&mut len_buf).await?;
    let len = u16::from_be_bytes(len_buf) as usize;
    debug!("Len: {}", len);

    let mut buf = vec![0u8; len];
    io.read_exact(&mut buf).await?;

    let packet = Packet::from_bytes(&buf).map_err(|e| io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    debug!("Read request {:?}", packet);
    Ok(packet)
  }

  #[doc = " Reads a response from the given I/O stream according to the"]
  #[doc = " negotiated protocol."]
  async fn read_response<T>(&mut self, _protocol: &Self::Protocol, io: &mut T) -> io::Result<Self::Response>
  where
    T: AsyncRead + Unpin + Send,
  {
    let mut len_buf = [0u8; 2];
    io.read_exact(&mut len_buf).await?;
    let len = u16::from_be_bytes(len_buf) as usize;
    debug!("Len: {}", len);

    let mut buf = vec![0u8; len];
    io.read_exact(&mut buf).await?;
    // debug!("Raw packet: {:?}", buf);

    let packet = Packet::from_bytes(&buf).map_err(|e| io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    debug!("Read response {:?}", packet);
    Ok(packet)
  }

  #[doc = " Writes a request to the given I/O stream according to the"]
  #[doc = " negotiated protocol."]
  async fn write_request<T>(&mut self, _protocol: &Self::Protocol, io: &mut T, req: Self::Request) -> io::Result<()>
  where
    T: AsyncWrite + Unpin + Send,
  {
    debug!("Write request {:?}", req);
    let bytes: Vec<u8> = Packet::to_bytes(&req).map_err(|e| io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    // debug!("Raw bytes: {:?}", bytes);
    io.write_all(&bytes).await?;
    io.flush().await
  }

  #[doc = " Writes a response to the given I/O stream according to the"]
  #[doc = " negotiated protocol."]
  async fn write_response<T>(&mut self, _protocol: &Self::Protocol, io: &mut T, res: Self::Response) -> io::Result<()>
  where
    T: AsyncWrite + Unpin + Send,
  {
    debug!("Write response {:?}", res);
    let bytes: Vec<u8> = Packet::to_bytes(&res).map_err(|e| io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    // debug!("Raw bytes: {:?}", bytes);
    io.write_all(&bytes).await?;
    io.flush().await
  }
}
