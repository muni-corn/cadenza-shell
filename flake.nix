{
  description = "muni's desktop shell";

  inputs = {
    nixpkgs.url = "github:cachix/devenv-nixpkgs/rolling";

    crate2nix = {
      url = "github:nix-community/crate2nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    devenv = {
      url = "github:muni-corn/devenv/rust-toolchain-import-fix";
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

      imports = [ inputs.musicaloft-style.flakeModule ];

      perSystem =
        {
          config,
          lib,
          pkgs,
          ...
        }:
        let
          pname = "cadenza-shell";

          toolchain = config.devenv.shells.default.languages.rust.toolchainPackage;

          # gtk/rust dependencies
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
          libraryPath = lib.makeLibraryPath buildInputs;
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

              # needed for dynamic linking at runtime
              rustflags = "-C link-args=-Wl,-fuse-ld=mold,-rpath,${libraryPath}";
            };

            git-hooks.hooks.clippy = {
              enable = true;
              packageOverrides = {
                cargo = toolchain;
                clippy = toolchain;
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

          packages.default =
            let
              crate2nixTools = pkgs.callPackage "${inputs.crate2nix}/tools.nix" { };
              # Build using the nightly toolchain by passing buildRustCrateForPkgs
              # directly to callPackage on the generated Cargo.nix. Using
              # languages.rust.import (which wraps appliedCargoNix) is insufficient
              # because it always defaults to pkgs.buildRustCrate (stable); the
              # .override it exposes only controls features/crateOverrides, not the
              # underlying toolchain.
              cargoNix =
                pkgs.callPackage
                  (crate2nixTools.generatedCargoNix {
                    name = pname;
                    src = ./.;
                  })
                  {
                    buildRustCrateForPkgs =
                      _pkgs:
                      pkgs.buildRustCrate.override {
                        rustc = toolchain;
                        cargo = toolchain;
                      };
                  };
              args = {
                crateOverrides = pkgs.defaultCrateOverrides // {
                  relm4-icons-build =
                    attrs:
                    let
                      # Extract the icons directory from the source tarball into a
                      # persistent Nix store path. The build script normally writes
                      # CARGO_MANIFEST_DIR/icons (a /build/... temp path) to
                      # shipped_icons.txt, which include_str! then bakes into the
                      # compiled library. By patching the build script to use a store
                      # path instead, the compiled constant remains valid in any sandbox.
                      iconsDir = pkgs.runCommand "relm4-icons-build-icons" { } ''
                        mkdir -p "$out"
                        tar xzf ${attrs.src} --strip-components=1 \
                          --directory="$out" \
                          relm4-icons-build-0.10.1/icons
                      '';
                    in
                    {
                      postPatch = ''
                                              cat > build.rs << 'BUILDSCRIPT'
                        fn main() {
                            let out_dir = std::env::var("OUT_DIR").unwrap();
                            std::fs::write(
                                std::path::Path::new(&out_dir).join("shipped_icons.txt"),
                                "${iconsDir}/icons",
                            )
                            .unwrap();
                        }
                        BUILDSCRIPT
                      '';
                    };
                  gtk4-layer-shell-sys = attrs: {
                    buildInputs = with pkgs; [ gtk4-layer-shell.dev ];
                    nativeBuildInputs = with pkgs; [ pkg-config ];
                  };
                  libadwaita-sys = attrs: {
                    buildInputs = with pkgs; [ libadwaita ];
                    nativeBuildInputs = with pkgs; [ pkg-config ];
                  };
                  libpulse-sys = attrs: {
                    buildInputs = with pkgs; [ libpulseaudio ];
                    nativeBuildInputs = with pkgs; [ pkg-config ];
                  };
                  ${pname} = attrs: {
                    inherit buildInputs nativeBuildInputs;
                    runtimeDependencies = buildInputs;
                  };
                };
              };
            in
            cargoNix.rootCrate.build.override args;
        };
    };
}
