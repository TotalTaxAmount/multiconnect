// TODO:
// - NetworkBehavior
// - ConnectionHandler
// - Todo: Error type

use std::{
  collections::{HashMap, VecDeque},
  fmt::Debug,
  future::Future,
  io::{self, Cursor},
  pin::Pin,
  task::{Poll, Waker},
};

use async_trait::async_trait;
use libp2p::{
  core::UpgradeInfo,
  futures::{
    self,
    io::{ReadHalf, WriteHalf},
    pin_mut, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt,
  },
  request_response,
  swarm::{ConnectionHandler, ConnectionHandlerEvent, ConnectionId, NetworkBehaviour, SubstreamProtocol, ToSwarm},
  InboundUpgrade, OutboundUpgrade, PeerId,
};
use log::{debug, error, warn};
use multiconnect_protocol::Packet;
use thiserror::Error;
use tokio::sync::mpsc;

pub trait AsyncReadWrite: AsyncRead + AsyncWrite + Debug {}
impl<T: AsyncRead + AsyncWrite + ?Sized + Debug> AsyncReadWrite for T {}
type Stream = Box<dyn AsyncReadWrite + Send + Unpin>;

#[derive(Debug)]
pub enum HandlerCommand {
  OpenStream,
  CloseStream,
  SendPacket { packet: Packet },
}

#[derive(Debug)]
pub enum HandlerEvent {
  PacketReceived { packet: Packet },
  ConnectionClose,
}

#[derive(Debug)]
pub enum BehaviourEvent {
  PacketReceived { peer_id: PeerId, packet: Packet },
}

#[derive(Error, Debug)]
pub enum StreamProtocolError {
  #[error("Failed to upgrade")]
  UpgradeError,
}

#[derive(Debug)]
enum StreamStates {
  PendingOutbound,
  Active { peer_id: PeerId, stream: Stream },
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

  type Error = ();

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

  type Error = ();

  type Future = futures::future::Ready<Result<Self::Output, Self::Error>>;

  fn upgrade_outbound(self, socket: TSocket, _info: Self::Info) -> Self::Future {
    futures::future::ready(Ok(Box::new(socket)))
  }
}

pub struct StreamProtocolConnectionHandler {
  pending_events: VecDeque<ConnectionHandlerEvent<StreamProtocol, (), HandlerEvent>>,
  pending_write: Option<Cursor<Vec<u8>>>,
  write_half: Option<WriteHalf<Stream>>,
  packet_recv_tx: mpsc::Sender<Packet>,
  packet_recv_rx: mpsc::Receiver<Packet>,
}

impl StreamProtocolConnectionHandler {
  fn new() -> Self {
    let (packet_recv_tx, packet_recv_rx) = mpsc::channel(10);
    Self { pending_events: VecDeque::new(), pending_write: None, write_half: None, packet_recv_tx, packet_recv_rx }
  }

  fn spawn_reader(&mut self, mut read_half: ReadHalf<Stream>) {
    let packet_recv_tx = self.packet_recv_tx.clone();
    tokio::spawn(async move {
      debug!("Spawing reader");
      loop {
        // Read the packet len as a u16
        let mut buf = [0u8; 2];
        let _ = read_half.read_exact(&mut buf).await;
        let len = u16::from_be_bytes(buf);
        debug!("Packet len: {}", len);
        if len == 0 {
          warn!("Recivied packet with len 0");
          break;
        }

        // Read the packet
        let mut buf = vec![0u8; len.into()];
        let _ = read_half.read_exact(&mut buf).await;

        match Packet::from_bytes(&buf) {
          Ok(p) => {
            debug!("Recvied packet: {:?}", p);
            let _ = packet_recv_tx.send(p).await;
          }
          Err(e) => {
            error!("Error decoding packet: {}", e);
          }
        }
      }
    });
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
    if let Some(event) = self.pending_events.pop_front() {
      debug!("Poll ready");
      return Poll::Ready(event);
    }

    if let (Some(cursor), Some(w_half)) = (&mut self.pending_write, &mut self.write_half) {
      let buf = &cursor.get_ref()[cursor.position() as usize..];
      match Pin::new(w_half).poll_write(cx, buf) {
        Poll::Ready(Ok(n)) => {
          cursor.set_position(cursor.position() + n as u64);
          if cursor.position() as usize >= cursor.get_ref().len() {
            self.pending_write = None;
          } else {
            return Poll::Pending; // Wait
          }
        }
        Poll::Ready(Err(e)) => {
          error!("Write failed: {}", e);
          self.pending_write = None;
          self.write_half = None;
          return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(HandlerEvent::ConnectionClose));
        }
        Poll::Pending => return Poll::Pending,
      }
    }

    let recv_fut = self.packet_recv_rx.recv();
    pin_mut!(recv_fut);

    // Maybe a little jank
    match recv_fut.poll(cx) {
      Poll::Ready(Some(packet)) => {
        self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(HandlerEvent::PacketReceived { packet }));
        Poll::Ready(self.pending_events.pop_back().unwrap())
      }
      Poll::Pending => Poll::Pending,
      Poll::Ready(None) => {
        // ??
        Poll::Pending
      }
    }
  }

  fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
    match event {
      HandlerCommand::OpenStream => {
        debug!("Handler opening stream");
        self.pending_events.push_back(ConnectionHandlerEvent::OutboundSubstreamRequest {
          protocol: SubstreamProtocol::new(StreamProtocol, ()),
        });
      }
      HandlerCommand::CloseStream => {
        self.write_half = None;
      }
      HandlerCommand::SendPacket { packet } => {
        if self.pending_write.is_some() {
          warn!("Overwritting existing pending write");
        }
        let bytes = Packet::to_bytes(&packet);
        match bytes {
          Ok(b) => self.pending_write = Some(Cursor::new(b)),
          Err(e) => {
            error!("Error encoding packet: {}", e);
          }
        }
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
        debug!("FNI");
        let stream = e.protocol;
        let (read_half, write_half) = (stream as Stream).split();
        self.spawn_reader(read_half);
        self.write_half = Some(write_half);
      }
      libp2p::swarm::handler::ConnectionEvent::FullyNegotiatedOutbound(e) => {
        debug!("FNO");
        let stream = e.protocol;
        let (read_half, write_half) = (stream as Stream).split();
        self.spawn_reader(read_half);
        self.write_half = Some(write_half);
      }
      _ => {}
    }
  }

  fn connection_keep_alive(&self) -> bool {
    true
  }
}

pub struct StreamProtocolBehavior {
  pending_events: VecDeque<ToSwarm<BehaviourEvent, HandlerCommand>>,
  connections: HashMap<PeerId, ConnectionId>,
}

impl StreamProtocolBehavior {
  pub fn new() -> Self {
    Self { pending_events: VecDeque::new(), connections: HashMap::new() }
  }

  pub fn send_packet(&mut self, peer_id: PeerId, packet: Packet) {
    if let Some(c_id) = self.connections.get(&peer_id) {
      self.pending_events.push_back(ToSwarm::NotifyHandler {
        peer_id,
        handler: libp2p::swarm::NotifyHandler::One(*c_id),
        event: HandlerCommand::SendPacket { packet },
      });
    } else {
      error!("Failed to send packet (no connection)")
    }
  }

  pub fn open_stream(&mut self, peer_id: PeerId) {
    debug!("Attemping to open a stream to {}", peer_id);
    if let Some(c_id) = self.connections.get(&peer_id) {
      self.pending_events.push_back(ToSwarm::NotifyHandler {
        peer_id,
        handler: libp2p::swarm::NotifyHandler::One(*c_id),
        event: HandlerCommand::OpenStream,
      });
    } else {
      error!("Failed to open stream (no connection)");
    }
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
    Ok(StreamProtocolConnectionHandler::new())
  }

  fn on_swarm_event(&mut self, event: libp2p::swarm::FromSwarm) {
    match event {
      libp2p::swarm::FromSwarm::ConnectionEstablished(connection_established) => {
        debug!("Connection established to: {}", connection_established.peer_id);
        self.connections.insert(connection_established.peer_id, connection_established.connection_id);
      }
      libp2p::swarm::FromSwarm::ConnectionClosed(connection_closed) => {
        debug!("Connection to {} closed", connection_closed.peer_id);
        self.connections.remove(&connection_closed.peer_id);
      }
      _ => {}
    }
  }

  fn on_connection_handler_event(
    &mut self,
    peer_id: PeerId,
    _connection_id: libp2p::swarm::ConnectionId,
    event: libp2p::swarm::THandlerOutEvent<Self>,
  ) {
    match event {
      HandlerEvent::PacketReceived { packet } => {
        debug!("Packet recivied: {:?}", packet);
        self.pending_events.push_back(ToSwarm::GenerateEvent(BehaviourEvent::PacketReceived { peer_id, packet }));
      }
      HandlerEvent::ConnectionClose => {
        self
          .pending_events
          .push_back(ToSwarm::CloseConnection { peer_id, connection: libp2p::swarm::CloseConnection::All });
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
