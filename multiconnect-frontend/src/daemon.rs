use log::{error, info};
use multiconnect_protocol::message::{MsgType, ProtocolMessage};
use std::{collections::{HashMap, VecDeque}, error::Error, sync::Arc};
use tokio::{
  io::AsyncWriteExt,
  net::{TcpSocket, TcpStream}, sync::Mutex,
};

const PORT: u16 = 10999;

pub struct Daemon {
  queue: Arc<Mutex<VecDeque<ProtocolMessage>>>,
  pending: Arc<Mutex<HashMap<u8, ProtocolMessage>>>
}

impl Daemon {
  pub async fn new() -> Result<Self, Box<dyn Error>> {
    let socket = TcpSocket::new_v4()?;
    let mut stream = match socket.connect(format!("127.0.0.1:{}", PORT).parse()?).await {
      Ok(s) => s,
      Err(e) => {
        error!("Failed to connect to daemon: {}", e);
        return Err(Box::new(e));
      }
    };
    info!("Connected to daemon");
    
    let queue: Arc<Mutex<VecDeque<ProtocolMessage>>> = Arc::new(Mutex::new(VecDeque::new()));
    let pending: Arc<Mutex<HashMap<u8, ProtocolMessage>>> = Arc::new(Mutex::new(HashMap::new()));

    let queue_clone = queue.clone();

    tokio::spawn(async move {
      loop {
        if let Some(p) = queue_clone.lock().await.pop_back() {
          let _ = stream.write_all(&p.to_bytes()).await;
          let _ = stream.flush().await;
        }
      }
    });

    Ok(Self { queue, pending })
  }

  pub async fn ping(&mut self) {
    let packet = ProtocolMessage::new(MsgType::Ping, vec![]);
    self.add_to_pending(packet.clone()).await;
    self.add_to_queue(packet).await;
  }

  async fn add_to_queue(&mut self, packet: ProtocolMessage) {
    self.queue.lock().await.push_front(packet);
  }

  async fn add_to_pending(&mut self, packet: ProtocolMessage) {
    self.pending.lock().await.insert(packet.id, packet);
  }
}
