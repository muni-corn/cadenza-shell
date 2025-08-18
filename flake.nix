{
  description = "muni's desktop shell";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    ags = {
      url = "github:aylur/ags";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      ags,
      ...
    }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };
      pname = "cadenza-shell";

      # TypeScript/AGS entry point (legacy)
      entry = "src/app.ts";

      # Rust toolchain
      rustToolchain = pkgs.rust-bin.stable.latest.default.override {
        extensions = [ "rust-src" ];
      };

      astalPackages = with ags.packages.${system}; [
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
      ];

      extraPackages = astalPackages ++ [
        pkgs.libadwaita
        pkgs.libsoup_3
      ];

      # GTK/Rust dependencies
      rustBuildInputs = with pkgs; [
        gtk4
        gtk4-layer-shell
        glib
        pango
        gdk-pixbuf
        pipewire
        wireplumber
        networkmanager
      ];
    in
    {
      # Legacy TypeScript build (for compatibility)
      packages.${system} = {
        typescript = pkgs.stdenv.mkDerivation {
          name = "${pname}-typescript";
          src = ./.;

          nativeBuildInputs = with pkgs; [
            wrapGAppsHook
            gobject-introspection
            ags.packages.${system}.default
          ];

          buildInputs = extraPackages ++ [ pkgs.gjs ];

          installPhase = ''
            runHook preInstall

            mkdir -p $out/bin
            mkdir -p $out/share
            cp -r * $out/share
            ags bundle ${entry} $out/bin/${pname} -d "SRC='$out/share'"

            runHook postInstall
          '';
        };

        # New Rust build
        default = pkgs.rustPlatform.buildRustPackage {
          pname = pname;
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
            rustToolchain
            wrapGAppsHook4
          ];

          buildInputs = rustBuildInputs;
        };
      };

      devShells.${system}.default = pkgs.mkShell {
        buildInputs = [
          # Rust development
          rustToolchain
          pkgs.rust-analyzer
          pkgs.pkg-config

          # GTK dependencies
        ]
        ++ rustBuildInputs
        ++ [
          # Legacy TypeScript development (for transition period)
          (ags.packages.${system}.default.override {
            inherit extraPackages;
          })
          pkgs.typescript
          pkgs.biome
        ];

        # Environment variables for GTK development
        PKG_CONFIG_PATH = "${pkgs.lib.makeSearchPath "lib/pkgconfig" rustBuildInputs}";
      };
    };
}
