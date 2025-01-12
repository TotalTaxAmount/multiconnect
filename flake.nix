{
  description = "Multiconnect Flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
  let
    buildToolsVersion = "34.0.0";

    pkgs = import nixpkgs {
      system = system;
      config.allowUnfree = true;
      config.android_sdk.accept_license = true;
    };
    androidEnv = pkgs.androidenv.override { licenseAccepted = true; };
    androidComposition = androidEnv.composeAndroidPackages {
      includeNDK = true;
      includeEmulator = true;
      platformToolsVersion = "35.0.2";
      buildToolsVersions = [ buildToolsVersion ];
      platformVersions = [ "34" ];
      systemImageTypes = [ "google_apis_playstore" ];
          abiVersions = [ "armeabi-v7a" "arm64-v8a" "x86_64" ];
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
      # androidComposition.androidsdk
      # androidComposition.ndk-bundle
      # vulkan-headers 
      # vulkan-loader
      # vulkan-tools
      # libsForQt5.qt5.qtwayland
      # wayland
      # libGL
      # jdk21
      # lsb-release
    ];
  in
  { 
    packages = {
      # TODO
    };

    devShell = pkgs.mkShell { # TODO: Clean up this when flake build works
      nativeBuildInputs = buildInputs;
      
      LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath buildInputs}";
      ANDROID_HOME = "${androidComposition.androidsdk}/libexec/android-sdk";
      NDK_HOME = "${androidComposition.androidsdk}/libexec/android-sdk/ndk/${builtins.head (pkgs.lib.lists.reverseList (builtins.split "-" "${androidComposition.ndk-bundle}"))}";
      ANDROID_SDK_ROOT = "${androidComposition.androidsdk}/libexec/android-sdk";
      ANDROID_NDK_ROOT = "${androidComposition.androidsdk}/libexec/android-sdk/ndk-bundle";
      JAVA_HOME = "${pkgs.jdk21.home}";

      GRADLE_OPTS = "-Dorg.gradle.project.android.aapt2FromMavenOverride=${androidComposition.androidsdk}/libexec/android-sdk/build-tools/${buildToolsVersion}/aapt2"; # Fix `APT2 aapt2-8.5.1-11315950-linux Daemon #0: Daemon startup failed`

      WEBKIT_DISABLE_COMPOSITING_MODE = 1; # Fix `Failed to get GBM device`

      GDK_BACKEND = "x11"; # https://github.com/tauri-apps/tauri/issues/12361
    };
  });
}
