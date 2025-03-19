use std::{collections::HashMap, sync::Arc};

use libp2p::{PeerId, Swarm};
use multiconnect_protocol::{Device, Packet};
use tokio::sync::RwLock;

use crate::{MulticonnectBehavior, SharedDaemon};

pub trait McModule: Send + Sync {
  fn name(&self) -> &'static str;
  fn on_peer_packet(&mut self, source: &PeerId, packet: &Packet, ctx: &mut McContext);
  fn on_frontend_event(&mut self, packet: &Packet, ctx: &mut McContext);
}

pub struct McContext<'a> {
  behaviour: Arc<RwLock<&'a mut MulticonnectBehavior>>,
  daemon: SharedDaemon,
  devices: Arc<RwLock<HashMap<PeerId, Device>>>,
}

impl<'a> McContext<'a> {
  pub fn new(
    behaviour: Arc<RwLock<&'a mut MulticonnectBehavior>>,
    daemon: SharedDaemon,
    devices: Arc<RwLock<HashMap<PeerId, Device>>>,
  ) -> Self {
    Self { behaviour, daemon, devices }
  }

  fn send_to_peer(&mut self, peer: &PeerId, packet: Packet) {
    let _ = self.behaviour.blocking_write().packet_protocol.send_request(&peer, packet);
  }

  async fn send_to_client(&self, packet: Packet) {
    self.daemon.send_packet(packet).await;
  }

  async fn get_devices(&self) -> HashMap<PeerId, Device> {
    self.devices.read().await.clone()
  }

  async fn get_device(&self, peer_id: &PeerId) -> Option<Device> {
    let devices = self.devices.read().await;
    let device = devices.get(peer_id);
    match device {
      Some(d) => Some(d.clone()),
      None => None,
    }
  }
}

#[multiconnect_macros::module]
pub struct ModuleA {
  test: u16,
}

impl ModuleA {
  fn new() -> Self {
    Self { test: 0 }
  }
}

// impl McModule for ModuleA {
//   fn name(&self) -> &'static str {
//     todo!()
//   }

//   fn on_peer_packet(&mut self, source: &PeerId, packet: &Packet, ctx: &mut McContext) {
//     todo!()
//   }

//   fn on_frontend_event(&mut self, packet: &Packet, ctx: &mut McContext) {
//     todo!()
//   }
