{ pkgs ? (import <nixpkgs> {
    config.allowUnfree = true;
}) }:

pkgs.stdenv.mkDerivation {
  name = "multiconnect";

  buildInputs = with pkgs; [
    rustup
    cargo
    cargo-tauri
    nodejs_22
    pkg-config
    libsoup
    webkitgtk
  ];
}