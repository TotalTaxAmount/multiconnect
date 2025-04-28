// TODO:
// - NetworkBehavior
// - ConnectionHandler
// - Todo: Error type

use std::{
  collections::{HashMap, VecDeque},
  fmt::Debug,
  io::ErrorKind,
  task::Poll,
};

use libp2p::{
  core::UpgradeInfo,
  futures::{self, AsyncRead, AsyncWrite, AsyncWriteExt},
  swarm::{ConnectionHandler, ConnectionHandlerEvent, NetworkBehaviour, SubstreamProtocol, ToSwarm},
  InboundUpgrade, OutboundUpgrade, PeerId,
};
use log::warn;
use multiconnect_protocol::Packet;

trait AsyncReadWrite: AsyncRead + AsyncWrite + Debug {}
impl<T: AsyncRead + AsyncWrite + ?Sized + Debug> AsyncReadWrite for T {}
type Stream = Box<dyn AsyncReadWrite + Send + Unpin>;

#[derive(Debug)]
pub enum BehaviourEvent {
  PacketRecived(PeerId, Packet),
  ConnectionOpenRequest(PeerId),
  ConnectionClosed(PeerId),
}

#[derive(Debug)]
pub enum HandlerCommand {
  OpenStream,
  SendPacket(Packet),
  CloseStream,
}

#[derive(Debug)]
pub enum HandlerEvent {
  InboundPacket(Packet),
  InboundStream(Stream),
  ConnectionClosed,
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
      }
      HandlerCommand::SendPacket(_packet) => {
        // TODO??
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
        self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(HandlerEvent::InboundStream(stream)));
      }
      libp2p::swarm::handler::ConnectionEvent::FullyNegotiatedOutbound(fno) => {
        let stream = fno.protocol;
        self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(HandlerEvent::InboundStream(stream)));
      }
      _ => {}
    }
  }
}

pub struct MulticonnectDataBehaviour {
  open_streams: HashMap<PeerId, Stream>,
  queued_events: VecDeque<ToSwarm<BehaviourEvent, HandlerCommand>>,
}

impl MulticonnectDataBehaviour {
  pub fn new() -> Self {
    Self { open_streams: HashMap::new(), queued_events: VecDeque::new() }
  }

  pub async fn open_stream(&mut self, peer_id: PeerId) -> std::io::Result<()> {
    self.queued_events.push_back(ToSwarm::NotifyHandler {
      peer_id,
      handler: libp2p::swarm::NotifyHandler::Any,
      event: HandlerCommand::OpenStream,
    });
    Ok(())
  }

  pub async fn send_packet(&mut self, peer_id: PeerId, packet: Packet) -> std::io::Result<()> {
    if let Some(s) = self.open_streams.get_mut(&peer_id) {
      let bytes = Packet::to_bytes(&packet).unwrap();
      s.write_all(&bytes).await;
      Ok(())
    } else {
      warn!("Appempted to send a packet to a peer that is not connected");
      Err(ErrorKind::HostUnreachable.into())
    }
  }

  pub async fn close_stream(&mut self, peer_id: PeerId) {
    self.open_streams.remove(&peer_id);
    self.queued_events.push_back(ToSwarm::GenerateEvent(BehaviourEvent::ConnectionClosed(peer_id)));
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
      HandlerEvent::InboundPacket(packet) => {
        self.queued_events.push_back(ToSwarm::GenerateEvent(BehaviourEvent::PacketRecived(peer_id, packet)));
      }
      HandlerEvent::InboundStream(stream) => {
        self.open_streams.remove(&peer_id);
        self.open_streams.insert(peer_id, stream);
      }
      HandlerEvent::ConnectionClosed => {
        self.open_streams.remove(&peer_id);
        self.queued_events.push_back(ToSwarm::GenerateEvent(BehaviourEvent::ConnectionClosed(peer_id)));
      }
    }
  }

  fn poll(
    &mut self,
    _: &mut std::task::Context<'_>,
  ) -> std::task::Poll<libp2p::swarm::ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>> {
    if let Some(e) = self.queued_events.pop_front() {
      Poll::Ready(e)
    } else {
      Poll::Pending
    }
  }
}
