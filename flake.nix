{
  description = "muni's desktop shell";

  inputs = {
    nixpkgs.url = "github:cachix/devenv-nixpkgs/rolling";

    crate2nix = {
      url = "github:nix-community/crate2nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    devenv = {
      url = "github:cachix/devenv";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };

    musicaloft-shell = {
      url = "github:musicaloft/musicaloft-shell/devenv";
      flake = false;
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

      imports = [ inputs.devenv.flakeModule ];

      perSystem =
        { config, ... }:
        {
          devenv.shells.default.imports = [
            "${inputs.musicaloft-shell}/devenv.nix"
            ./devenv.nix
          ];

          # package build
          packages = config.devenv.shells.default.outputs;
        };
    };
}
