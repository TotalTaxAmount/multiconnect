{
  description = "Multiconnect Flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
  let
    pkgs = import nixpkgs {
      system = system;
      config.allowUnfree = true; # Enable unfree packages here
    };
    # TODO
  in
  { 
    packages = {
      # TODO
    };

    devShell = pkgs.mkShell { # TODO: Clean up this when flake build works
      buildInputs = with pkgs; [
        rustup
        cargo
        nodejs_23
        yarn
        android-studio

        # Tauri
        cargo-tauri
        pkg-config 
        libsoup_3
        webkitgtk_4_1
        openssl
      ];
    };
  });
}
