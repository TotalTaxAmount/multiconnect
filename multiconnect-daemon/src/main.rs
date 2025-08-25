use fern::colors::{Color, ColoredLevelConfig};
use multiconnect_config::ConfigManager;
use multiconnect_daemon::{
  modules::{debug::DebugModule, file_transfer::FileTransferModule, pairing::PairingModule, ModuleManager},
  networking::NetworkManager,
  Daemon, MulticonnectArgs,
};
use std::{error::Error, str::FromStr, time::SystemTime};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let args: MulticonnectArgs = argh::from_env();

  let colors = ColoredLevelConfig::new().error(Color::Red).warn(Color::Yellow).info(Color::Green).debug(Color::Blue);

  fern::Dispatch::new()
    .format(move |out, message, record| {
      let bold_start = "\u{001b}[1m"; // Bold
      let reset = "\u{001b}[0m"; // Reset

      out.finish(format_args!(
        "{} [{}] [{bold_start}{}{reset}] > {}",
        humantime::format_rfc3339(SystemTime::now()),
        colors.color(record.level()),
        record.target(),
        message
      ))
    })
    .level(log::LevelFilter::from_str(&args.log_level)?)
    .level_for("netlink_proto", log::LevelFilter::Off)
    .level_for("netlink_packet_route", log::LevelFilter::Off)
    .chain(std::io::stdout())
    .apply()?;

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
