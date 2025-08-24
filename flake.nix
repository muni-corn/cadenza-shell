{
  description = "muni's desktop shell";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };

    git-hooks-nix = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    rust-flake = {
      url = "github:juspay/rust-flake";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
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
        inputs.git-hooks-nix.flakeModule
        inputs.rust-flake.flakeModules.default
        inputs.rust-flake.flakeModules.nixpkgs
        inputs.treefmt-nix.flakeModule
      ];

      perSystem =
        {
          self',
          config,
          pkgs,
          system,
          ...
        }:
        let
          
      pname = "cadenza-shell";
        in
        {
          # git hooks
          pre-commit.settings.hooks = {
            # commit linting
            commitlint-rs =
              let
                config = pkgs.writers.writeYAML "commitlintrc.yml" {
                  rules = {
                    description-empty.level = "error";
                    description-format = {
                      level = "error";
                      format = "^[a-z].*$";
                    };
                    description-max-length = {
                      level = "error";
                      length = 72;
                    };
                    scope-max-length = {
                      level = "warning";
                      length = 10;
                    };
                    scope-empty.level = "warning";
                    type = {
                      level = "error";
                      options = [
                        "build"
                        "chore"
                        "ci"
                        "docs"
                        "feat"
                        "fix"
                        "perf"
                        "refactor"
                        "test"
                      ];
                    };
                  };
                };

              in
              {
                enable = true;
                name = "commitlint-rs";
                package = pkgs.commitlint-rs;
                description = "Validate commit messages with commitlint-rs";
                entry = "${pkgs.lib.getExe pkgs.commitlint-rs} -g ${config} -e";
                always_run = true;
                stages = [ "commit-msg" ];
              };

            # format on commit
            treefmt.enable = true;
          };

          # formatting
          treefmt.programs = {
            biome = {
              enable = true;
              settings = pkgs.lib.importJSON ./biome.json;
            };
            nixfmt.enable = true;
            rustfmt.enable = true;
            taplo.enable = true;
          };

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
            inputsFrom = [
              self'.devShells.rust
              config.pre-commit.devShell
            ];
            packages = [ pkgs.cargo-outdated ];
            RUST_LOG = "info,muse_shell=debug";
          };
        };
    };
}
