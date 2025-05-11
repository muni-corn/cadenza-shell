{
  description = "muni's desktop shell";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    astal = {
      url = "github:Aylur/astal";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    ags = {
      url = "github:aylur/ags";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      ags,
      ...
    }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};

      extraAstalPackages = with ags.packages.${system}; [
        battery
        bluetooth
        hyprland
        mpris
        network
        notifd
        tray
        wireplumber
      ];
    in
    {
      packages.${system}.default = ags.lib.bundle {
        inherit pkgs;
        src = ./.;
        name = "muse-shell";
        entry = "app.ts";
        gtk4 = true;

        # additional libraries and executables to add to gjs' runtime
        extraPackages = extraAstalPackages;
      };

      devShells.${system}.default = pkgs.mkShell {
        buildInputs = [
          (ags.packages.${system}.agsFull)
        ];
      };
    };
}
