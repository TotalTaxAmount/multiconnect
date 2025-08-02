pub mod impls;

use std::time::{SystemTime, UNIX_EPOCH};

use generated::{
  multiconnect::{
    local::{peer::*, transfer::*},
    p2p::{peer::*, *},
    shared::peer::*,
  },
  P4TransferStart, P5TransferChunk, P6TransferStatus,
};

use libp2p::PeerId;
use log::{error, trace};
use prost::Message;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uid::IdU32;

pub mod generated {
  include!(concat!(env!("OUT_DIR"), "/proto.rs"));
}

pub use generated::multiconnect::*;

use crate::generated::D0Debug;

// Generate handlers for a list of packets
macro_rules! gen_packet_handlers {
  {$($variant:ident => $tag:expr),* $(,)?} => {
    #[derive(Debug, Clone, PartialEq)]
    pub enum Packet { $( $variant($variant), )* }

    impl Packet {
      pub fn match_encode_packet(packet: &Packet, buf: &mut Vec<u8>) -> Result<(), PacketError> {
        match packet {
          $(
            Packet::$variant(inner) => {
              buf.push($tag);
              inner.encode(buf).map_err(|_| PacketError::EncodeError)
            }
          ),*
        }
      }

      pub fn from_bytes(bytes: &[u8]) -> Result<Packet, PacketError> {
        let (packet_type, data) = (bytes[0], &bytes[1..]);
        match packet_type {
          $(
            $tag => {
              let pkt = $variant::decode(data)
                .map_err(|_| PacketError::MalformedPacket)?;
              Ok(Packet::$variant(pkt))
            }
          ),*
          ,
          other => {
            error!("Unknown packet type {}", other);
            Err(PacketError::InvalidPacket("Unknown packet type".into()))
          }
        }
      }
    }
  }
}

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

/// Device struct containing a peer and metadata about the device
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
  /// The peer *id* of the device on the network
  pub peer: PeerId,
  /// The string name of the device's os
  pub os_name: String,
  /// The string name of the device (hostname by default)
  pub device_name: String,
  /// The version of multiconnect in use
  pub mc_version: String,
  /// The type of device
  pub device_type: DeviceType,
}

impl Device {
  pub fn new(peer: PeerId, os_name: String, device_name: String, mc_version: String, device_type: DeviceType) -> Self {
    Self { peer, os_name, device_name, mc_version, device_type }
  }

  pub fn this(local_peer_id: PeerId) -> Self {
    let os_name = os_info::get().to_string();
    let device_name = gethostname::gethostname().to_str().unwrap().to_string();
    let mc_version = option_env!("CARGO_PKG_VERSION").unwrap_or("Unknown");
    let device_type = if is_laptop() {
      DeviceType::Laptop
    } else {
      DeviceType::Desktop
    };

    Self::new(local_peer_id, os_name, device_name, mc_version.to_owned(), device_type)
  }

  pub fn from_meta(peer_meta: S1PeerMeta, peer_id: PeerId) -> Self {
    let device_type = peer_meta.device_type();
    Self::new(peer_id, peer_meta.os_name, peer_meta.device_name, peer_meta.mc_version, device_type)
  }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SavedDevice {
  device: Device,
  paired: bool,
  last_seen: u64,
}

impl SavedDevice {
  pub fn new(device: Device, paired: bool) -> Self {
    Self { device, paired, last_seen: SystemTime::elapsed(&UNIX_EPOCH).unwrap().as_secs() }
  }

  pub fn get_device(&self) -> &Device {
    &self.device
  }

  pub fn is_paired(&self) -> bool {
    self.paired
  }

  pub fn last_seen(&self) -> u64 {
    self.last_seen
  }

  pub fn set_last_seen(&mut self, last_seen: u64) {
    self.last_seen = last_seen;
  }

  pub fn set_paired(&mut self, paired: bool) {
    self.paired = paired;
  }
}

gen_packet_handlers! {
  P0Ping => 0,
  P1Acknowledge=> 1,
  P2PeerPairRequest=>2,
  P3PeerPairResponse=> 3,
  P4TransferStart=> 4,
  P5TransferChunk=> 5,
  P6TransferStatus=> 6,
  L0PeerFound=> 7,
  L1PeerExpired=> 8,
  L2PeerPairRequest=> 9,
  L3PeerPairResponse=> 10,
  L4Refresh=> 11,
  L7DeviceStatus=> 12,
  L8DeviceStatusUpdate=> 13,
  L9TransferFile => 14,
  L10TransferProgress => 15,
  L11TransferStatus => 16,
  S1PeerMeta => 17,
  D0Debug => 99,
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
}

#[cfg(target_os = "linux")]
fn is_laptop() -> bool {
  use std::fs;
  let power_supply_path = "/sys/class/power_supply/";
  if let Ok(entries) = fs::read_dir(power_supply_path) {
    for entry in entries.flatten() {
      let filename = entry.file_name();
      if filename.to_string_lossy().starts_with("BAT") {
        return true;
      }
    }
  }
  false
}

#[cfg(target_os = "windows")]
fn is_laptop() -> bool {
  use serde::Deserialize;
  use wmi::{COMLibrary, WMIConnection};

  #[derive(Deserialize, Debug)]
  struct Win32_Battery {
    Name: String,
  }

  let com_con = COMLibrary::new().expect("Failed to initialize COM");
  let wmi_con = WMIConnection::new(com_con.into()).expect("Failed to connect to WMI");

  let result: Vec<Win32_Battery> = wmi_con.query().unwrap_or_default();
  !result.is_empty()
}

#[cfg(test)]
mod tests {
  use super::*;

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
