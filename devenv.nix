{
  config,
  lib,
  pkgs,
  ...
}:
let
  pname = "cadenza-shell";

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

  libraryPath = lib.makeLibraryPath buildInputs;
in
{
  # needed for dynamic linking at runtime
  env = {
    RUSTFLAGS = lib.mkForce "-C link-args=-Wl,-fuse-ld=mold,-rpath,${libraryPath}";
    RUST_LOG = "warn,cadenza_shell=debug";
  };

  languages.rust = {
    enable = true;
    channel = "nightly";
    mold.enable = true;
  };

  packages =
    with pkgs;
    [
      bacon
      cargo-outdated
      cargo-tarpaulin

      secretspec
    ]
    ++ buildInputs
    ++ nativeBuildInputs;

  scripts.tarp.exec = ''cargo tarpaulin --engine llvm "$@"'';

  outputs.default =
    let
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
    config.languages.rust.import ./. args;
}
