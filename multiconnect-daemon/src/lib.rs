pub mod config;
pub mod networking;

use std::{collections::VecDeque, sync::Arc, time::Duration};

use log::{debug, error, info, trace};
use multiconnect_protocol::Packet;
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::{TcpListener, TcpStream},
  sync::{broadcast::{self, error::RecvError}, mpsc, watch, Mutex, Notify, RwLock},
  time,
};

pub type SharedDaemon = Arc<Daemon>;
type Queue = Arc<(Notify, Mutex<VecDeque<Packet>>)>;

const PORT: u16 = 10999;

#[derive(Debug)]
pub struct Daemon {
  /// The [`TcpListener`] for the daemon
  listener: TcpListener,

  /// Incoming packet sender form clients
  incoming_tx: broadcast::Sender<Packet>,
  /// Incoming packet receiver form clients
  incoming_rx: broadcast::Receiver<Packet>,

  /// Outgoing packet sender to clients
  outgoing_tx: mpsc::Sender<Packet>,
  /// Outgoing packet receiver to clients
  outgoing_rx: Arc<RwLock<mpsc::Receiver<Packet>>>,
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

    let (incoming_tx, incoming_rx) = broadcast::channel(100);
    let (outgoing_tx, outgoing_rx) = mpsc::channel(100);

    // let queue: Queue = Arc::new((Notify::new(), Mutex::new(VecDeque::new())));
    let daemon = Arc::new(Self { listener, incoming_tx, incoming_rx, outgoing_tx, outgoing_rx: Arc::new(RwLock::new(outgoing_rx)) });

    Ok(daemon)
  }

  /// Start accepting and handling incoming connections
  pub async fn start(&self) {
    loop {
      match self.listener.accept().await {
        Ok((stream, addr)) => {
          info!("New connection from {}", addr);
          let incoming_tx = self.incoming_tx.clone();
          let outgoing_rx = self.outgoing_rx.clone();

          tokio::spawn(async move { Self::handle(stream, incoming_tx, outgoing_rx).await });
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
  async fn handle(stream: TcpStream, incoming_tx: broadcast::Sender<Packet>, outgoing_rx: Arc<RwLock<mpsc::Receiver<Packet>>>) {
    let (mut read_half, mut write_half) = stream.into_split();

    let (shutdown_tx, mut shutdown_rx) = watch::channel(());

    loop {
      tokio::select! {
        res = read_half.read_u16() => {
          match res {
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

                if let Err(e) = incoming_tx.send(packet) {
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

        mut w_lock = outgoing_rx.write() => {
          let res = w_lock.recv().await;
          match res {
            Some(p) => {
              let bytes = Packet::to_bytes(p).unwrap();
              if let Err(e) = write_half.write_all(&bytes).await {
                error!("Write error: {}", e);
              }

              let _ = write_half.flush().await;
            },
            None => todo!(),
          }
        }

        _ = shutdown_rx.changed() => {
          break;
        }
      }
    }
  }

  /// Add a packet to the queue.
  ///
  /// Arguments:
  /// * `packet` - A [`Packet`] to be sent (will be sent to all connected
  ///   clients)
  pub async fn send_packet(&self, packet: Packet) {
    let _ = self.outgoing_tx.send(packet);
  }

  /// Await a packet to be received
  pub async fn on_packet(&mut self) -> Result<Packet, RecvError> {
    self.incoming_rx.recv().await
  }
}
