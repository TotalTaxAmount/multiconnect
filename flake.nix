{
  description = "Multiconnect Flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          system = system;
          config.allowUnfree = true;
        };

        buildInputs = with pkgs; [
          rustup
          rust-analyzer
          cargo
          nodejs_23
          yarn
          protobuf_27
          #      openssl_3
          lsb-release
          nixfmt-rfc-style

          # Tokio
          tokio-console

          # Tauri
          cargo-tauri
          pkg-config
          libsoup_3
          webkitgtk_4_1
          gtk3
          gtk3-x11
          adwaita-icon-theme
          #     openssl
          librsvg
          gsettings-desktop-schemas
          glib

          # Android
          android-studio
        ];
      in
      {
        packages = {
          # TODO
        };

        devShell = pkgs.mkShell {
          # TODO: Clean up this when flake build works
          nativeBuildInputs = buildInputs;
          LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath buildInputs}";
          JAVA_HOME = "${pkgs.jdk21.home}";

          GSETTINGS_SCHEMA_DIR = "${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}/glib-2.0/schemas";

          RUSTFLAGS = "--cfg tokio_unstable"; # TODO: Don't but this here
          WEBKIT_DISABLE_COMPOSITING_MODE = 1; # Fix `Failed to get GBM device`

          GDK_BACKEND = "x11"; # https://github.com/tauri-apps/tauri/issues/12361

        };
      }
    );
}
