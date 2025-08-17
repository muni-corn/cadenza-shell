{
  description = "muni's desktop shell";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };

    rust-flake = {
      url = "github:juspay/rust-flake";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    ags = {
      url = "github:aylur/ags";
      inputs.nixpkgs.follows = "nixpkgs";
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
        inputs.rust-flake.flakeModules.default
        inputs.rust-flake.flakeModules.nixpkgs
      ];

      perSystem =
        {
          self',
          pkgs,
          system,
          ...
        }:
        let
          
      pname = "cadenza-shell";
        in
        {
          # Configure rust-flake
          rust-project = {
            # Use fenix for Rust toolchain
            toolchain = inputs.fenix.packages.${system}.stable.toolchain;

            crates.${pname}.crane.args = {
              # GTK/Rust dependencies
              buildInputs = with pkgs; [
                gtk4
                gtk4-layer-shell
                glib
                pango
                gdk-pixbuf
                pipewire
                wireplumber
                networkmanager
              ];

              # Additional build inputs for GTK
              nativeBuildInputs = with pkgs; [
                pkg-config
                wrapGAppsHook4
              ];
            };
          };

          # Legacy TypeScript build (for compatibility)
          packages = {
            typescript = pkgs.stdenv.mkDerivation {
              name = "${pname}-typescript";
              src = ./.;

              nativeBuildInputs = with pkgs; [
                wrapGAppsHook
                gobject-introspection
                inputs.ags.packages.${system}.default
              ];

              buildInputs = (
                with inputs.ags.packages.${system};
                [
                  astal4
                  battery
                  bluetooth
                  hyprland
                  io
                  mpris
                  network
                  notifd
                  tray
                  wireplumber

                  pkgs.libadwaita
                  pkgs.libsoup_3
                  pkgs.gjs
                ]
              );

              installPhase = ''
                runHook preInstall

                mkdir -p $out/bin
                mkdir -p $out/share
                cp -r * $out/share
                ags bundle src/app.ts $out/bin/${pname} -d "SRC='$out/share'"

                runHook postInstall
              '';
            };

            # Set the Rust build as default
            default = self'.packages.muse-shell;
          };

          # Development shell
          devShells.default = pkgs.mkShell {
            # for some reason, using the dev shell directly doesn't work, but this does
            inputsFrom = [ self'.devShells.rust ];
          };
        };
    };
}
