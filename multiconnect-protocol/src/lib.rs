pub mod impls;

use generated::multiconnect::{peer::*, sms::*, transfer::*, *};

use libp2p::{Multiaddr, PeerId};
use log::{debug, error, trace};
use prost::Message;
use thiserror::Error;
use uid::IdU32;

pub mod generated {
  include!(concat!(env!("OUT_DIR"), "/proto.rs"));
}

pub use generated::multiconnect::*;

#[derive(Debug, Clone, Error)]
pub enum PacketError {
  #[error("Malformed packet")]
  MalformedPacket,
  #[error("Invalid packet")]
  InvalidPacket(String),
  #[error("Failed to encode packet")]
  EncodeError,
}

impl From<std::io::Error> for PacketError {
  fn from(value: std::io::Error) -> Self {
    todo!()
  }
}

/// Peer struct, will have other field like device type and common name
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
pub struct Peer {
  /// The libp2p peer id for the peer
  pub peer_id: PeerId,
  /// The libp2p address for the peer
  pub multiaddr: Multiaddr,
}

/// Device struct containing a peer and metadata about the device
pub struct Device {
  pub peer: Peer,
  pub meta: PeerMeta,
}

impl Peer {
  /// Create a new peer
  fn new(peer_id: PeerId, multiaddr: Multiaddr) -> Self {
    Self { peer_id, multiaddr }
  }
}

/// All the possible packet types
#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
  /// Ping packet, used to see if the client or daemon is responding
  Ping(Ping),
  /// Acknowledge pings or other packets
  Acknowledge(Acknowledge),
  /// Sent when a peer is found by libp2p (daemon -> client)
  PeerFound(PeerFound),
  /// Sent when a peer expires (daemon -> client)
  PeerExpired(PeerExpired),
  /// Sent either when a peer requests to pair or when the client wants to
  /// send pair request (daemon -> client or client -> daemon)
  PeerPairRequest(PeerPairRequest),
  /// The pairing response from the peer (daemon -> client)
  PeerPairResponse(PeerPairResponse),
  // TODO
  TransferStart(TransferStart),
  TransferChunk(TransferChunk),
  TransferEnd(TransferEnd),
  TransferStatus(TransferStatus),
  SmsMessage(SmsMessage),
  Notify(Notification),
}

impl Packet {
  #[inline]
  pub(crate) fn create_id() -> u32 {
    IdU32::<Packet>::new().get()
  }

  /// Convert a [`Packet`] to bytes as a [`Vec<u8>`]
  pub fn to_bytes(packet: &Packet) -> Result<Vec<u8>, PacketError> {
    let mut buf = Vec::new();
    match packet {
      Packet::Ping(ping) => {
        buf.push(0);
        ping.encode(&mut buf).map_err(|_| PacketError::EncodeError)?;
      }
      Packet::Acknowledge(acknowledge) => {
        buf.push(1);
        acknowledge.encode(&mut buf).map_err(|_| PacketError::EncodeError)?;
      }
      Packet::PeerFound(peer_found) => {
        buf.push(2);
        peer_found.encode(&mut buf).map_err(|_| PacketError::EncodeError)?;
      }
      Packet::PeerExpired(peer_expired) => {
        buf.push(3);
        peer_expired.encode(&mut buf).map_err(|_| PacketError::EncodeError)?;
      }
      Packet::PeerPairRequest(peer_pair_request) => {
        buf.push(4);
        peer_pair_request.encode(&mut buf).map_err(|_| PacketError::EncodeError)?;
      }
      Packet::PeerPairResponse(peer_pair_response) => {
        buf.push(5);
        peer_pair_response.encode(&mut buf).map_err(|_| PacketError::EncodeError)?;
      }
      Packet::TransferStart(transfer_start) => todo!(),
      Packet::TransferChunk(transfer_chunk) => todo!(),
      Packet::TransferEnd(transfer_end) => todo!(),
      Packet::TransferStatus(transfer_status) => todo!(),
      Packet::SmsMessage(sms_message) => todo!(),
      Packet::Notify(notify) => todo!(),
    }
    if buf.len() > u16::MAX.into() {
      return Err(PacketError::InvalidPacket("Packet is too big".into()));
    }

    let len = buf.len() as u16;
    let mut send_buf = Vec::with_capacity((2 + len).into());
    send_buf.extend_from_slice(&len.to_be_bytes());
    send_buf.extend_from_slice(&buf);

    trace!("Real len: {}", len);
    trace!("Raw bytes: {:?}", send_buf);
    Ok(send_buf)
  }

  pub fn from_bytes(bytes: &[u8]) -> Result<Packet, PacketError> {
    let (packet_type, data) = (bytes[0], &bytes[1..]);
    match packet_type {
      0 => Ok(Packet::Ping(Ping::decode(data).map_err(|_| PacketError::MalformedPacket)?)),
      1 => Ok(Packet::Acknowledge(Acknowledge::decode(data).map_err(|_| PacketError::MalformedPacket)?)),
      2 => Ok(Packet::PeerFound(PeerFound::decode(data).map_err(|_| PacketError::MalformedPacket)?)),
      3 => Ok(Packet::PeerExpired(PeerExpired::decode(data).map_err(|_| PacketError::MalformedPacket)?)),
      4 => Ok(Packet::PeerPairRequest(PeerPairRequest::decode(data).map_err(|_| PacketError::MalformedPacket)?)),
      5 => Ok(Packet::PeerPairResponse(PeerPairResponse::decode(data).map_err(|_| PacketError::MalformedPacket)?)),
      _ => {
        error!("Unknown packet type {}", packet_type);
        Err(PacketError::InvalidPacket("Unknown packet type".into()))
      }
    }
  }
}
