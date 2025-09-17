pub mod modules;
pub mod networking;

use std::{sync::Arc, time::Duration};

use argh::FromArgs;
use log::{debug, error, info, trace};
use multiconnect_core::Packet;
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::{TcpListener, TcpStream},
  sync::{broadcast, mpsc, watch, Mutex},
  time,
};

pub type SharedDaemon = Arc<Daemon>;

const OUTGOING_BUFFER_SIZE: usize = 30;

#[derive(Debug, Clone)]
pub enum FrontendEvent {
  RecvPacket(Packet),
  Connected,
  Disconnected,
}

#[derive(FromArgs)]
#[argh(help_triggers("-h", "--help"))]
/// Sync devices
pub struct MulticonnectArgs {
  /// specify the port of the daemon to run on (default 10999)
  #[argh(option, default = "10999", short = 'p')]
  pub port: u16,
  /// specify the log level (default is info) {trace|debug|info|warn|error}
  #[argh(option, default = "String::from(\"info\")")]
  pub log_level: String,
}

#[derive(Debug)]
pub struct Daemon {
  /// The [`TcpListener`] for the daemon
  listener: TcpListener,

  /// Incoming packet sender form clients
  incoming_tx: broadcast::Sender<FrontendEvent>,
  /// Incoming packet receiver form clients
  incoming_rx: broadcast::Receiver<FrontendEvent>,

  /// Outgoing packet sender to clients
  outgoing_tx: mpsc::Sender<Packet>,
  /// Outgoing packet receiver to clients
  outgoing_rx: Arc<Mutex<mpsc::Receiver<Packet>>>,
}

// TODO: Clean all this up
impl Daemon {
  /// Create a new daemon and bind too a port (`MC_PORT` env var)
  pub async fn new(port: u16) -> Result<SharedDaemon, std::io::Error> {
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

    let daemon = Arc::new(Self {
      listener,
      incoming_tx,
      incoming_rx,
      outgoing_tx,
      outgoing_rx: Arc::new(Mutex::new(outgoing_rx)),
    });

    Ok(daemon)
  }

  /// Start accepting and handling incoming connections
  pub async fn start(&self) {
    loop {
      match self.listener.accept().await {
        Ok((stream, addr)) => {
          info!("New connection from {}", addr);
          self.handle(stream).await;
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
  /// ### Arguments:
  /// * `stream` - The [`TcpStream`] for the client connect
  async fn handle(&self, stream: TcpStream) {
    let _ = self.incoming_tx.send(FrontendEvent::Connected);
    let (mut read_half, mut write_half) = stream.into_split();

    let (shutdown_tx, mut shutdown_rx) = watch::channel(());
    let mut outgoing_lock = self.outgoing_rx.lock().await;

    let mut outgoing_buf = Vec::with_capacity(OUTGOING_BUFFER_SIZE);

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
                  let packet = match Packet::from_bytes(&raw) {
                  Ok(p) => p,
                  Err(e) => {
                    error!("Error decoding packet {}", e);
                    continue;
                  }
                };

                trace!("Received {:?} packet", packet);

                if let Err(e) = self.incoming_tx.send(FrontendEvent::RecvPacket(packet)) {
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

        _ = outgoing_lock.recv_many(&mut outgoing_buf, OUTGOING_BUFFER_SIZE) => {
          for packet in outgoing_buf.drain(..) {
            let bytes = Packet::to_bytes(&packet).unwrap();
            trace!("Raw packet: {:?}", bytes);
            if let Err(e) = write_half.write_all(&bytes).await {
              error!("Write error: {}", e);
            }
          }
          let _ = write_half.flush().await;
        }

        _ = shutdown_rx.changed() => {
          break;
        }
      }
    }

    let _ = self.incoming_tx.send(FrontendEvent::Disconnected);
  }

  /// Get the stream for sending packets
  pub fn send_packet_channel(&self) -> mpsc::Sender<Packet> {
    self.outgoing_tx.clone()
  }

  /// Get a channel for incoming packets
  pub fn recv_packet_channel(&self) -> broadcast::Receiver<FrontendEvent> {
    self.incoming_rx.resubscribe()
  }
}
