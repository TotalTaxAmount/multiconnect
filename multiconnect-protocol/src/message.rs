use core::fmt;
use std::{fs::Metadata, str::Bytes};

use log::{error, info};
use rand::Rng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub enum ProtocolError {
  TypeError,
  MalformedPacket,
}

impl fmt::Display for ProtocolError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match &self {
      ProtocolError::TypeError => write!(f, "Unknown message type"),
      ProtocolError::MalformedPacket => write!(f, "Malformed packed"),
    }
  }
}

impl std::error::Error for ProtocolError {}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[repr(u8)]
pub enum MsgType {
  Ping = 0x01,
  Acknowledge = 0x02,
  TransferStart = 0x03,
  TransferChunk = 0x04,
  TransferEnd = 0x5,
  Message = 0x6,
  Status = 0x07,
}

impl TryFrom<u8> for MsgType {
  type Error = ProtocolError;

  fn try_from(value: u8) -> Result<Self, Self::Error> {
    match value {
      0x01 => Ok(Self::Ping),
      0x02 => Ok(Self::Acknowledge),
      0x03 => Ok(Self::TransferStart),
      0x04 => Ok(Self::TransferChunk),
      0x05 => Ok(Self::TransferEnd),
      0x06 => Ok(Self::Message),
      0x07 => Ok(Self::Status),
      _ => Err(ProtocolError::TypeError),
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProtocolMessage {
  pub id: u8,
  pub msg_type: MsgType,
  pub length: u32,
  pub payload: Vec<u8>,
}

impl ProtocolMessage {
  pub fn new(msg_type: MsgType, payload: Vec<u8>) -> Self {
    let id = rand::thread_rng().gen_range(0..=u8::MAX);
    let length = payload.len() as u32;
    ProtocolMessage { id, msg_type, length, payload }
  }

  pub fn to_bytes(&self) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.push(self.id);
    buf.push(self.msg_type as u8);
    buf.extend_from_slice(&self.length.to_be_bytes());
    buf.extend_from_slice(&self.payload);
    buf.extend_from_slice(b"\r\n\r\n");
    buf
  }

  pub fn from_bytes(bytes: &[u8]) -> Result<Self, ProtocolError> {
    let id = u8::from_be_bytes([bytes[0]]);
    let msg_type = MsgType::try_from(bytes[1]).map_err(|_| {
      error!("Invalid message type: {}", bytes[1]);
      ProtocolError::MalformedPacket
    })?;

    let length = u32::from_be_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
    let payload = bytes[6..(bytes.len() - 4)].to_vec();
    Ok(Self { id, msg_type, length, payload })
  }
}
