use std::{
  collections::{HashSet, VecDeque},
  ops::Not,
  sync::Arc,
};

use log::{debug, error, info, warn};
use multiconnect_protocol::{daemon::peer::PeerFound, p2p::Peer, Packet};
use serde::{Deserialize, Serialize};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::TcpSocket,
  sync::{mpsc, Mutex, Notify},
  time::Sleep,
};

type Queue = Arc<(Notify, Mutex<VecDeque<Packet>>)>;
type SharedDaemon = Arc<Mutex<Daemon>>;
const PORT: u16 = 10999;

pub struct Daemon {
  queue: Queue,
  packet_rx: mpsc::Receiver<Packet>,
}

impl Daemon {
  /// Connect to the daemon and establish a [`TcpStream`]
  pub async fn connect() -> Result<SharedDaemon, Box<dyn std::error::Error>> {
    let socket = TcpSocket::new_v4()?;
    let stream = match socket.connect(format!("127.0.0.1:{}", PORT).parse()?).await {
      Ok(s) => s,
      Err(e) => {
        error!("Failed to connect to daemon: {}", e);
        return Err(Box::new(e));
      }
    };
    info!("Connected to daemon");

    let queue: Queue = Arc::new((Notify::new(), Mutex::new(VecDeque::new())));
    let queue_clone = Arc::clone(&queue);

    let (packet_tx, packet_rx) = mpsc::channel(100);

    tokio::spawn(async move {
      let (mut read_half, mut write_half) = stream.into_split();

      let read_task = tokio::spawn(async move {
        loop {
          match read_half.read_u16().await {
            Ok(len) => {
              if len > u16::MAX {
                error!("Packet is to big: {}", len);
                continue;
              }

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

                  if let Err(e) = packet_tx.send(packet).await {
                    error!("Error sending packet (local): {}", e);
                  }

                  // let mut locked = read_queue_clone.lock().await;
                  // match packet {
                  //   Packet::Acknowledge(acknowledge) => {
                  //     debug!("Received ack for ping req {}",
                  // acknowledge.req_id);   }
                  //   Packet::PeerFound(peer_found) => todo!(),
                  //   Packet::PeerPairRequest(peer_pair_request) => todo!(),
                  //   Packet::PeerConnect(peer_connect) => todo!(),
                  //   Packet::TransferStart(transfer_start) => todo!(),
                  //   Packet::TransferChunk(transfer_chunk) => todo!(),
                  //   Packet::TransferEnd(transfer_end) => todo!(),
                  //   Packet::TransferStatus(transfer_status) => todo!(),
                  //   Packet::SmsMessage(sms_message) => todo!(),
                  //   Packet::Notify(notify) => todo!(),
                  //   _ => {
                  //     error!("Received unexpected packet")
                  //   }
                  // };
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
          queue_clone.0.notified().await;
          let mut locked = queue_clone.1.lock().await;
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

    Ok(Arc::new(Mutex::new(Self { queue, packet_rx })))
  }

  /// Add a packet to the queue
  pub async fn add_to_queue(&mut self, packet: Packet) {
    self.queue.1.lock().await.push_front(packet);
    self.queue.0.notify_one();
  }

  pub async fn on_packet(&mut self) -> Option<Packet> {
    self.packet_rx.recv().await
  }
}

pub struct DaemonController {
  peers: Arc<Mutex<HashSet<Peer>>>,
}

impl DaemonController {
  pub async fn bind(daemon: SharedDaemon) -> Self {
    let peers = Arc::new(Mutex::new(HashSet::new()));
    let peers_clone = Arc::clone(&peers);

    tokio::spawn(async move {
      let mut daemon_lock = daemon.lock().await;
      loop {
        match daemon_lock.on_packet().await {
          Some(Packet::PeerFound(p)) => {
            peers_clone.lock().await.insert(bincode::deserialize(&p.peer).unwrap());
          }
          Some(_) | None => {
            warn!("Received a packet but it is None");
          }
        }
      }
    });

    Self { peers }
  }

  pub async fn get_peers(&self) -> Vec<Peer> {
    let locked = self.peers.lock().await;
    locked.clone().into_iter().collect()
  }
}
