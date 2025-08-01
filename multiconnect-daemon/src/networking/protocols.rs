// TODO:
// - NetworkBehavior
// - ConnectionHandler
// - Todo: Error type

use std::{
  collections::{HashMap, HashSet, VecDeque},
  fmt::Debug,
  future::Future,
  io::{self, Cursor},
  pin::Pin,
  sync::{atomic::AtomicBool, Arc},
  task::Poll,
  time::Duration,
  vec,
};

use async_trait::async_trait;
use bincode::de;
use libp2p::{
  core::UpgradeInfo,
  futures::{
    self,
    io::{ReadHalf, WriteHalf},
    pin_mut, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt,
  },
  request_response,
  swarm::{
    dial_opts::DialOpts, ConnectionHandler, ConnectionHandlerEvent, ConnectionId, NetworkBehaviour, SubstreamProtocol,
    ToSwarm,
  },
  InboundUpgrade, Multiaddr, OutboundUpgrade, PeerId,
};
use log::{debug, error, warn};
use multiconnect_protocol::Packet;
use thiserror::Error;
use tokio::{
  sync::{mpsc, oneshot},
  task::JoinHandle,
  time::timeout,
};

pub trait AsyncReadWrite: AsyncRead + AsyncWrite + Debug {}
impl<T: AsyncRead + AsyncWrite + ?Sized + Debug> AsyncReadWrite for T {}
type Stream = Box<dyn AsyncReadWrite + Send + Unpin>;

#[derive(Debug)]
pub enum HandlerCommand {
  OpenStream,
  CloseStream,
  SendPacket { packet: Packet },
  UpdateWhitelist { is_whitelisted: bool },
}

#[derive(Debug)]
pub enum HandlerEvent {
  PacketReceived { packet: Packet },
  StreamClosed { reason: StreamCloseReason },
  StreamOpened,
}

#[derive(Debug)]
pub enum StreamCloseReason {
  RemoteClosed,
  ReadError(String),
  WriteError(String),
  Timeout,
}

#[derive(Debug)]
pub enum ConnectionState {
  Disconnected,
  Dialing,
  Connected { connection_id: ConnectionId },
  Opening { connection_id: ConnectionId },
  Open { connection_id: ConnectionId },
}

#[derive(Debug)]
pub enum BehaviourEvent {
  PacketReceived { peer_id: PeerId, packet: Packet },
  StreamClosed { peer_id: PeerId, reason: StreamCloseReason },
  StreamOpend { peer_id: PeerId },
}

#[derive(Error, Debug)]
pub enum StreamProtocolError {
  #[error("Connection error: {0}")]
  ConnectionError(String),
  #[error("IO error: {0}")]
  Io(#[from] io::Error),
}

#[derive(Debug)]
pub struct StreamProtocol;

impl UpgradeInfo for StreamProtocol {
  type Info = String;

  type InfoIter = std::iter::Once<Self::Info>;

  fn protocol_info(&self) -> Self::InfoIter {
    std::iter::once("/stream-protocol/0.0.1".to_string())
  }
}

impl<TSocket> InboundUpgrade<TSocket> for StreamProtocol
where
  TSocket: AsyncRead + AsyncWrite + Send + Unpin + Debug + 'static,
{
  type Output = Stream;

  type Error = StreamProtocolError;

  type Future = futures::future::Ready<Result<Self::Output, Self::Error>>;

  fn upgrade_inbound(self, socket: TSocket, _info: Self::Info) -> Self::Future {
    futures::future::ready(Ok(Box::new(socket)))
  }
}

impl<TSocket> OutboundUpgrade<TSocket> for StreamProtocol
where
  TSocket: AsyncRead + AsyncWrite + Send + Unpin + Debug + 'static,
{
  type Output = Stream;

  type Error = StreamProtocolError;

  type Future = futures::future::Ready<Result<Self::Output, Self::Error>>;

  fn upgrade_outbound(self, socket: TSocket, _info: Self::Info) -> Self::Future {
    futures::future::ready(Ok(Box::new(socket)))
  }
}

pub struct StreamProtocolConnectionHandler {
  pending_events: VecDeque<ConnectionHandlerEvent<StreamProtocol, (), HandlerEvent>>,
  pending_writes: VecDeque<Cursor<Vec<u8>>>,
  write_half: Option<WriteHalf<Stream>>,
  reader_handle: Option<JoinHandle<()>>,
  close_rx: Option<oneshot::Receiver<StreamCloseReason>>,
  packet_recv_tx: mpsc::Sender<Packet>,
  packet_recv_rx: mpsc::Receiver<Packet>,
  is_stream_open: bool,
  is_whitelisted: bool,
}

impl StreamProtocolConnectionHandler {
  fn new() -> Self {
    let (packet_recv_tx, packet_recv_rx) = mpsc::channel(10);
    Self {
      pending_events: VecDeque::new(),
      pending_writes: VecDeque::new(),
      write_half: None,
      close_rx: None,
      reader_handle: None,
      packet_recv_tx,
      packet_recv_rx,
      is_stream_open: false,
      is_whitelisted: false,
    }
  }

  fn start_reader(&mut self, read_half: ReadHalf<Stream>) {
    let packet_recv_tx = self.packet_recv_tx.clone();
    let (close_tx, close_rx) = oneshot::channel::<StreamCloseReason>();

    self.close_rx = Some(close_rx);
    self.is_stream_open = true;

    self.reader_handle = Some(tokio::spawn(async move {
      debug!("Starting stream reader");
      let close_reason = Self::read_loop(read_half, packet_recv_tx).await;
      let _ = close_tx.send(close_reason);
      debug!("Stream reader finished")
    }));
  }

  async fn read_loop(mut read_half: ReadHalf<Stream>, packet_recv_tx: mpsc::Sender<Packet>) -> StreamCloseReason {
    loop {
      // let len_result = timeout(Duration::from_secs(10), async {
      let mut buf = [0u8; 2];
      let _ = read_half.read_exact(&mut buf).await;
      let len = u16::from_be_bytes(buf);
      // })
      // .await;

      // let len = match len_result {
      //   Ok(Ok(len)) => len,
      //   Ok(Err(e)) => {
      //     error!("Read error: {}", e);
      //     return StreamCloseReason::ReadError(e.to_string());
      //   }
      //   Err(_) => {
      //     warn!("Read timeout");
      //     return StreamCloseReason::Timeout;
      //   }
      // };

      if len == 0 {
        debug!("Received zero-length packet, closing stream");
        return StreamCloseReason::RemoteClosed;
      }

      if len > u16::MAX - 2 {
        error!("Packet too large: {} bytes", len);
        return StreamCloseReason::ReadError(format!("Packet too large: {}", len));
      }

      // Read packet data
      let packet_result = timeout(Duration::from_secs(10), async {
        let mut buf = vec![0u8; len as usize];
        read_half.read_exact(&mut buf).await?;
        Ok::<Vec<u8>, io::Error>(buf)
      })
      .await;

      let buf = match packet_result {
        Ok(Ok(buf)) => buf,
        Ok(Err(e)) => {
          error!("Read error: {}", e);
          return StreamCloseReason::ReadError(e.to_string());
        }
        Err(_) => {
          warn!("Read timeout");
          return StreamCloseReason::Timeout;
        }
      };

      match Packet::from_bytes(&buf) {
        Ok(packet) => {
          debug!("Received packet: {:?}", packet);
          if packet_recv_tx.send(packet).await.is_err() {
            debug!("Packet receiver dropped, closing reader");
            return StreamCloseReason::RemoteClosed;
          }
        }
        Err(e) => {
          error!("Error decoding packet: {}", e);
          return StreamCloseReason::ReadError(format!("Decode error: {}", e));
        }
      }
    }
  }

  fn cleanup(&mut self, reason: StreamCloseReason) {
    debug!("Cleaning up stream");
    self.pending_writes.clear();
    self.write_half = None;
    self.is_stream_open = false;
    if let Some(handle) = self.reader_handle.take() {
      handle.abort();
    }
    self.close_rx = None;
    self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(HandlerEvent::StreamClosed { reason }));
  }
}

impl ConnectionHandler for StreamProtocolConnectionHandler {
  type FromBehaviour = HandlerCommand;

  type ToBehaviour = HandlerEvent;

  type InboundProtocol = StreamProtocol;

  type OutboundProtocol = StreamProtocol;

  type InboundOpenInfo = ();

  type OutboundOpenInfo = ();

  fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, ()> {
    SubstreamProtocol::new(StreamProtocol, ())
  }

  fn poll(
    &mut self,
    cx: &mut std::task::Context<'_>,
  ) -> Poll<ConnectionHandlerEvent<Self::OutboundProtocol, (), Self::ToBehaviour>> {
    // Check for pending events
    if let Some(event) = self.pending_events.pop_front() {
      return Poll::Ready(event);
    }

    // Check for stream close
    if let Some(rx) = &mut self.close_rx {
      match Pin::new(rx).poll(cx) {
        Poll::Ready(Ok(reason)) => {
          self.cleanup(reason);
          return Poll::Pending; // cleanup() queued the event
        }
        Poll::Ready(Err(_)) => {
          self.cleanup(StreamCloseReason::ReadError("Close channel dropped".to_string()));
          return Poll::Pending;
        }
        Poll::Pending => {}
      }
    }

    // Handle pending writes
    if let (Some(cursor), Some(w_half)) = (&mut self.pending_writes.get_mut(0), &mut self.write_half) {
      let buf = &cursor.get_ref()[cursor.position() as usize..];
      match Pin::new(w_half).poll_write(cx, buf) {
        Poll::Ready(Ok(n)) => {
          cursor.set_position(cursor.position() + n as u64);
          if cursor.position() as usize >= cursor.get_ref().len() {
            self.pending_writes.pop_front();
          } else {
            return Poll::Pending; // Wait
          }
        }
        Poll::Ready(Err(e)) => {
          error!("Write failed: {}", e);
          self.cleanup(StreamCloseReason::WriteError(e.to_string()));
        }
        Poll::Pending => return Poll::Pending,
      }
    }

    // Check for received packets
    let recv_fut = self.packet_recv_rx.recv();
    pin_mut!(recv_fut);

    // Maybe a little jank
    match recv_fut.poll(cx) {
      Poll::Ready(Some(packet)) => {
        Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(HandlerEvent::PacketReceived { packet }))
      }
      Poll::Pending => Poll::Pending,
      Poll::Ready(None) => {
        // self.cleanup(StreamCloseReason::RemoteClosed); // Doesnt work
        Poll::Pending
      }
    }
  }

  fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
    match event {
      HandlerCommand::OpenStream => {
        if self.reader_handle.is_none() {
          debug!("Handler opening stream");
          self.pending_events.push_back(ConnectionHandlerEvent::OutboundSubstreamRequest {
            protocol: SubstreamProtocol::new(StreamProtocol, ()),
          });
        } else {
          warn!("Stream already open, ignoring open request");
        }
      }
      HandlerCommand::CloseStream => {
        debug!("Closing stream");
        self.cleanup(StreamCloseReason::RemoteClosed);
      }
      HandlerCommand::SendPacket { packet } => {
        if self.write_half.is_some() {
          let bytes = Packet::to_bytes(&packet);
          match bytes {
            Ok(b) => self.pending_writes.push_back(Cursor::new(b)),
            Err(e) => {
              error!("Error encoding packet: {}", e);
            }
          }
        } else {
          warn!("Cannot send packet: stream not open");
        }
      }
      HandlerCommand::UpdateWhitelist { is_whitelisted } => {
        debug!("Updating whitelist status to: {}", is_whitelisted);
        self.is_whitelisted = is_whitelisted;
      }
    }
  }

  fn on_connection_event(
    &mut self,
    event: libp2p::swarm::handler::ConnectionEvent<Self::InboundProtocol, Self::OutboundProtocol, (), ()>,
  ) {
    match event {
      // TODO: Be able to accept/deny streams
      libp2p::swarm::handler::ConnectionEvent::FullyNegotiatedInbound(e) => {
        if self.is_whitelisted {
          debug!("Inbound stream established");
          let stream = e.protocol;
          let (read_half, write_half) = (stream as Stream).split();
          self.start_reader(read_half);
          self.write_half = Some(write_half);

          self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(HandlerEvent::StreamOpened));
        } else {
          warn!("Denied non-whitelisted stream open request");
        }
      }
      libp2p::swarm::handler::ConnectionEvent::FullyNegotiatedOutbound(e) => {
        debug!("Outbound stream established");
        let stream = e.protocol;
        let (read_half, write_half) = (stream as Stream).split();
        self.start_reader(read_half);
        self.write_half = Some(write_half);

        self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(HandlerEvent::StreamOpened));
      }
      _ => {}
    }
  }

  fn connection_keep_alive(&self) -> bool {
    self.is_stream_open || !self.pending_events.is_empty()
  }
}

pub struct StreamProtocolBehavior {
  pending_events: VecDeque<ToSwarm<BehaviourEvent, HandlerCommand>>,
  connection_states: HashMap<PeerId, ConnectionState>,
  peer_addrs: HashMap<PeerId, Multiaddr>,
  whitelisted_peers: HashSet<PeerId>,
}

impl StreamProtocolBehavior {
  pub fn new() -> Self {
    Self {
      pending_events: VecDeque::new(),
      peer_addrs: HashMap::new(),
      connection_states: HashMap::new(),
      whitelisted_peers: HashSet::new(),
    }
  }

  pub fn add_peer_address(&mut self, peer_id: PeerId, addr: Multiaddr) {
    self.peer_addrs.insert(peer_id, addr);
  }

  pub fn remove_peer_address(&mut self, peer_id: &PeerId) {
    self.peer_addrs.remove(peer_id);
  }

  pub fn add_whitelisted_peer(&mut self, peer_id: &PeerId) {
    self.whitelisted_peers.insert(*peer_id);
  }

  pub fn remove_whitelisted_peer(&mut self, peer_id: &PeerId) {
    self.whitelisted_peers.remove(peer_id);
  }

  pub fn send_packet(&mut self, peer_id: PeerId, packet: Packet) -> Result<(), StreamProtocolError> {
    if let Some(ConnectionState::Open { connection_id }) = self.connection_states.get(&peer_id) {
      debug!("Sending packet to {}: {:?}", peer_id, packet);
      self.pending_events.push_back(ToSwarm::NotifyHandler {
        peer_id,
        handler: libp2p::swarm::NotifyHandler::One(*connection_id),
        event: HandlerCommand::SendPacket { packet },
      });
      Ok(())
    } else {
      error!("No active stream to peer {}", peer_id);
      Err(StreamProtocolError::ConnectionError("No active stream for peer".to_string()))
    }
  }

  pub fn open_stream(&mut self, peer_id: PeerId) -> Result<(), StreamProtocolError> {
    debug!("Attemping to open a stream to {}", peer_id);

    match self.connection_states.get(&peer_id) {
      Some(ConnectionState::Open { .. }) => {
        warn!("Stream to {} already open", peer_id);
        Ok(())
      }
      Some(ConnectionState::Opening { .. }) => {
        debug!("Stream to {} is already being opened", peer_id);
        Ok(())
      }
      Some(ConnectionState::Connected { connection_id }) => {
        debug!("Opening stream to {}", peer_id);
        self.pending_events.push_back(ToSwarm::NotifyHandler {
          peer_id,
          handler: libp2p::swarm::NotifyHandler::One(*connection_id),
          event: HandlerCommand::OpenStream,
        });
        self.connection_states.insert(peer_id, ConnectionState::Opening { connection_id: *connection_id });
        Ok(())
      }
      Some(ConnectionState::Dialing) => {
        debug!("Already dialing {}", peer_id);
        Ok(())
      }
      Some(ConnectionState::Disconnected) | None => {
        if let Some(addr) = self.peer_addrs.get(&peer_id) {
          debug!("Dialing {} at {}", peer_id, addr);
          self
            .pending_events
            .push_back(ToSwarm::Dial { opts: DialOpts::peer_id(peer_id).addresses(vec![addr.clone()]).build() });
          self.connection_states.insert(peer_id, ConnectionState::Dialing);
          Ok(())
        } else {
          Err(StreamProtocolError::ConnectionError(format!("No know address for {}", peer_id)))
        }
      }
      _ => {
        // debug!("No state for {}", peer_id);
        todo!()

        // Err(StreamProtocolError::ConnectionError("".to_string()))
      }
    }
  }

  pub fn close_stream(&mut self, peer_id: PeerId) -> Result<(), StreamProtocolError> {
    match self.connection_states.get(&peer_id) {
      Some(ConnectionState::Open { connection_id }) => {
        self.pending_events.push_back(ToSwarm::NotifyHandler {
          peer_id,
          handler: libp2p::swarm::NotifyHandler::One(*connection_id),
          event: HandlerCommand::CloseStream,
        });
        Ok(())
      }
      Some(state) => {
        debug!("Cannot close stream to {}: not open (state: {:?})", peer_id, state);
        Err(StreamProtocolError::ConnectionError("Stream not open".to_string()))
      }
      None => Err(StreamProtocolError::ConnectionError("No stream state for peer".to_string())),
    }
  }

  pub fn get_connected_peers(&self) -> Vec<PeerId> {
    self
      .connection_states
      .iter()
      .filter_map(|(peer_id, state)| match state {
        &ConnectionState::Connected { .. } | &ConnectionState::Opening { .. } | &ConnectionState::Open { .. } => {
          Some(*peer_id)
        }
        _ => None,
      })
      .collect()
  }

  pub fn get_active_streams(&self) -> Vec<PeerId> {
    self
      .connection_states
      .iter()
      .filter_map(|(peer_id, state)| matches!(state, ConnectionState::Open { .. }).then_some(*peer_id))
      .collect()
  }

  pub fn is_stream_active(&self, peer_id: &PeerId) -> bool {
    matches!(self.connection_states.get(peer_id), Some(ConnectionState::Open { .. }))
  }

  pub fn get_stream_state(&self, peer_id: &PeerId) -> Option<&ConnectionState> {
    self.connection_states.get(peer_id)
  }
}

impl NetworkBehaviour for StreamProtocolBehavior {
  type ConnectionHandler = StreamProtocolConnectionHandler;

  type ToSwarm = BehaviourEvent;

  fn handle_established_inbound_connection(
    &mut self,
    _connection_id: libp2p::swarm::ConnectionId,
    _peer: PeerId,
    _local_addr: &libp2p::Multiaddr,
    _remote_addr: &libp2p::Multiaddr,
  ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
    debug!("Creating handler for inbound conneciton");
    Ok(StreamProtocolConnectionHandler::new())
  }

  fn handle_established_outbound_connection(
    &mut self,
    _connection_id: libp2p::swarm::ConnectionId,
    _peer: PeerId,
    _addr: &libp2p::Multiaddr,
    _role_override: libp2p::core::Endpoint,
    _port_use: libp2p::core::transport::PortUse,
  ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
    debug!("Creating handler for outbound conneciton");
    Ok(StreamProtocolConnectionHandler::new())
  }

  fn on_swarm_event(&mut self, event: libp2p::swarm::FromSwarm) {
    match event {
      libp2p::swarm::FromSwarm::ConnectionEstablished(connection_established) => {
        debug!("Connection established to: {}", connection_established.peer_id);
        let peer_id = connection_established.peer_id;
        let connection_id = connection_established.connection_id;

        if self.whitelisted_peers.contains(&peer_id) {
          self.pending_events.push_back(ToSwarm::NotifyHandler {
            peer_id,
            handler: libp2p::swarm::NotifyHandler::One(connection_id),
            event: HandlerCommand::UpdateWhitelist { is_whitelisted: true },
          });
        }

        match self.connection_states.get(&peer_id) {
          Some(ConnectionState::Dialing) => {
            // We where dialing this before
            self.pending_events.push_back(ToSwarm::NotifyHandler {
              peer_id,
              handler: libp2p::swarm::NotifyHandler::One(connection_id),
              event: HandlerCommand::OpenStream,
            });
          }
          _ => {
            self.connection_states.insert(peer_id, ConnectionState::Connected { connection_id });
          }
        }
      }
      libp2p::swarm::FromSwarm::ConnectionClosed(connection_closed) => {
        let peer_id = connection_closed.peer_id;
        debug!("Connection to {} closed", peer_id);

        if self.connection_states.contains_key(&peer_id) {
          self.connection_states.insert(peer_id, ConnectionState::Disconnected);
        }
      }
      _ => {}
    }
  }

  fn on_connection_handler_event(
    &mut self,
    peer_id: PeerId,
    connection_id: libp2p::swarm::ConnectionId,
    event: libp2p::swarm::THandlerOutEvent<Self>,
  ) {
    match event {
      HandlerEvent::PacketReceived { packet } => {
        debug!("Packet recivied: {:?}", packet);
        self.pending_events.push_back(ToSwarm::GenerateEvent(BehaviourEvent::PacketReceived { peer_id, packet }));
      }
      HandlerEvent::StreamOpened => {
        debug!("Stream to {} opened", peer_id);
        self.connection_states.insert(peer_id, ConnectionState::Open { connection_id });
        self.pending_events.push_back(ToSwarm::GenerateEvent(BehaviourEvent::StreamOpend { peer_id }));
      }
      HandlerEvent::StreamClosed { reason } => {
        debug!("Stream to {} closed: {:?}", peer_id, reason);
        match self.connection_states.get(&peer_id) {
          Some(ConnectionState::Open { .. }) | Some(ConnectionState::Opening { .. }) => {
            self.connection_states.insert(peer_id, ConnectionState::Connected { connection_id });
          }
          _ => {
            warn!("Peer disconnected (last state: {:?})", self.connection_states.get(&peer_id));
            self.connection_states.insert(peer_id, ConnectionState::Disconnected);
          }
        }
        self.pending_events.push_back(ToSwarm::GenerateEvent(BehaviourEvent::StreamClosed { peer_id, reason }));
      }
    }
  }

  fn poll(
    &mut self,
    _cx: &mut std::task::Context<'_>,
  ) -> Poll<ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>> {
    if let Some(event) = self.pending_events.pop_front() {
      debug!("Poll ready (bh)");
      Poll::Ready(event)
    } else {
      Poll::Pending
    }
  }
}

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
