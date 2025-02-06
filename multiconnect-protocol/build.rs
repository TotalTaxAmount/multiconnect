use std::fs;
use std::path::Path;

fn main() {
  let proto_dir = "proto";

  let proto_files: Vec<String> = fs::read_dir(proto_dir)
    .expect("Failed to read proto directory")
    .filter_map(|entry| {
      let entry = entry.ok()?;
      let path = entry.path();
      if path.extension()?.to_str()? == "proto" {
        Some(path.to_string_lossy().into_owned())
      } else {
        None
      }
    })
    .collect();

  for proto in &proto_files {
    println!("cargo:rerun-if-changed={}", proto);
  }

  let includes = &[proto_dir];

  prost_build::Config::new()
    .include_file("proto.rs")
    .compile_protos(&proto_files, includes)
    .expect("Failed to compile protos");
}
