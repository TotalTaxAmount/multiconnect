// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[tokio::main]
async fn main() {
  if std::env::var("LOGLEVEL").is_err() {
    std::env::set_var("LOGLEVEL", "info");
  }

  pretty_env_logger::formatted_timed_builder().parse_env("LOGLEVEL").format_timestamp_secs().init();

  multiconnect_lib::run().await;
}
