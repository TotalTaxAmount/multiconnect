pub mod impls;

use generated::multiconnect::{
  local::peer::*,
  p2p::{peer::*, *},
  shared::peer::*,
};

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
  pub meta: S1PeerMeta,
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
  /// Ping packet, used to see if the client or daemon is responding (p -> p)
  P0Ping(P0Ping),
  /// Acknowledge pings or other packets (p -> p)
  P1Acknowledge(P1Acknowledge),
  /// Sent over p2p when a peer makes a pair rq (s -> r)
  P2PeerPairRequest(P2PeerPairRequest),
  /// Sent over p2p when a peer response to a request (r -> s)
  P3PeerPairResponse(P3PeerPairResponse),
  /// Sent when a peer is found by libp2p (daemon -> client)
  L0PeerFound(L0PeerFound),
  /// Sent when a peer expires (daemon -> client)
  L1PeerExpired(L1PeerExpired),
  /// Sent either when a peer requests to pair or when the client wants to
  /// send pair request (daemon -> client or client -> daemon)
  L2PeerPairRequest(L2PeerPairRequest),
  /// The pairing response from the peer (daemon -> client or client -> daemon)
  L3PeerPairResponse(L3PeerPairResponse),
  /// Get metadata about a peer
  S1PeerMeta(S1PeerMeta), // TODO
                          // TransferStart(TransferStart),
                          // TransferChunk(TransferChunk),
                          // TransferEnd(TransferEnd),
                          // TransferStatus(TransferStatus),
                          // SmsMessage(SmsMessage),
                          // Notify(Notification),
}

impl Packet {
  #[inline]
  pub(crate) fn create_id() -> u32 {
    IdU32::<Packet>::new().get()
  }

  /// Convert a [`Packet`] to bytes as a [`Vec<u8>`]
  pub fn to_bytes(packet: &Packet) -> Result<Vec<u8>, PacketError> {
    let mut buf = Vec::new();

    if buf.len() > u16::MAX.into() {
      return Err(PacketError::InvalidPacket("Packet is too big".into()));
    }

    Self::match_encode_packet(packet, &mut buf)?;

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
      0 => Ok(Packet::P0Ping(P0Ping::decode(data).map_err(|_| PacketError::MalformedPacket)?)),
      1 => Ok(Packet::P1Acknowledge(P1Acknowledge::decode(data).map_err(|_| PacketError::MalformedPacket)?)),
      2 => Ok(Packet::P2PeerPairRequest(P2PeerPairRequest::decode(data).map_err(|_| PacketError::MalformedPacket)?)),
      3 => Ok(Packet::P3PeerPairResponse(P3PeerPairResponse::decode(data).map_err(|_| PacketError::MalformedPacket)?)),
      4 => Ok(Packet::L0PeerFound(L0PeerFound::decode(data).map_err(|_| PacketError::MalformedPacket)?)),
      5 => Ok(Packet::L1PeerExpired(L1PeerExpired::decode(data).map_err(|_| PacketError::MalformedPacket)?)),
      6 => Ok(Packet::L2PeerPairRequest(L2PeerPairRequest::decode(data).map_err(|_| PacketError::MalformedPacket)?)),
      7 => Ok(Packet::L3PeerPairResponse(L3PeerPairResponse::decode(data).map_err(|_| PacketError::MalformedPacket)?)),
      _ => {
        error!("Unknown packet type {}", packet_type);
        Err(PacketError::InvalidPacket("Unknown packet type".into()))
      }
    }
  }

  #[rustfmt::skip]
  fn match_encode_packet(packet: &Packet, buf: &mut Vec<u8>) -> Result<(), PacketError> {
    fn encode_packet<M: Message>(buf: &mut Vec<u8>, packet_type: u8, data: &M) -> Result<(), PacketError> {
      buf.push(packet_type);
      data.encode(buf).map_err(|_| PacketError::EncodeError)
    }

    match packet {
      // p2p packets
      Packet::P0Ping(ping)                                       => encode_packet(buf, 0, ping)?,
      Packet::P1Acknowledge(acknowledge)                  => encode_packet(buf, 1, acknowledge)?,
      Packet::P2PeerPairRequest(peer_pair_request)    => encode_packet(buf, 2, peer_pair_request)?,
      Packet::P3PeerPairResponse(peer_pair_response) => encode_packet(buf, 3, peer_pair_response)?,
      // Local (daemon + client) packets
      Packet::L0PeerFound(peer_found)                       => encode_packet(buf, 4, peer_found)?,
      Packet::L1PeerExpired(peer_expired)                 => encode_packet(buf, 5, peer_expired)?,
      Packet::L2PeerPairRequest(peer_pair_request)    => encode_packet(buf, 6, peer_pair_request)?,
      Packet::L3PeerPairResponse(peer_pair_response) => encode_packet(buf, 7, peer_pair_response)?,
      // Shared (daemon + client and p2p) packets
      Packet::S1PeerMeta(peer_meta)                          => encode_packet(buf, 8, peer_meta)?,
      // Packet::TransferStart(transfer_start) => encode_packet(&mut buf, 6, transfer_start)?,
      // Packet::TransferChunk(transfer_chunk) => encode_packet(&mut buf, 7, transfer_chunk)?,
      // Packet::TransferEnd(transfer_end) => encode_packet(&mut buf, 8, transfer_end)?,
      // Packet::TransferStatus(transfer_status) => encode_packet(&mut buf, 9, transfer_status)?,
      // Packet::SmsMessage(sms_message) => encode_packet(&mut buf, 10, sms_message)?,
      // Packet::Notify(notify) => encode_packet(&mut buf, 11, notify)?,
    };
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use libp2p::PeerId;

  #[test]
  fn test_packet_serialization() {
    let packet = Packet::P0Ping(P0Ping::new());
    let bytes = Packet::to_bytes(&packet).expect("Failed to serialize packet");
    let deserialized_packet = Packet::from_bytes(&bytes[2..]).expect("Failed to deserialize packet");
    assert_eq!(packet, deserialized_packet, "Serialized and deserialized packets do not match");
  }

  #[test]
  fn test_invalid_packet() {
    let invalid_bytes = vec![255, 0, 1, 2]; // Unknown packet type
    let result = Packet::from_bytes(&invalid_bytes);
    assert!(result.is_err(), "Invalid packet should return an error");
  }
}
