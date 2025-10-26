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
        inputs.treefmt-nix.flakeModule
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
              config.treefmt.build.wrapper
              pkgs.bacon
              pkgs.cargo-outdated
            ]
            ++ buildInputs
            ++ nativeBuildInputs
            ++ (builtins.attrValues config.treefmt.build.programs);

            # git hooks
            git-hooks.hooks = {
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
                          "style"
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
              treefmt = {
                enable = true;
                packageOverrides.treefmt = config.treefmt.build.wrapper;
              };
            };
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

          # formatting
          treefmt.programs = {
            dprint = {
              enable = true;
              includes = [
                "*.scss"
              ];
              settings = {
                indentWidth = 2;
                useTabs = false;

                plugins = (
                  pkgs.dprint-plugins.getPluginList (
                    plugins: with plugins; [
                      g-plane-malva
                    ]
                  )
                );

                malva = {
                  hexColorLength = "short";
                  quotes = "preferSingle";
                  formatComments = true;
                  declarationOrder = "smacss";
                  keyframeSelectorNotation = "keyword";
                  preferSingleLine = true;
                };
              };
            };
            mdformat = {
              enable = true;
              package = pkgs.mdformat.withPlugins (
                p: with p; [
                  mdformat-frontmatter
                  mdformat-tables
                ]
              );
              settings.wrap = 80;
            };
            nixfmt.enable = true;
            rustfmt.enable = true;
            taplo.enable = true;
          };

          packages.default = config.rust-project.crates.${pname}.crane.outputs.packages.${pname};
        };
    };
}
