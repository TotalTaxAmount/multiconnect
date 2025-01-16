use log::{error, info};
use tokio::net::TcpSocket;

const PORT: u16 = 10999;

pub struct Daemon {}

impl Daemon {
  pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
    let socket = TcpSocket::new_v4()?;
    let mut stream = match socket.connect(format!("127.0.0.1:{}", PORT).parse()?).await {
        Ok(s) => s,
        Err(e) => {
          error!("Failed to connect to daemon: {}", e);
          return Err(Box::new(e));
        },
    };
    info!("Connected to daemon");

    todo!()
  }
}