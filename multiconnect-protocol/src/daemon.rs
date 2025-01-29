use crate::Packet;

#[derive(Clone, PartialEq, prost::Message)]
pub struct Ping {
  #[prost(uint32, tag = "1")]
  pub id: u32,
}

impl Ping {
  pub fn new() -> Self {
    Self { id: Packet::create_id() }
  }
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct Acknowledge {
  #[prost(uint32, tag = "1")]
  pub id: u32,
  #[prost(uint32, tag = "2")]
  pub req_id: u32,
}

impl Acknowledge {
  pub fn new(req_id: u32) -> Self {
    Self { id: Packet::create_id(), req_id }
  }
}

pub mod peer {
  use crate::{p2p::Peer, Packet};

  #[derive(Clone, PartialEq, prost::Message)]
  pub struct PeerFound {
    #[prost(uint32, tag = "1")]
    pub id: u32,
    #[prost(bytes, tag = "2")]
    pub peer: Vec<u8>,
  }

  impl PeerFound {
    pub fn new(peer: Peer) -> Self {
      Self { id: Packet::create_id(), peer: bincode::serialize(&peer).unwrap() } // FIXME: Unsafe
    }
  }

  #[derive(Clone, PartialEq, prost::Message)]
  pub struct PeerPairRequest {
    #[prost(uint32, tag = "1")]
    pub id: u32,
  }

  #[derive(Clone, PartialEq, prost::Message)]
  pub struct PeerConnect {
    #[prost(uint32, tag = "1")]
    pub id: u32,
  }
}

pub mod transfer {

  #[derive(Clone, PartialEq, prost::Message)]
  pub struct TransferStart {
    #[prost(uint32, tag = "1")]
    pub id: u32,
    #[prost(uint64, tag = "2")]
    pub total_len: u64,
  }

  #[derive(Clone, PartialEq, prost::Message)]
  pub struct TransferChunk {
    #[prost(uint32, tag = "1")]
    pub id: u32,
    #[prost(uint64, tag = "2")]
    pub len: u64,
    #[prost(bytes, tag = "3")]
    pub data: Vec<u8>,
  }

  #[derive(Clone, PartialEq, prost::Message)]
  pub struct TransferEnd {
    #[prost(uint32, tag = "1")]
    pub id: u32,
  }

  #[derive(Clone, PartialEq, prost::Message)]
  pub struct TransferStatus {
    #[prost(uint32, tag = "1")]
    pub id: u32,
    #[prost(enumeration = "Status", tag = "2")]
    pub status: i32,
  }

  #[derive(Clone, PartialEq, Eq, Debug, prost::Enumeration)]
  #[repr(i32)]
  pub enum Status {
    Ok = 0,
    MalformedPacket = -1,
  }
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct SmsMessage {
  #[prost(uint32, tag = "1")]
  pub id: u32,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct Notify {
  #[prost(uint32, tag = "1")]
  pub id: u32,
}
