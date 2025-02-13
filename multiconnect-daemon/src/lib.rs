pub mod config;
pub mod networking;

use std::{collections::VecDeque, sync::Arc, time::Duration};

use log::{debug, error, info, trace};
use multiconnect_protocol::Packet;
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::{TcpListener, TcpStream},
  sync::{mpsc, watch, Mutex, Notify},
  time,
};

pub type SharedDaemon = Arc<Daemon>;
type Queue = Arc<(Notify, Mutex<VecDeque<Packet>>)>;

const PORT: u16 = 10999;

#[derive(Debug)]
pub struct Daemon {
  listener: TcpListener,
  queue: Queue,
  packet_tx: mpsc::Sender<Packet>,
  packet_rx: mpsc::Receiver<Packet>,
}

// TODO: Clean all this up
impl Daemon {
  /// Create a new daemon and bind too a port (`MC_PORT` env var)
  pub async fn new() -> Result<SharedDaemon, std::io::Error> {
    let port = std::env::var("MC_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(PORT);

    let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)).await {
      Ok(l) => l,
      Err(e) => {
        error!("Failed to start daemon (is it already running?)");
        return Err(e);
      }
    };

    info!("Daemon listening on 127.0.0.1:{}", port);

    let (packet_tx, packet_rx) = mpsc::channel(100);

    let queue: Queue = Arc::new((Notify::new(), Mutex::new(VecDeque::new())));
    let daemon = Arc::new(Self { listener, queue, packet_tx, packet_rx });

    Ok(daemon)
  }

  /// Start accepting and handling incoming connections
  pub async fn start(&self) {
    loop {
      match self.listener.accept().await {
        Ok((stream, addr)) => {
          info!("New connection from {}", addr);
          let packet_tx_clone = self.packet_tx.clone();
          let queue_clone = Arc::clone(&self.queue);

          tokio::spawn(async move { Self::handle(stream, packet_tx_clone, queue_clone).await });
        }
        Err(e) => {
          error!("Failed to accept connection: {}", e);
          time::sleep(Duration::from_secs(1)).await;
        }
      }
    }
  }

  /// Handle a connection from a client
  /// Currently the same queue is used for every client, but it is indented to
  /// be used with one client so it is fine for now
  /// Arguments:
  /// * `stream` - The [`TcpStream`]
  /// * `packet_tx` - A [`mspc::Sender<Packet>`], received packets are send on
  ///   this channel
  /// * `queue` - A [`Queue`] of the packets to be sent, all future packets to
  ///   be sent should be added to this queue
  // TODO: Possibly use self in the future
  async fn handle(stream: TcpStream, packet_tx: mpsc::Sender<Packet>, queue: Queue) {
    let (mut read_half, mut write_half) = stream.into_split();

    let (shutdown_tx, mut shutdown_rx) = watch::channel(());
    let mut shutdown_rx_clone: watch::Receiver<()> = watch::Receiver::clone(&shutdown_rx);

    let read_task = tokio::spawn({
      let shutdown_tx = shutdown_tx.clone();
      async move {
        loop {
          tokio::select! {
            result = read_half.read_u16() => {
              match result {
                Ok(len) => {
                  trace!("Len: {}", len);
                  if len > u16::MAX {
                    error!("Packet is to big: {}", len);
                    continue;
                  }

                  let mut raw: Vec<u8> = vec![0u8; len.into()];
                  match read_half.read_exact(&mut raw).await {
                    Ok(_) => {
                      trace!("Bytes: {:?}", raw);
                      let packet = match Packet::from_bytes(&raw) {
                      Ok(p) => p,
                      Err(e) => {
                        error!("Error decoding packet {}", e);
                        continue;
                      }
                    };

                    debug!("Received {:?} packet", packet);

                    if let Err(e) = packet_tx.send(packet).await {
                      error!("Failed to add send packet (local): {}", e);
                    };
                  }
                  Err(e) => {
                    error!("Read error: {}", e);
                    break;
                  }
                }
              }
              Err(_) => {
                  info!("Connection closed by peer");
                  let _ = shutdown_tx.send(());
                  break;
                }
              }
            }
            _ = shutdown_rx_clone.changed() => {
              break;
            }
          }
        }
      }
    });

    let write_task = tokio::spawn(async move {
      loop {
        tokio::select! {
          _ = queue.0.notified() => {
            let mut locked = queue.1.lock().await;

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
          _ = shutdown_rx.changed() => {
            info!("Shutting down stream for client");
            break;
          }
        }
      }
    });

    let _ = tokio::try_join!(read_task, write_task);
  }

  /// Add a packet to the queue.
  ///
  /// Arguments:
  /// * `packet` - A [`Packet`] to be sent (will be sent to all connected
  ///   clients)
  pub async fn add_to_queue(&self, packet: Packet) {
    {
      let mut locked = self.queue.1.lock().await;
      locked.push_front(packet);
    }
    self.queue.0.notify_one();
  }

  /// Await a packet to be received
  pub async fn on_packet(&mut self) -> Option<Packet> {
    self.packet_rx.recv().await
  }
}
