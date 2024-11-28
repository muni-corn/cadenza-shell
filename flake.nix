{
  description = "muni's desktop shell";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    ags = {
      url = "github:aylur/ags";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    nixpkgs,
    ags,
    ...
  }: let
    system = "x86_64-linux";
    pkgs = nixpkgs.legacyPackages.${system};

    extraAstalPackages = with ags.packages.${system}; [
      battery
      hyprland
      mpris
      bluetooth
      network
      tray
      wireplumber
    ];
  in {
    packages.${system}.default = ags.lib.bundle {
      inherit pkgs;
      src = ./.;
      name = "muse-shell";
      entry = "app.ts";

      # additional libraries and executables to add to gjs' runtime
      extraPackages = extraAstalPackages;
    };

    devShells.${system}.default = pkgs.mkShell {
      buildInputs = [
        # includes all Astal libraries
        # ags.packages.${system}.agsFull

        # includes astal3 astal4 astal-io by default
        (ags.packages.${system}.default.override {
          extraPackages = extraAstalPackages;
        })
      ];
    };
  };
}
