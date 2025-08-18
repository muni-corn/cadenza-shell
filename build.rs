use glib_build_tools::compile_resources;
use std::path::Path;

fn main() {
    // Rerun if build script changes
    println!("cargo:rerun-if-changed=build.rs");

    // Rerun if any source files change (for development)
    println!("cargo:rerun-if-changed=src/");

    // Check for CSS and resource files
    println!("cargo:rerun-if-changed=src/style.scss");

    // Compile GLib resources if resource files exist
    let resources_dir = Path::new("resources");
    if resources_dir.exists() {
        println!("cargo:rerun-if-changed=resources/");

        // Look for gresource.xml file
        let gresource_file = resources_dir.join("muse-shell.gresource.xml");
        if gresource_file.exists() {
            compile_resources(
                &[resources_dir],
                "resources/muse-shell.gresource.xml",
                "muse-shell.gresource",
            );
        }
    }

    // Set environment variables for build metadata
    println!(
        "cargo:rustc-env=MUSE_SHELL_VERSION={}",
        env!("CARGO_PKG_VERSION")
    );
    println!(
        "cargo:rustc-env=MUSE_SHELL_TARGET={}",
        std::env::var("TARGET").unwrap_or_default()
    );
}
