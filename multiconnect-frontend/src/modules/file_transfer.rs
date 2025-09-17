use std::{any::Any, error::Error, str::FromStr, sync::Arc};

use async_trait::async_trait;
use bincode::de;
use libp2p_core::PeerId;
use log::debug;
use multiconnect_core::{local::transfer::L9TransferFile, Packet};
use tauri::{Emitter, State};
use tokio::sync::Mutex;

use crate::{daemon::DaemonEvent, with_ctx};

use super::{FrontendCtx, FrontendModule, FrontendModuleManager};

pub struct FileTransferModule;

#[async_trait]
impl FrontendModule for FileTransferModule {
  async fn init(&mut self, _ctx: Arc<Mutex<FrontendCtx>>) -> Result<(), Box<dyn Error>> {
    Ok(())
  }

  async fn on_event(&mut self, event: DaemonEvent, ctx: &mut FrontendCtx) -> Result<(), Box<dyn Error>> {
    if let DaemonEvent::PacketReceived { packet } = event {
      match packet {
        Packet::L10TransferProgress(packet) => {
          ctx.app.emit(
            &format!("file_transfer/{}_progress", packet.direction().as_str_name().to_lowercase()),
            (packet.uuid, packet.file_name, packet.total, packet.done),
          )?;
        }
        Packet::L11TransferStatus(packet) => {
          debug!("File transfer status: {:?}", packet);
          ctx.app.emit("file_transfer/status", (&packet.uuid, packet.status()))?;
        }
        _ => {}
      }
    }
    Ok(())
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
    self
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

#[tauri::command]
pub async fn send_file(manager: State<'_, FrontendModuleManager>, peer: String, file_path: String) -> Result<(), ()> {
  with_ctx!(manager, |ctx| {
    ctx
      .do_action(crate::modules::FrontendAction::SendPacket {
        packet: Packet::L9TransferFile(L9TransferFile::new(PeerId::from_str(&peer).unwrap(), file_path)),
      })
      .await;
    Ok(())
  })
}
