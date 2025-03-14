use libp2p::PeerId;

use crate::{
  local::peer::*,
  p2p::{
    peer::{P2PeerPairRequest, P3PeerPairResponse},
    *,
  },
  shared::peer::*,
  Packet, Peer,
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
  pub fn new(target: &PeerId) -> Self {
    Self { id: Packet::create_id(), target_id: bincode::serialize(&target).unwrap() }
  }
}

impl P3PeerPairResponse {
  pub fn new(req_id: u32, accepted: bool) -> Self {
    Self { id: Packet::create_id(), req_id, accepted }
  }
}

impl L0PeerFound {
  pub fn new(peer: Peer) -> Self {
    Self { id: Packet::create_id(), peer: bincode::serialize(&peer).unwrap() }
  }
}

impl L1PeerExpired {
  pub fn new(peer_id: &PeerId) -> Self {
    Self { id: Packet::create_id(), peer_id: peer_id.to_string() }
  }
}

impl L2PeerPairRequest {
  pub fn new(peer_id: &PeerId) -> Self {
    Self { id: Packet::create_id(), peer_id: bincode::serialize(&peer_id).unwrap() }
  }
}

impl L3PeerPairResponse {
  pub fn new(accepted: bool, req_id: u32) -> Self {
    Self { id: Packet::create_id(), req_id, accepted }
  }
}

impl S1PeerMeta {
  pub fn new(os_name: String, device_name: String, mc_version: String, device_type: DeviceType) -> Self {
    Self { id: Packet::create_id(), os_name, device_name, mc_version, r#type: device_type.into() }
  }
}
