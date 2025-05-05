use std::time::{Duration, SystemTime, UNIX_EPOCH};

use libp2p::PeerId;
use tokio::time::Instant;
use uuid::Uuid;

use crate::{
  local::peer::{l7_saved_peer_status::SavedPeer, *},
  p2p::{peer::*, *},
  shared::peer::*,
  Device, Packet,
};

// TODO: Maybe use traits here?
impl P0Ping {
  pub fn new() -> Self {
    Self { id: Packet::create_id() }
  }
}

impl P1Acknowledge {
  pub fn new(req_id: u32) -> Self {
    Self { id: Packet::create_id(), req_id }
  }
}

impl P2PeerPairRequest {
  pub fn new(device: &Device, req_uuid: Uuid) -> Self {
    Self { id: Packet::create_id(), device: bincode::serialize(&device).unwrap(), req_uuid: req_uuid.to_string() }
  }
}

impl P3PeerPairResponse {
  pub fn new(req_id: Uuid, accepted: bool) -> Self {
    Self { id: Packet::create_id(), req_uuid: req_id.to_string(), accepted }
  }
}

impl L4Refresh {
  pub fn new() -> Self {
    Self { id: Packet::create_id() }
  }
}

impl L0PeerFound {
  pub fn new(device: &Device) -> Self {
    Self { id: Packet::create_id(), device: bincode::serialize(device).unwrap() }
  }
}

impl L1PeerExpired {
  pub fn new(peer_id: &PeerId) -> Self {
    Self { id: Packet::create_id(), peer_id: peer_id.to_string() }
  }
}

impl L2PeerPairRequest {
  pub fn new(device: &Device, uuid: Uuid) -> Self {
    Self { id: Packet::create_id(), device: bincode::serialize(&device).unwrap(), req_uuid: uuid.to_string() }
  }
}

impl L3PeerPairResponse {
  pub fn new(accepted: bool, uuid: Uuid) -> Self {
    Self { id: Packet::create_id(), req_uuid: uuid.to_string(), accepted }
  }
}

impl L7SavedPeerStatus {
  pub fn new(peers: Vec<SavedPeer>) -> Self {
    Self { id: Packet::create_id(), peers }
  }
}

impl L8SavedPeerUpdate {
  pub fn new(
    peer_id: &PeerId,
    device: Option<&Device>,
    paired: Option<bool>,
    online: Option<bool>,
    last_seen: Option<SystemTime>,
  ) -> Self {
    let device = if let Some(d) = device {
      let raw = bincode::serialize(d).unwrap();
      Some(raw)
    } else {
      None
    };

    let last_seen = if let Some(instant) = last_seen {
      Some(instant.duration_since(UNIX_EPOCH).unwrap().as_secs())
    } else {
      None
    };
    Self { id: Packet::create_id(), peer_id: peer_id.to_string(), device, online, paired, last_seen }
  }
}

impl S1PeerMeta {
  pub fn new(os_name: String, device_name: String, mc_version: String, device_type: DeviceType) -> Self {
    Self { id: Packet::create_id(), os_name, device_name, mc_version, device_type: device_type.into() }
  }

  pub fn from_device(device: &Device) -> Self {
    Self::new(
      device.os_name.to_string(),
      device.device_name.to_string(),
      device.mc_version.to_string(),
      device.device_type,
    )
  }
}
