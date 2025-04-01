use fern::colors::{Color, ColoredLevelConfig};
use multiconnect_daemon::{
  modules::{ModuleManager, ModuleTest},
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
  let mut module_manager = ModuleManager::new();
  module_manager.register(ModuleTest::new());

  NetworkManager::start(daemon.clone(), module_manager).await?;

  let _ = daemon.start().await;

  Ok(())
}
