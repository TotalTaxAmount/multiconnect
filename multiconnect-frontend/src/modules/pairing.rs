use std::{any::Any, collections::HashMap, str::FromStr, sync::Arc};

use async_trait::async_trait;
use libp2p_core::PeerId;
use log::{debug, error};
use multiconnect_protocol::{
  local::peer::{L2PeerPairRequest, L3PeerPairResponse, L4Refresh},
  Device, Packet,
};
use tauri::{Emitter, State};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{with_ctx, with_manager_module};

use super::{FrontendCtx, FrontendModule, FrontendModuleManager};

pub struct PairingModule {
  pending: HashMap<Uuid, Device>,
  devices: HashMap<PeerId, (Device, bool, u64)>,
}

impl PairingModule {
  pub fn new() -> Self {
    Self { pending: HashMap::new(), devices: HashMap::new() }
  }

  pub async fn send_request(&mut self, target: Device, ctx: &mut FrontendCtx) -> Option<Uuid> {
    if !self.devices.contains_key(&target.peer) {
      let uuid = Uuid::new_v4();
      self.pending.insert(uuid, target.clone());
      ctx.send_packet(Packet::L2PeerPairRequest(L2PeerPairRequest::new(&target, uuid))).await;
      return Some(uuid);
    }
    None
  }

  pub async fn send_response(&mut self, uuid: Uuid, accepted: bool, ctx: &mut FrontendCtx) {
    if let Some(source) = self.pending.remove(&uuid) {
      debug!("Sending pairing response: uuid = {}, accepted = {}", uuid, accepted);
      if accepted {
        let _ = ctx.app.emit("peer-expired", source.peer); // Clear device from discovred devices
      }
      ctx.send_packet(Packet::L3PeerPairResponse(L3PeerPairResponse::new(accepted, uuid))).await;
    }
  }
}

#[async_trait]
impl FrontendModule for PairingModule {
  async fn init(&mut self, _ctx: Arc<Mutex<FrontendCtx>>) {}

  async fn on_packet(&mut self, packet: Packet, ctx: &mut FrontendCtx) {
    match packet {
      Packet::L2PeerPairRequest(packet) => {
        debug!("Received pairing request packet");
        let source = bincode::deserialize::<Device>(&packet.device).unwrap();
        let uuid = Uuid::from_str(&packet.req_uuid).unwrap();
        self.pending.insert(uuid, source.clone());
        ctx.app.emit("peer-pair-request", (source, uuid.to_string())).unwrap();
      }
      Packet::L3PeerPairResponse(packet) => {
        debug!("Received pairing response");
        let uuid = Uuid::from_str(&packet.req_uuid).unwrap();
        if let Some(_source) = self.pending.remove(&uuid) {
          // if packet.accepted {
          //   let _ = ctx.app.emit("peer-expired", source.peer);
          // }
          let _ = ctx.app.emit("pair-response", (packet.req_uuid, packet.accepted));
        }
      }
      Packet::L0PeerFound(packet) => {
        let device = bincode::deserialize::<Device>(&packet.device).unwrap();
        debug!("Received peer found: device = {:?}", device);
        if let Err(e) = ctx.app.emit("peer-found", device) {
          error!("Failed to emit: {}", e);
        }
      }
      Packet::L1PeerExpired(packet) => {
        let _ = ctx.app.emit("peer-expired", packet.peer_id);
      }
      Packet::L7DeviceStatus(packet) => {
        // Only for paired devices
        let device: Device = bincode::deserialize::<Device>(&packet.device).unwrap();

        let _ = ctx.app.emit("device-status", (&device, packet.online, packet.last_seen));
        self.devices.insert(device.peer, (device, packet.online, packet.last_seen));
      }
      Packet::L8DeviceStatusUpdate(packet) => {
        // Only for paired devices
        debug!("Received device status update for peer {}", packet.peer_id);

        let peer_id = PeerId::from_str(&packet.peer_id).unwrap();
        if let Some(entry) = self.devices.get_mut(&peer_id) {
          if let Some(device_bytes) = &packet.device {
            if let Ok(device) = bincode::deserialize::<Device>(device_bytes) {
              entry.0 = device;
            }
          }

          if let Some(online) = packet.online {
            entry.1 = online;
          }

          if let Some(last_seen) = packet.last_seen {
            entry.2 = last_seen;
          }

          let _ = ctx.app.emit("device-status", (&entry.0, entry.1, entry.2));
        } else {
          debug!("Device status update ignored: peer {} not in device list", peer_id);
        }
      }
      _ => {}
    }
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

#[tauri::command]
pub async fn send_pairing_request(manager: State<'_, FrontendModuleManager>, target: Device) -> Result<String, &str> {
  debug!("Received pairing request command from UI");
  with_manager_module!(manager, PairingModule, |m, ctx| {
    if let Some(uuid) = m.send_request(target, &mut ctx).await {
      return Ok(uuid.to_string());
    }
    Err("Already paired")
    // Ok(uuid.unwrap().to_string())
  })
}

#[tauri::command]
pub async fn send_pairing_response(
  manager: State<'_, FrontendModuleManager>,
  uuid: String,
  accepted: bool,
) -> Result<(), &str> {
  let uuid = Uuid::parse_str(&uuid).expect("Invalid UUID");

  with_manager_module!(manager, PairingModule, |m, ctx| {
    m.send_response(uuid, accepted, &mut ctx).await;
    Ok(())
  })
}

#[tauri::command]
pub async fn refresh_devices(manager: State<'_, FrontendModuleManager>) -> Result<(), &str> {
  with_ctx!(manager, |ctx| {
    ctx.send_packet(Packet::L4Refresh(L4Refresh::new())).await;
    Ok(())
  })
}
