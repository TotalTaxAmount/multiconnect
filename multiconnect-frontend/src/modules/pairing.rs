use std::{
  any::Any,
  collections::{HashMap, VecDeque},
  ops::Mul,
  str::FromStr,
  sync::Arc,
};

use async_trait::async_trait;
use log::debug;
use multiconnect_protocol::{local::peer::L2PeerPairRequest, Device, Packet};
use tauri::{AppHandle, Emitter, State, Wry};
use tokio::sync::{futures::Notified, Mutex, Notify};
use uuid::Uuid;

use crate::with_manager_module;

use super::{FrontendCtx, FrontendModule, FrontendModuleManager};

pub struct PairingModule {
  pending: HashMap<Uuid, Device>,
}

impl PairingModule {
  pub fn new() -> Self {
    Self { pending: HashMap::new() }
  }

  pub async fn send_request(&mut self, target: Device, ctx: &mut FrontendCtx) {
    let uuid = Uuid::new_v4();
    self.pending.insert(uuid, target.clone());
    ctx.send_packet(Packet::L2PeerPairRequest(L2PeerPairRequest::new(&target, uuid))).await;
  }
}

#[async_trait]
impl FrontendModule for PairingModule {
  async fn init(&mut self, _ctx: Arc<Mutex<FrontendCtx>>) {}

  async fn on_packet(&mut self, packet: Packet, ctx: &mut FrontendCtx) {
    match packet {
      Packet::L2PeerPairRequest(packet) => {
        debug!("Recvivied pairing request packet");
        let device = bincode::deserialize::<Device>(&packet.device).unwrap();
        let uuid = Uuid::from_str(&packet.req_uuid).unwrap();
        self.pending.insert(uuid, device.clone());
        ctx.app.emit("pair-request", device).unwrap();
      }
      Packet::L3PeerPairResponse(packet) => {
        debug!("Recivied pairing response");
        let uuid = Uuid::from_str(&packet.req_uuid).unwrap();
        if let Some(d) = self.pending.remove(&uuid) {
          let _ = ctx.app.emit("pair-response", (d, packet.accepted));
        }
      }
      _ => {}
    }
  }

  async fn periodic(&mut self, _ctx: &mut FrontendCtx) {}

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

#[tauri::command]
pub async fn send_pairing_request(manager: State<'_, FrontendModuleManager>, target: Device) -> Result<(), &str> {
  with_manager_module!(manager, PairingModule, |m, ctx| {
    m.send_request(target, &mut ctx).await;
    Ok(())
  })
}
