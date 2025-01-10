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
    androidEnv = pkgs.androidenv.override { licenseAccepted = true; };
    androidComposition = androidEnv.composeAndroidPackages {
      includeNDK = true;
      includeEmulator = true;
      platformToolsVersion = "35.0.2";
      buildToolsVersions = [ "35.0.0" ];
      platformVersions = [ "35" ];
      systemImageTypes = [ "google_apis_playstore" ];
      includeSystemImages = true;
      cmakeVersions = [ "3.10.2" ];
      extraLicenses = [
        "android-googletv-license"
        "android-sdk-arm-dbt-license"
        "android-sdk-license"
        "android-sdk-preview-license"
        "google-gdk-license"
        "intel-android-extra-license"
        "intel-android-sysimage-license"
        "mips-android-sysimage-license"
      ];
    };

    android-studio-custom = pkgs.android-studio.overrideAttrs (oldAttrs: rec {
      postInstall = ''
        sed -i 's/Name=Android Studio (stable channel)/Name=Android Studio/' \
          "$out/share/applications/android-studio.desktop"
      '';
    });

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
        
      # Android
      android-studio-custom
      androidComposition.androidsdk
      androidComposition.ndk-bundle
      vulkan-headers 
      vulkan-loader
      vulkan-tools
      libsForQt5.qt5.qtwayland
      wayland
      libGL
      jdk11
      lsb-release
    ];
    # TODO
  in
  { 
    packages = {
      # TODO
    };

    devShell = pkgs.mkShell { # TODO: Clean up this when flake build works
      nativeBuildInputs = buildInputs;
      
      LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath buildInputs}";
      NIX_LD = "${pkgs.stdenv.cc.libc}/lib/ld-linux-x86-64.so.2";
      ANDROID_HOME = "${androidComposition.androidsdk}/libexec/android-sdk";
      NDK_HOME = "${androidComposition.androidsdk}/libexec/android-sdk/ndk/${builtins.head (pkgs.lib.lists.reverseList (builtins.split "-" "${androidComposition.ndk-bundle}"))}";
      ANDROID_SDK_ROOT = "${androidComposition.androidsdk}/libexec/android-sdk";
      ANDROID_NDK_ROOT = "${androidComposition.androidsdk}/libexec/android-sdk/ndk-bundle";
      JAVA_HOME = "${pkgs.jdk11.home}";
    };
  });
}
