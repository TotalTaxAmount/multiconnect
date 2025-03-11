pub mod impls;

use generated::multiconnect::{peer::*, sms::*, transfer::*, *};

use libp2p::{Multiaddr, PeerId};
use log::{debug, error, trace};
use prost::{bytes::BufMut, Message};
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

    fn encode_packet<M: Message>(buf: &mut Vec<u8>, packet_type: u8, data: &M) -> Result<(), PacketError> {
      buf.push(packet_type);
      data.encode(buf).map_err(|_| PacketError::EncodeError)
    }

    match packet {
      Packet::Ping(ping) => encode_packet(&mut buf, 0, ping)?,
      Packet::Acknowledge(acknowledge) => encode_packet(&mut buf, 1, acknowledge)?,
      Packet::PeerFound(peer_found) => encode_packet(&mut buf, 2, peer_found)?,
      Packet::PeerExpired(peer_expired) => encode_packet(&mut buf, 3, peer_expired)?,
      Packet::PeerPairRequest(peer_pair_request) => encode_packet(&mut buf, 4, peer_pair_request)?,
      Packet::PeerPairResponse(peer_pair_response) => encode_packet(&mut buf, 5, peer_pair_response)?,
      Packet::TransferStart(transfer_start) => encode_packet(&mut buf, 6, transfer_start)?,
      Packet::TransferChunk(transfer_chunk) => encode_packet(&mut buf, 7, transfer_chunk)?,
      Packet::TransferEnd(transfer_end) => encode_packet(&mut buf, 8, transfer_end)?,
      Packet::TransferStatus(transfer_status) => encode_packet(&mut buf, 9, transfer_status)?,
      Packet::SmsMessage(sms_message) => encode_packet(&mut buf, 10, sms_message)?,
      Packet::Notify(notify) => encode_packet(&mut buf, 11, notify)?,
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
