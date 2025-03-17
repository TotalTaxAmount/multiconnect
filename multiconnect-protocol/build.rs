use std::{fs, path::Path};

const PROTO_DIR: &str = "proto";

fn main() {
  let proto_files = collect_proto_files(PROTO_DIR);

  for proto in &proto_files {
    println!("cargo:rerun-if-changed={}", proto);
  }

  let includes = &[PROTO_DIR];

  prost_build::Config::new()
    .type_attribute(".", "#[derive(serde::Serialize,serde::Deserialize)]")
    .include_file("proto.rs")
    .compile_protos(&proto_files, includes)
    .expect("Failed to compile protos");
}

fn collect_proto_files<P: AsRef<Path> + ?Sized>(dir: &P) -> Vec<String> {
  let mut proto_files = Vec::new();
  if let Ok(entries) = fs::read_dir(dir) {
    for entry in entries.filter_map(Result::ok) {
      let path = entry.path();
      if path.is_dir() {
        proto_files.extend(collect_proto_files(&path));
      } else if path.extension().and_then(|s| s.to_str()) == Some("proto") {
        proto_files.push(path.to_string_lossy().into_owned());
      }
    }
  }
  proto_files
}
