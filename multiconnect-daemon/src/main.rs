use multiconnect_daemon::{networking::NetworkManager, Daemon};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  if std::env::var("MULTICONNECT_LOG").is_err() {
    std::env::set_var("MULTICONNECT_LOG", "info");
  }

  pretty_env_logger::formatted_timed_builder().parse_env("MULTICONNECT_LOG").format_timestamp_secs().init();

  let daemon = Daemon::new().await?;
  let network_manager = NetworkManager::new(daemon.clone()).await;


  let _ = daemon.start().await;

  Ok(())
}
