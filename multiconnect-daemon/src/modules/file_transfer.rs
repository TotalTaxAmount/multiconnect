use async_trait::async_trait;
use libp2p::PeerId;
use log::{debug, info, warn};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use uuid::Uuid;

use multiconnect_protocol::Packet;

use crate::{networking::NetworkEvent, FrontendEvent};

use super::{MulticonnectCtx, MulticonnectModule};

pub struct FileTransferModule {
  ongoing_transfers: Arc<Mutex<HashMap<Uuid, FileTransferState>>>,
}

struct FileTransferState {
  total_len: u64,
  received_len: u64,
  file_id: Uuid,
  peer_id: PeerId,
}

impl FileTransferModule {
  pub async fn new() -> Self {
    Self { ongoing_transfers: Arc::new(Mutex::new(HashMap::new())) }
  }
}

#[async_trait]
impl MulticonnectModule for FileTransferModule {
  async fn on_network_event(&mut self, event: NetworkEvent, ctx: &mut MulticonnectCtx) {
    // match event {
    // NetworkEvent::TransferStart(peer_id, packet) => {
    //   let transfer_start: TransferStart = packet.try_into().unwrap();
    //   let file_id = Uuid::new_v4();
    //   self.ongoing_transfers.lock().await.insert(
    //     file_id,
    //     FileTransferState { total_len: transfer_start.total_len, received_len: 0, file_id, peer_id },
    //   );
    //   ctx.send_to_frontend(Packet::TransferStatus(TransferStatus { id: file_id, status: Status::OK })).await;
    // }
    // NetworkEvent::TransferChunk(peer_id, packet) => {
    //   let transfer_chunk: TransferChunk = packet.try_into().unwrap();
    //   if let Some(transfer_state) = self.ongoing_transfers.lock().await.get_mut(&transfer_chunk.id) {
    //     transfer_state.received_len += transfer_chunk.len;
    //     if transfer_state.received_len == transfer_state.total_len {
    //       ctx
    //         .send_to_frontend(Packet::TransferStatus(TransferStatus { id: transfer_chunk.id, status: Status::OK }))
    //         .await;
    //     }
    //   }
    // }
    // NetworkEvent::TransferEnd(peer_id, packet) => {
    //   let transfer_end: TransferEnd = packet.try_into().unwrap();
    //   if let Some(_) = self.ongoing_transfers.lock().await.remove(&transfer_end.id) {
    //     debug!("Transfer {} completed", transfer_end.id);
    //   }
    // }
    // _ => {}
  }

  async fn on_frontend_event(&mut self, event: FrontendEvent, ctx: &mut MulticonnectCtx) {
    // match event {
    //   FrontendEvent::SendFile(file_path, peer_id) => {
    //     let file_id = Uuid::new_v4();
    //     let total_len = std::fs::metadata(&file_path).unwrap().len();
    //     let transfer_start = TransferStart { id: file_id, total_len };

    //     let _ = ctx.send_to_network(Packet::TransferStart(transfer_start)).await;

    //     let mut file = std::fs::File::open(file_path).unwrap();
    //     let mut buffer = Vec::new();
    //     while let Ok(n) = file.read_to_end(&mut buffer) {
    //       if n == 0 {
    //         break;
    //       }
    //       let chunk = TransferChunk { id: file_id, len: n as u64, data: buffer.clone() };
    //       let _ = ctx.send_to_network(Packet::TransferChunk(chunk)).await;
    //     }

    //     let transfer_end = TransferEnd { id: file_id };
    //     let _ = ctx.send_to_network(Packet::TransferEnd(transfer_end)).await;
    //   }
    //   _ => {}
    // }
  }

  async fn init(&mut self, ctx: Arc<Mutex<MulticonnectCtx>>) {
    // Initialization logic for transfer module, if needed
  }
}
