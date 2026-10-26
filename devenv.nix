{
  config,
  pkgs,
  ...
}:
let
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
  # additional build inputs for GTK
  nativeBuildInputs = with pkgs; [
    pkg-config
    wrapGAppsHook4
    makeWrapper
  ];
in
{
  # needed for dynamic linking at runtime
  env.RUST_LOG = "warn,cadenza_shell=debug";

  languages.rust = {
    enable = true;
    channel = "nightly";
    wild.enable = true;
  };

  packages = buildInputs ++ nativeBuildInputs;

  outputs.default = config.languages.rust.crane.import ./. {
    inherit buildInputs nativeBuildInputs;
    runtimeDependencies = buildInputs;

    # style.scss is read with include_bytes!, so it has to survive the source
    # filter
    extraFileTypes = [ "scss" ];
  };
}
