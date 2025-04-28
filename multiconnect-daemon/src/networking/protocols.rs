// TODO:
// - NetworkBehavior
// - ConnectionHandler

use std::fmt::Debug;

use fern::Output;
use libp2p::{
  core::UpgradeInfo,
  futures, request_response,
  swarm::{handler::InboundUpgradeSend, ConnectionHandler, NetworkBehaviour, SubstreamProtocol},
  InboundUpgrade, OutboundUpgrade,
};
use multiconnect_protocol::Packet;
use tokio::io::{AsyncRead, AsyncWrite};

trait AsyncReadWrite: AsyncRead + AsyncWrite + Debug {}
impl<T: AsyncRead + AsyncWrite + ?Sized + Debug> AsyncReadWrite for T {}
type NegoiatedSubstream = Box<dyn AsyncReadWrite + Send>;

pub enum BehaviourEvent {}

#[derive(Debug)]
pub enum HandlerEvent {
  InboundPacket(Packet),
  InboundStream(NegoiatedSubstream),
}

pub struct Proto;

impl UpgradeInfo for Proto {
  type Info = String;

  type InfoIter = Vec<String>;

  fn protocol_info(&self) -> Self::InfoIter {
    vec!["/mc-proto/0.0.1".to_string()]
  }
}

impl InboundUpgrade<NegoiatedSubstream> for Proto {
  type Output = NegoiatedSubstream;

  type Error = std::io::Error;

  type Future = futures::future::Ready<Result<Self::Output, Self::Error>>;

  fn upgrade_inbound(self, socket: NegoiatedSubstream, _: Self::Info) -> Self::Future {
    futures::future::ready(Ok(socket))
  }
}

impl OutboundUpgrade<NegoiatedSubstream> for Proto {
  type Output = NegoiatedSubstream;

  type Error = std::io::Error;

  type Future = futures::future::Ready<Result<Self::Output, Self::Error>>;

  fn upgrade_outbound(self, socket: NegoiatedSubstream, _: Self::Info) -> Self::Future {
    futures::future::ready(Ok(socket))
  }
}

pub struct SteamConnectionHandler;
impl ConnectionHandler for SteamConnectionHandler {
  type FromBehaviour = ();

  type ToBehaviour = HandlerEvent;

  type InboundProtocol = SubstreamProtocol<Proto, ()>;

  type OutboundProtocol;

  type InboundOpenInfo;

  type OutboundOpenInfo;

  fn listen_protocol(&self) -> libp2p::swarm::SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
    todo!()
  }

  fn poll(
    &mut self,
    cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<
    libp2p::swarm::ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
  > {
    todo!()
  }

  fn on_behaviour_event(&mut self, _event: Self::FromBehaviour) {
    todo!()
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
    todo!()
  }
}

pub struct MulticonnectDataBehaviour;
impl NetworkBehaviour for MulticonnectDataBehaviour {
  type ConnectionHandler = SteamConnectionHandler;

  type ToSwarm = BehaviourEvent;

  fn handle_established_inbound_connection(
    &mut self,
    _connection_id: libp2p::swarm::ConnectionId,
    peer: libp2p::PeerId,
    local_addr: &libp2p::Multiaddr,
    remote_addr: &libp2p::Multiaddr,
  ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
    todo!()
  }

  fn handle_established_outbound_connection(
    &mut self,
    _connection_id: libp2p::swarm::ConnectionId,
    peer: libp2p::PeerId,
    addr: &libp2p::Multiaddr,
    role_override: libp2p::core::Endpoint,
    port_use: libp2p::core::transport::PortUse,
  ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
    todo!()
  }

  fn on_swarm_event(&mut self, event: libp2p::swarm::FromSwarm) {
    todo!()
  }

  fn on_connection_handler_event(
    &mut self,
    _peer_id: libp2p::PeerId,
    _connection_id: libp2p::swarm::ConnectionId,
    _event: libp2p::swarm::THandlerOutEvent<Self>,
  ) {
    todo!()
  }

  fn poll(
    &mut self,
    cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<libp2p::swarm::ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>> {
    todo!()
  }
}
