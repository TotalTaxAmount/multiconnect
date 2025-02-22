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
  pub fn new(accepted: bool) -> Self {
    Self { id: Packet::create_id(), accepted }
  }
}

// impl PeerPacket for PeerPairRequest {
//   fn deserialize_peer(&self) -> Result<Peer, PacketError> {
//     Ok(bincode::deserialize::<Peer>(&self.peer).map_err(|_|
// PacketError::MalformedPacket)?)   }
// }

// impl PeerPacket for PeerFound {
//   fn deserialize_peer(&self) -> Result<Peer, PacketError> {
//     Ok(bincode::deserialize::<Peer>(&self.peer).map_err(|_|
// PacketError::MalformedPacket)?)   }
// }

// impl PeerPacket for PeerExpired {
//   fn deserialize_peer(&self) -> Result<Peer, PacketError> {
//     Ok(bincode::deserialize::<Peer>(&self.peer).map_err(|_|
// PacketError::MalformedPacket)?)   }
// }
