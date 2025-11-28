{
  description = "muni's desktop shell";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

    devenv = {
      url = "github:cachix/devenv";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };

    musicaloft-style = {
      url = "git+https://git.musicaloft.com/municorn/musicaloft-style";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    git-hooks-nix = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    rust-flake = {
      url = "github:juspay/rust-flake";
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
        inputs.devenv.flakeModule
        inputs.rust-flake.flakeModules.default
        inputs.rust-flake.flakeModules.nixpkgs
        inputs.musicaloft-style.flakeModule
      ];

      perSystem =
        {
          config,
          pkgs,
          ...
        }:
        let
          pname = "cadenza-shell";

          # GTK/Rust dependencies
          buildInputs = with pkgs; [
            dbus.dev
            gtk4
            gtk4-layer-shell
            glib
            pango
            gdk-pixbuf
            libpulseaudio.dev
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
              mold.enable = true;
            };

            packages = [
              pkgs.bacon
              pkgs.cargo-outdated
            ]
            ++ buildInputs
            ++ nativeBuildInputs;
          };

          # setup rust packages
          rust-project = {
            # ensure scss files are included with build
            src = pkgs.lib.cleanSourceWith {
              src = inputs.self;
              filter =
                path: type:
                (pkgs.lib.hasSuffix ".scss" path) || (config.rust-project.crane-lib.filterCargoSources path type);
            };

            # use the same rust toolchain from the dev shell for consistency
            toolchain = config.devenv.shells.default.languages.rust.toolchainPackage;

            # specify dependencies
            defaults.perCrate.crane.args = {
              inherit nativeBuildInputs buildInputs;
            };
          };

          packages.default = config.rust-project.crates.${pname}.crane.outputs.packages.${pname};
        };
    };
}
