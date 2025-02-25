use multiconnect_daemon::{networking::NetworkManager, Daemon};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  if std::env::var("MC_LOG").is_err() {
    std::env::set_var("MC_LOG", "info");
  }

  pretty_env_logger::formatted_timed_builder().parse_env("MC_LOG").format_timestamp_secs().init();

  let daemon = Daemon::new().await?;
  NetworkManager::start(daemon.clone()).await?;

  let _ = daemon.start().await;

  Ok(())
}
