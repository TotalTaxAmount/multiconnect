use async_trait::async_trait;
use blake3::Hasher;
use dashmap::DashMap;
use lazy_static::lazy_static;
use libp2p::{
  futures::{channel::mpsc, SinkExt, StreamExt},
  PeerId,
};
use log::{debug, info, trace, warn};
use multiconnect_config::CONFIG;
use std::{
  error::Error,
  io,
  path::{Path, PathBuf},
  str::FromStr,
  sync::Arc,
  u16,
};
use tokio::{
  fs::{self, File, OpenOptions},
  io::{AsyncReadExt, AsyncWriteExt, BufReader},
  sync::Mutex,
  time::Instant,
};
use uuid::Uuid;

use multiconnect_core::{
  generated::{P4TransferStart, P5TransferChunk, P6TransferStatus, P7TransferAck},
  local::transfer::{
    l10_transfer_progress,
    l11_transfer_status::{self, LtStatus},
    L10TransferProgress, L11TransferStatus, L9TransferFile,
  },
  Packet,
};

use crate::{networking::NetworkEvent, FrontendEvent};

use super::{MulticonnectCtx, MulticonnectModule};

pub struct FileTransferModule {
  transfers: Arc<DashMap<Uuid, FileTransfer>>,
  transfer_tx: mpsc::Sender<FileTransfer>,
  transfer_rx: Option<mpsc::Receiver<FileTransfer>>,
  chunk_tx: mpsc::Sender<P5TransferChunk>,
  chunk_rx: Option<mpsc::Receiver<P5TransferChunk>>,
}

const METADATA_SIZE: usize = 257;

#[derive(Debug, Clone)]
enum TransferDirection {
  Inbound,
  Outbound,
}

#[derive(Debug)]
struct FileTransfer {
  /// The uuid of the transfer
  uuid: Uuid,
  /// Total size of the file
  total_len: usize,
  /// The amount of the file that has been sent or recived
  processed_len: usize,
  /// The peer id of the sender or the reciver depending on the direction
  peer: PeerId,
  /// The file name
  file: String,
  /// The hash of the file
  hash: Option<String>,
  /// The direction of the transfer
  direction: TransferDirection,
  /// File handle
  file_handle: Option<File>,
}

impl Clone for FileTransfer {
  fn clone(&self) -> Self {
    Self {
      uuid: self.uuid,
      total_len: self.total_len,
      processed_len: self.processed_len,
      peer: self.peer.clone(),
      file: self.file.clone(),
      hash: self.hash.clone(),
      direction: self.direction.clone(),
      file_handle: None, // File handle should not be cloned, it is only used for writing
    }
  }
}

impl FileTransfer {
  pub fn new(
    uuid: Uuid,
    len: usize,
    peer: PeerId,
    file: impl Into<String>,
    hash: Option<String>,
    direction: TransferDirection,
    file_handle: Option<File>,
  ) -> Self {
    Self { uuid, total_len: len, processed_len: 0, peer, file: file.into(), hash, direction, file_handle }
  }
}

impl FileTransferModule {
  pub async fn new() -> Self {
    let (transfer_tx, transfer_rx) = mpsc::channel::<FileTransfer>(10);
    let (chunk_tx, chunk_rx) = mpsc::channel::<P5TransferChunk>(100);

    Self {
      transfers: Arc::new(DashMap::new()),
      transfer_rx: Some(transfer_rx),
      transfer_tx,
      chunk_tx,
      chunk_rx: Some(chunk_rx),
    }
  }

  /// Calculate the sha256 checksum of a file
  async fn hash_blake3<P: AsRef<Path>>(path: P) -> std::io::Result<String> {
    debug!("Hashing file: {}", path.as_ref().display());
    let mut file = BufReader::new(File::open(path).await?);
    let mut hasher = Hasher::new();
    let mut buf = vec![0u8; 16 * 1024];
    loop {
      let read = file.read(&mut buf).await?;
      if read == 0 {
        break;
      }
      hasher.update(&buf[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
  }

  /// Generate a unique file path from a file path
  /// Will check if file exists, if it does it will chage the name until the file doesnt exist
  /// For example, say the path is `/foo/bar.png` and it exists. The target path will become
  /// `/foo/bar (1).png` if that exists then `/foo/bar (2).png` and so on
  async fn generate_unique_file<P: AsRef<Path>>(base: P) -> io::Result<PathBuf> {
    let base = base.as_ref();
    let parent = base.parent().unwrap_or_else(|| Path::new("."));
    let stem = base.file_stem().unwrap_or_default().to_string_lossy();
    let ext = base.extension().map(|e| e.to_string_lossy()).unwrap_or(std::borrow::Cow::Borrowed(""));

    let mut path = base.to_path_buf();
    let mut c = 1;

    while path.exists() {
      let filename = format!("{} ({}).{}", stem, c, ext);
      path = parent.join(filename);
      c += 1;
    }

    Ok(path)
  }
}

#[async_trait]
impl MulticonnectModule for FileTransferModule {
  async fn on_network_event(&mut self, event: NetworkEvent, ctx: &mut MulticonnectCtx) -> Result<(), Box<dyn Error>> {
    // Only thing we care about is recivied packets from peers
    if let NetworkEvent::PacketReceived(source, packet) = event {
      match packet {
        // A peer has started a transfer to us
        Packet::P4TransferStart(packet) => {
          // Get the uuid for the transfer provided by the peer
          let uuid = Uuid::from_str(&packet.uuid)?;
          let cfg = &CONFIG.get().ok_or("Failed to get config")?.read().await;

          // Figure out where to save the file
          let base = Path::new(&cfg.get_config().modules.transfer.save_path); // Base path from config

          let file_name =
            Path::new(&packet.file_name).file_name().ok_or("Failed to get filename from path")?.to_os_string(); // File name from the peer

          let full_path = base.join(file_name); // The full save path

          let mut path = Self::generate_unique_file(&full_path).await?.to_string_lossy().to_string(); // Make sure we are not going to overwrite something
          path.push_str(".tmp");

          debug!("Received transfer start from {} for {:?}: uuid = {}", source, path, uuid);

          // Notfiy the frontend that we are transferring a file
          ctx
            .send_to_frontend(Packet::L10TransferProgress(L10TransferProgress::new(
              uuid,
              path.clone(),
              packet.file_size as u64,
              0, // We have not processed anything yet
              l10_transfer_progress::Direction::Inbound,
            )))
            .await;
          ctx.send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(uuid, LtStatus::Transfering))).await;

          // create the file handle
          let file_handle = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&path)
            .await
            .map_err(|e| format!("Failed to open file {}: {}", path, e))?;

          // Add transfer to know transfers
          self.transfers.insert(
            uuid,
            FileTransfer::new(
              uuid,
              packet.file_size as usize,
              source,
              path,
              Some(packet.signature),
              TransferDirection::Inbound,
              Some(file_handle),
            ),
          );
        }
        Packet::P5TransferChunk(packet) => {
          // Send transfer packet to other thread so we don't block other modules
          self.chunk_tx.send(packet).await?;
        }
        Packet::P6TransferStatus(packet) => match packet.status() {
          // A peer recivied a file successfully
          multiconnect_core::generated::p6_transfer_status::PtStatus::Ok => {
            ctx
              .send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(
                Uuid::from_str(&packet.uuid)?,
                l11_transfer_status::LtStatus::Ok,
              )))
              .await;
          }
          multiconnect_core::generated::p6_transfer_status::PtStatus::MalformedPacket => todo!(),
          // Signatures did not match and the peer did not save the file
          multiconnect_core::generated::p6_transfer_status::PtStatus::WrongSig => {
            // Notify the frontend about the failed transfer
            ctx
              .send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(
                Uuid::from_str(&packet.uuid)?,
                l11_transfer_status::LtStatus::WrongSig,
              )))
              .await;
          }
          multiconnect_core::generated::p6_transfer_status::PtStatus::Validating => {
            // Notify the frontend about the current status
            ctx
              .send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(
                Uuid::from_str(&packet.uuid)?,
                l11_transfer_status::LtStatus::Validating,
              )))
              .await;
          }
        },
        Packet::P7TransferAck(packet) => {
          // A peer has acknowledged a transfer chunk
          let uuid = Uuid::from_str(&packet.uuid)?;
          let mut transfer = self.transfers.get_mut(&uuid).ok_or(format!("No active transfer for uuid = {}", uuid))?;

          // Update the processed length of the transfer
          transfer.processed_len = packet.progress as usize;

          debug!("Recivied ack for transfer {}: {} bytes processed", uuid, transfer.processed_len);

          // Notify the frontend about the progress
          ctx
            .send_to_frontend(Packet::L10TransferProgress(L10TransferProgress::new(
              transfer.uuid,
              transfer.file.clone(),
              transfer.total_len as u64,
              transfer.processed_len as u64,
              l10_transfer_progress::Direction::Outbound,
            )))
            .await;
        }
        _ => {}
      }
    }

    Ok(())
  }

  async fn on_frontend_event(&mut self, event: FrontendEvent, ctx: &mut MulticonnectCtx) -> Result<(), Box<dyn Error>> {
    if let FrontendEvent::RecvPacket(packet) = event {
      match packet {
        Packet::L9TransferFile(packet) => {
          debug!("Received command to send file from fronted");
          // Get the size of the file we want to send
          let len = fs::metadata(&packet.file_path).await?.len() as usize;
          // Target peer address
          let peer_id = PeerId::from_str(&packet.target)?;

          let uuid = Uuid::new_v4();

          debug!("Sending on channel");
          let transfer = FileTransfer::new(
            uuid,
            len,
            peer_id,
            packet.file_path,
            None, /* Hash is none for now */
            TransferDirection::Outbound,
            None,
          );

          self.transfers.insert(uuid, transfer.clone());
          self.transfer_tx.send(transfer).await?;
        }
        _ => {}
      }
    }
    Ok(())
  }

  async fn init(&mut self, ctx: Arc<Mutex<MulticonnectCtx>>) -> Result<(), Box<dyn Error>> {
    let mut transfer_rx = self.transfer_rx.take().unwrap();
    let mut chunk_rx = self.chunk_rx.take().unwrap();
    let max_bps = CONFIG.get().unwrap().read().await.get_config().modules.transfer.max_bps;

    let transfers = self.transfers.clone();

    let mut bytes_since_last_update = 0;

    // Thread to handle reading and writing packets
    tokio::spawn(async move {
      loop {
        tokio::select! {
          // We have a transfer to send
          transfer = transfer_rx.next() => if let Some(transfer) = transfer {

            debug!("Recivied transfer to send: {:?}", transfer);
            // Send one progress packet to the frontend
            ctx.lock().await.send_to_frontend(Packet::L10TransferProgress(L10TransferProgress::new(
              transfer.uuid,
              transfer.file.clone(),
              transfer.total_len as u64,
              0, // We have not sent anything yet
              l10_transfer_progress::Direction::Outbound,
            ))).await;
            // Notify frontend that we are preparing the transfer
            ctx.lock().await.send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(transfer.uuid, LtStatus::Preparing))).await;

            // Now we get the hash when we wont block other things
            let hash = match Self::hash_blake3(&transfer.file).await {
              Ok(hash) => hash,
              Err(e) => {
                warn!("Failed to hash file {}: {}", transfer.file, e);
                // Notify frontend
                let guard = ctx.lock().await;
                guard
                  .send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(
                    transfer.uuid,
                    l11_transfer_status::LtStatus::WrongSig,
                  )))
                  .await;
                continue;
              }
            };

            // Notify frontend that we are starting the transfer
            ctx.lock().await.send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(transfer.uuid, LtStatus::Transfering))).await;

            debug!("Starting transfer: {}, sig = {}", transfer.file, hash);

            // Create a uuid for this transfer

            // Get this size for a chunk, make sure to leave room for the reset of the transfer packet
            let chunk_size = u16::MAX as usize - METADATA_SIZE;
            let ctx = ctx.clone();
            let transfer_clone = transfer.clone();

            // Do the acuall transfer
            let result: Result<(), String> = async move {
              let path = Path::new(&transfer.file);

              debug!("Sending start packet");

              // Send transfer start packet
              ctx
                .lock()
                .await
                .send_to_peer(
                  transfer.peer,
                  Packet::P4TransferStart(P4TransferStart::new(
                    transfer.total_len as u64,
                    transfer.file.clone(),
                    transfer.uuid.to_string(),
                    hash /* Now we have the hash */,
                  )),
                )
                .await;


              // Open the file we want to send
              let mut file = File::open(path).await.map_err(|e| e.to_string())?;

              let mut last_check = Instant::now();
              let mut allowance = max_bps; // Allowance in bytes
              // Create a buffer to write into
              let mut buf = vec![0u8; chunk_size];
              loop {
                // Read into the buffer
                let read = file.read(&mut buf).await.map_err(|e| e.to_string())?;
                if read == 0 {
                  debug!("Done reading file");
                  // We have read the whole file
                  break;
                }
                let now = Instant::now();
                let elapsed = now.duration_since(last_check);
                last_check = now;

                allowance = (allowance + (elapsed.as_secs_f64() * allowance as f64) as usize)
                  .min(max_bps);

                if read > allowance {
                  // We have to wait before sending more data
                  let wait_time = (read as f64 / max_bps as f64) * 1000.0; // Convert to milliseconds
                  debug!("Waiting for {} ms", wait_time);
                  tokio::time::sleep(tokio::time::Duration::from_millis(wait_time as u64)).await;
                  allowance = 0; // Reset allowance
                } else {
                  allowance -= read; // Decrease allowance by the amount we read
                }

                // Data to send
                let data = buf[..read].to_vec();
                buf.resize(chunk_size, 0);
                let guard = ctx.lock().await;

                // Send chunk to targret peer
                debug!("Sending chunk with {} bytes", read);
                guard
                  .send_to_peer(transfer.peer.clone(), Packet::P5TransferChunk(P5TransferChunk::new(transfer.uuid, data)))
                  .await;
              }

              Ok(())
            }.await;

            if let Err(e) = result {
              warn!("Failed to send file {} to peer {}: {}", transfer_clone.file, transfer_clone.peer, e);
            }
          },
          // Recivied a chunk for an incoming transfer
          chunk_packet = chunk_rx.next() => if let Some(chunk_packet) = chunk_packet {
            debug!("Recivied chunk");
            // Get the uuid of the transfer from the packet
            let uuid = Uuid::from_str(&chunk_packet.uuid).unwrap();

            // Get the transfer (added from P4TransferStart)
            if let Some(mut transfer) = transfers.get_mut(&uuid) {
              // Get the path to save the file (currenttly it is a temp file) and open it
              let file_clone = transfer.file.clone();
              let path = Path::new(&file_clone);
              let pretty_name = path.file_name().ok_or("Failed to get filename").unwrap().to_string_lossy().strip_suffix(".tmp").unwrap_or("unknown").to_string();

              match transfer.file_handle {
                Some(ref mut _file) => {
                  // If the file handle is Some, we can use it
                  trace!("Using existing file handle for {}", path.display());
                }
                None => {
                  // If the file handle is None, we need to open the file
                  warn!("File handle is None, opening file {}", path.display());
                  transfer.file_handle = Some(
                    OpenOptions::new()
                      .create(true)
                      .append(true)
                      .open(&path)
                      .await
                      .map_err(|e| format!("Failed to open file {}: {}", path.display(), e)).unwrap(),
                  );
                }
              }

              let file = transfer.file_handle.as_mut().unwrap();

              // Write the data from the chunk packet to the file
              if let Ok(len) = file.write(&chunk_packet.data).await {
                // Update transfer progress
                transfer.processed_len += len;

                // Check if all data has been transfered
                if transfer.processed_len == transfer.total_len {
                  let guard = ctx.lock().await;
                  // Notfiy peer that the progress is complete
                  guard.send_to_peer(transfer.peer, Packet::P7TransferAck(P7TransferAck::new(uuid, transfer.total_len as u64))).await;
                  guard.send_to_peer(transfer.peer, Packet::P6TransferStatus(P6TransferStatus::new(uuid, multiconnect_core::generated::p6_transfer_status::PtStatus::Validating))).await;

                  // Notify frontend that the transfer is complete
                  guard.send_to_frontend(Packet::L10TransferProgress(L10TransferProgress::new(
                    transfer.uuid,
                    pretty_name,
                    transfer.total_len as u64,
                    transfer.total_len as u64,
                    l10_transfer_progress::Direction::Inbound,
                  ))).await;
                  guard.send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(uuid, LtStatus::Validating))).await;

                  debug!("Transfer complete, validating file");
                  // Get the real signature
                  let sig = Self::hash_blake3(path).await.unwrap();
                  // Get the expected signature
                  let hash = transfer.hash.clone().ok_or("Failed to get hash").unwrap();

                  // Confirm signatures match
                  if sig == hash {
                    // Figure out the final save path
                    let file_name = path.file_name().ok_or("Failed to get filename").unwrap().to_string_lossy();
                    let file_name = file_name.strip_suffix(".tmp").ok_or("Expected .tmp suffix").unwrap();
                    let parent = path.parent().ok_or("Failed to get parent directory").unwrap();
                    let final_path = parent.join(file_name);

                    // Move temp download file to the final path
                    tokio::fs::rename(path, &final_path).await.unwrap();
                    debug!("Successfully saved file ({} bytes)", transfer.total_len);

                    // Notify frontend
                    guard
                      .send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(
                        transfer.uuid,
                        l11_transfer_status::LtStatus::Ok,
                      )))
                      .await;

                    guard.send_to_peer(transfer.peer, Packet::P6TransferStatus(P6TransferStatus::new(uuid, multiconnect_core::generated::p6_transfer_status::PtStatus::Ok))).await;
                  } else {
                    warn!("File signature doesnt match: {} != {}", sig, hash);
                    // Delete whatever was downloaded
                    // let _ = fs::remove_file(path).await;

                    // Notify frontend
                    guard
                      .send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(
                        transfer.uuid,
                        l11_transfer_status::LtStatus::WrongSig,
                      )))
                      .await;

                    guard.send_to_peer(transfer.peer, Packet::P6TransferStatus(P6TransferStatus::new(uuid, multiconnect_core::generated::p6_transfer_status::PtStatus::WrongSig))).await;
                  }
                  drop(transfer);
                  transfers.remove(&uuid);
                } else {
                  // Update sender about the progress if its time
                  if bytes_since_last_update >= 1024 * 1024 * 4 {
                    debug!("Sending ack for {} bytes processed", transfer.processed_len);
                    let guard = ctx.lock().await;
                    guard
                      .send_to_peer(transfer.peer.clone(), Packet::P7TransferAck(P7TransferAck::new(uuid, transfer.processed_len as u64)))
                      .await;
                    bytes_since_last_update = 0;
                  } else {
                    bytes_since_last_update += len;
                  }

                  // Update frontend progress
                  let guard = ctx.lock().await;
                  guard
                  .send_to_frontend(Packet::L10TransferProgress(L10TransferProgress::new(
                    transfer.uuid,
                    pretty_name,
                    transfer.total_len as u64,
                    transfer.processed_len as u64,
                    l10_transfer_progress::Direction::Inbound,
                  )))
                  .await;
                }
              } else {
                warn!("Failed to write file");
              }
            } else {
              warn!("No transfer with uuid: {}", uuid);
            }
          }
        }
      }
    });
    Ok(())
  }
}
