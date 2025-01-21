use std::{collections::VecDeque, ops::Not, sync::Arc};

use log::{debug, error, info, warn};
use multiconnect_protocol::daemon::packets::{Packet, Ping};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::TcpSocket,
  sync::{Mutex, Notify},
};

const PORT: u16 = 10999;

pub struct Daemon {
  queue: Arc<Mutex<VecDeque<Packet>>>,
  notify: Arc<Notify>
}

impl Daemon {
  pub async fn new() -> Result<Arc<Mutex<Self>>, Box<dyn std::error::Error>> {
    let socket = TcpSocket::new_v4()?;
    let stream = match socket.connect(format!("127.0.0.1:{}", PORT).parse()?).await {
      Ok(s) => s,
      Err(e) => {
        error!("Failed to connect to daemon: {}", e);
        return Err(Box::new(e));
      }
    };
    info!("Connected to daemon");

    let queue = Arc::new(Mutex::new(VecDeque::new()));
    let notify = Arc::new(Notify::new());

    let queue_clone = Arc::clone(&queue);
    let notify_clone = Arc::clone(&notify);

    tokio::spawn(async move {
      let (mut read_half, mut write_half) = stream.into_split();

      let read_queue_clone = Arc::clone(&queue_clone);
      let read_task = tokio::spawn(async move {
        loop {
          match read_half.read_u16().await {
            Ok(len) => {
              let mut raw: Vec<u8> = vec![0u8; len.into()];
              match read_half.read_exact(&mut raw).await {
                Ok(_) => {
                  let packet = match Packet::from_bytes(&raw) {
                    Ok(p) => p,
                    Err(e) => {
                      error!("Error decoding packet {}", e);
                      continue;
                    }
                  };

                  debug!("Received {:?} packet", packet);

                  let mut locked = read_queue_clone.lock().await;
                  match packet {
                    Packet::Acknowledge(acknowledge) => {
                      debug!("Received ack for ping req {}", acknowledge.req_id);
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
                    _ => {
                      error!("Received unexpected packet")
                    }
                  };
                }
                Err(e) => {
                  error!("Read error: {}", e);
                  continue;
                }
              };
            }
            Err(_) => {
              info!("Connection closed by peer");
              break;
            }
          }
        }
      });

      let write_task = tokio::spawn(async move {
        loop {
          notify_clone.notified().await;
          let mut locked = queue_clone.lock().await;
          while let Some(packet) = locked.pop_back() {
            debug!("Sending {:?} packet", packet);
            let bytes = Packet::to_bytes(packet);
            match bytes {
              Ok(b) => {
                if let Err(e) = write_half.write_all(&b).await {
                  error!("Write error: {}", e);
                  break;
                };
                let _ = write_half.flush().await;
              }
              Err(_) => todo!(),
            }
          }
        }
      });

      let _ = tokio::try_join!(read_task, write_task);
    });

    Ok(Arc::new(Mutex::new(Self { notify, queue })))
  }

  pub async fn ping(&mut self) {
    self.add_to_queue(Packet::Ping(Ping::new())).await;
  }

  pub async fn add_to_queue(&mut self, packet: Packet) {
    self.queue.lock().await.push_front(packet);
    self.notify.notify_one();
  }
}
