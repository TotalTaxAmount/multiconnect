use multiconnect_config::ConfigManager;
use multiconnect_daemon::{
  modules::{debug::DebugModule, file_transfer::FileTransferModule, pairing::PairingModule, ModuleManager},
  networking::NetworkManager,
  Daemon, MulticonnectArgs,
};
use std::{error::Error, time::SystemTime};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let args: MulticonnectArgs = argh::from_env();

  multiconnect_core::init_tracing(&args.log_level, args.port + 1)?;

  ConfigManager::init().await;
  let daemon = Daemon::new(args.port).await?;
  let mut network_manager = NetworkManager::new().await?;
  let pairing_module =
    PairingModule::new(network_manager.send_command_channel(), network_manager.get_mc_event_recv().unwrap()).await;
  let module_manager = Box::leak(Box::new(ModuleManager::new(network_manager, daemon.clone()).await));
  module_manager.register(pairing_module);
  module_manager.register(FileTransferModule::new().await);
  module_manager.register(DebugModule::new());
  let _ = module_manager.start().await;
  Ok(())
}
