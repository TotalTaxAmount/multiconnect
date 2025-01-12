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
      config.allowUnfree = true;
    };
    };

    buildInputs = with pkgs; [
      rustup
      cargo
      nodejs_23
      yarn

      # Tauri
      cargo-tauri
      pkg-config 
      libsoup_3
      webkitgtk_4_1
      openssl
      librsvg
        
      # Android
      android-studio
    ];
  in
  { 
    packages = {
      # TODO
    };

    devShell = pkgs.mkShell { # TODO: Clean up this when flake build works
      nativeBuildInputs = buildInputs;
      LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath buildInputs}";
      JAVA_HOME = "${pkgs.jdk21.home}";


      WEBKIT_DISABLE_COMPOSITING_MODE = 1; # Fix `Failed to get GBM device`

      GDK_BACKEND = "x11"; # https://github.com/tauri-apps/tauri/issues/12361
    };
  });
}
