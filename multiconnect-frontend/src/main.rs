// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{error::Error, str::FromStr, time::SystemTime};

use fern::colors::{Color, ColoredLevelConfig};
use multiconnect_lib::FrontendArgs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let args: FrontendArgs = argh::from_env();

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

  Ok(multiconnect_lib::run(args.port))
}
