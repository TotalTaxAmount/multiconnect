pub mod packets;

use log::error;
use packets::*;
use prost::{EncodeError, Message};
use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum PacketError {
  #[error("Malformed packet")]
  MalformedPacket,
  #[error("Invalid packet")]
  InvalidPacket,
  #[error("Failed to encode packet")]
  EncodeError,
}

impl Packet {
  pub fn to_bytes(packet: Packet) -> Result<Vec<u8>, PacketError> {
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
      Packet::PeerFound(peer_found) => todo!(),
      Packet::PeerPairRequest(peer_pair_request) => todo!(),
      Packet::PeerConnect(peer_connect) => todo!(),
      Packet::TransferStart(transfer_start) => todo!(),
      Packet::TransferChunk(transfer_chunk) => todo!(),
      Packet::TransferEnd(transfer_end) => todo!(),
      Packet::TransferStatus(transfer_status) => todo!(),
      Packet::SmsMessage(sms_message) => todo!(),
      Packet::Notify(notify) => todo!(),
    }
    buf.extend_from_slice(b"\r\n\r\n");
    Ok(buf)
  }

  pub fn from_bytes(bytes: &[u8]) -> Result<Packet, PacketError> {
    let (packet_type, data) = (bytes[0], &bytes[1..]);
    match packet_type {
      0 => Ok(Packet::Ping(Ping::decode(data).map_err(|e| PacketError::MalformedPacket )?)),
      1 => Ok(Packet::Acknowledge(Acknowledge::decode(data).map_err(|_| PacketError::MalformedPacket)?)),
      _ => {
        error!("Unknown packet type {}", packet_type);
        Err(PacketError::InvalidPacket)
      }
    }
  }
}
