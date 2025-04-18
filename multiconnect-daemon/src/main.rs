use fern::colors::{Color, ColoredLevelConfig};
use multiconnect_daemon::{
  modules::{discovery::Discovery, pairing::PairingModule, ModuleManager, ModuleTest, MulticonnectCtx},
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
    .chain(std::io::stdout())
    .apply()?;

  let daemon = Daemon::new(args.port).await?;

  let network_manager = NetworkManager::start().await?;
  let module_manager = Box::leak(Box::new(ModuleManager::new(network_manager, daemon.clone())));
  module_manager.register(PairingModule::new());
  module_manager.register(Discovery);

  module_manager.start().await;
  let _ = daemon.start().await;

  Ok(())
}
