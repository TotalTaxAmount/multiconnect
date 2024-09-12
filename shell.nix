{ pkgs ? (import <nixpkgs> {
    config.allowUnfree = true;
}) }:

pkgs.stdenv.mkDerivation {
  name = "multiconnect";

  buildInputs = with pkgs; [
    rustup
    cargo
  ];
}