use crate::{*, sms::*, transfer::*, peer::*};

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

impl PeerPairRequest {
  pub fn new() -> Self {
    Self {
      id: Packet::create_id(),
    }
  }
}

impl PeerPairResponse {
  pub fn new(accepted: bool) -> Self {
    Self { id: Packet::create_id(), accepted }
  }
}