use std::collections::HashMap;

use async_trait::async_trait;
use libp2p::PeerId;
use log::info;
use multiconnect_protocol::{Device, Packet};
use tokio::sync::mpsc;
use tracing_subscriber::filter::targets;

use crate::SharedDaemon;

#[async_trait]
pub trait MulticonnectModule: Send + Sync {
  async fn periodic(&mut self, ctx: &mut MulticonnectCtx);
  async fn on_peer_packet(&mut self, packet: Packet, source: Device, ctx: &mut MulticonnectCtx);
  async fn on_frontend_packet(&mut self, packet: Packet, ctx: &mut MulticonnectCtx);
}

pub struct MulticonnectCtx {
  daemon: SharedDaemon,
  packet_channel: mpsc::Sender<(PeerId, Packet)>,
}

impl MulticonnectCtx {
  pub fn new(daemon: SharedDaemon, packet_channel: mpsc::Sender<(PeerId, Packet)>) -> Self {
    Self { daemon, packet_channel }
  }

  pub async fn send_to_frontend(&self, packet: Packet) {
    self.daemon.send_packet(packet).await;
  }

  pub async fn send_to_peer(&self, target: PeerId, packet: Packet) {
    let _ = self.packet_channel.send((target, packet)).await;
  }
}

pub struct ModuleManager {
  modules: Vec<Box<dyn MulticonnectModule>>,
}

impl ModuleManager {
  pub fn new() -> Self {
    Self { modules: Vec::new() }
  }

  pub fn register<M: MulticonnectModule + 'static>(&mut self, module: M) {
    let boxed = Box::new(module);
    self.modules.push(boxed);
  }

  pub async fn call_perodic(&mut self, ctx: &mut MulticonnectCtx) {
    for module in self.modules.iter_mut() {
      module.periodic(ctx).await;
    }
  }

  pub async fn call_on_peer_packet(&mut self, device: &Device, packet: Packet, ctx: &mut MulticonnectCtx) {
    for module in self.modules.iter_mut() {
      module.on_peer_packet(packet.clone(), device.clone(), ctx).await;
    }
  }

  pub async fn call_on_frontend_packet(&mut self, packet: Packet, ctx: &mut MulticonnectCtx) {
    for module in self.modules.iter_mut() {
      module.on_frontend_packet(packet.clone(), ctx).await;
    }
  }
}

pub struct ModuleTest;
impl ModuleTest {
  pub fn new() -> Self {
    Self {}
  }
}

#[async_trait]
impl MulticonnectModule for ModuleTest {
  async fn periodic(&mut self, ctx: &mut MulticonnectCtx) {
    info!("Periodic")
  }

  async fn on_peer_packet(&mut self, packet: Packet, source: Device, ctx: &mut MulticonnectCtx) {
    info!("Recv packet: {:?} from {:?}", packet, source)
  }

  async fn on_frontend_packet(&mut self, packet: Packet, ctx: &mut MulticonnectCtx) {
    info!("Recv frontend packet: {:?}", packet)
  }
}
