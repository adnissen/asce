use std::env;
use std::path::PathBuf;

fn main() {
    // Find libmpv using pkg-config
    let mpv = pkg_config::Config::new()
        .probe("mpv")
        .expect("mpv not found. Install with: brew install mpv");

    println!("cargo:rerun-if-changed=wrapper.h");

    // Generate bindings for libmpv
    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        // Add include paths from pkg-config
        .clang_args(
            mpv.include_paths
                .iter()
                .map(|path| format!("-I{}", path.display())),
        )
        // Generate bindings for mpv client and render APIs
        .allowlist_function("mpv_.*")
        .allowlist_type("mpv_.*")
        .allowlist_var("MPV_.*")
        // Don't generate bindings for standard library types
        .blocklist_type("__darwin_.*")
        .blocklist_type("__uint.*")
        .blocklist_type("__int.*")
        // Derive common traits
        .derive_debug(true)
        .derive_default(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    // Write bindings to out directory
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("mpv_bindings.rs"))
        .expect("Couldn't write bindings!");
}
