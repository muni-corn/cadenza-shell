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

  outputs = inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      
      imports = [
        inputs.rust-flake.flakeModules.default
        inputs.rust-flake.flakeModules.nixpkgs
      ];

      perSystem = { config, self', inputs', pkgs, system, ... }:
        let
          
      pname = "cadenza-shell";
          
          # TypeScript/AGS entry point (legacy)
          entry = "src/app.ts";

          astalPackages = with inputs.ags.packages.${system}; [
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
          # Configure rust-flake
          rust-project = {
            crate2nixDerivation = {
              pname = pname;
              version = "0.1.0";
              src = ./.;
              
              # Additional build inputs for GTK
              buildInputs = rustBuildInputs;
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
          };

          # Development shell
          devShells.default = pkgs.mkShell {
            inputsFrom = [
              config.rust-project.devShell
            ];
            
            buildInputs = rustBuildInputs ++ [
              # Legacy TypeScript development (for transition period)
              (inputs.ags.packages.${system}.default.override {
                inherit extraPackages;
              })
              pkgs.typescript
              pkgs.biome
            ];

            # Environment variables for GTK development
            PKG_CONFIG_PATH = "${pkgs.lib.makeSearchPath "lib/pkgconfig" rustBuildInputs}";
          };
        };
    };
}
