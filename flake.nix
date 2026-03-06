{
  description = "muni's desktop shell";

  inputs = {
    nixpkgs.url = "github:cachix/devenv-nixpkgs/rolling";

    devenv = {
      url = "github:cachix/devenv";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };

    musicaloft-style = {
      url = "github:musicaloft/musicaloft-style";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    git-hooks-nix = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    devenv-root = {
      url = "file+file:///dev/null";
      flake = false;
    };
  };

  outputs =
    inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      imports = [
        inputs.git-hooks-nix.flakeModule
        inputs.musicaloft-style.flakeModule
      ];

      perSystem =
        {
          config,
          lib,
          pkgs,
          ...
        }:
        let
          # GTK/Rust dependencies
          buildInputs = with pkgs; [
            dbus.dev
            gtk4
            gtk4-layer-shell
            glib
            pango
            gdk-pixbuf
            libadwaita
            libpulseaudio.dev
            udev.dev
            pipewire
            wireplumber
            networkmanager
          ];
          # Additional build inputs for GTK
          nativeBuildInputs = with pkgs; [
            pkg-config
            wrapGAppsHook4
            makeWrapper
          ];
        in
        {
          # rust setup
          devenv.shells.default = {
            env.RUST_LOG = "info,cadenza_shell=debug";

            languages.rust = {
              enable = true;
              channel = "nightly";
              version = "2026-02-14";
              mold.enable = true;
            };

            git-hooks.hooks.clippy = {
              enable = true;
              packageOverrides = {
                cargo = config.devenv.shells.default.languages.rust.toolchainPackage;
                clippy = config.devenv.shells.default.languages.rust.toolchainPackage;
              };
              settings.denyWarnings = lib.mkForce false;
            };

            packages = [
              pkgs.bacon
              pkgs.cargo-outdated
              pkgs.samply # profiling
              pkgs.zbus-xmlgen
            ]
            ++ buildInputs
            ++ nativeBuildInputs;
          };

          packages.default = config.devenv.shells.default.languages.rust.import ./. { };
        };
    };
}
