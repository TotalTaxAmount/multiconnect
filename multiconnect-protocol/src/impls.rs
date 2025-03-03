use crate::{peer::*, sms::*, transfer::*, *};

// TODO: Maybe use traits here?
impl Ping {
  pub fn new() -> Self {
    Self { id: Packet::create_id() }
  }
}

impl Acknowledge {
  pub fn new(req_id: u32) -> Self {
    Self { id: Packet::create_id(), req_id }
  }
}

impl PeerFound {
  pub fn new(peer: Peer) -> Self {
    Self { id: Packet::create_id(), peer: bincode::serialize(&peer).unwrap() }
  }
}

impl PeerExpired {
  pub fn new(peer_id: &PeerId) -> Self {
    Self { id: Packet::create_id(), peer_id: peer_id.to_string() }
  }
}

impl PeerPairRequest {
  pub fn new(peer: &Peer) -> Self {
    Self { id: Packet::create_id(), peer: bincode::serialize(&peer).unwrap() }
  }
}

impl PeerPairResponse {
  pub fn new(accepted: bool, req_id: u32, peer: &Peer) -> Self {
    Self { id: Packet::create_id(), req_id, accepted, peer: bincode::serialize(&peer).unwrap() }
  }
}

impl PeerMeta {
  pub fn new(os_name: String, device_name: String, mc_version: String, device_type: DeviceType) -> Self {
    Self { id: Packet::create_id(), os_name, device_name, mc_version, r#type: device_type.into() }
  }
}
