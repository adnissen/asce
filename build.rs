use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=wrapper.h");

    // Platform-specific setup
    let (include_paths, lib_path) = if cfg!(target_os = "windows") {
        setup_windows()
    } else {
        setup_unix()
    };

    // Tell cargo where to find the library
    if let Some(lib_path) = lib_path {
        println!("cargo:rustc-link-search=native={}", lib_path.display());
    }
    println!("cargo:rustc-link-lib=mpv");

    // Generate bindings for libmpv
    let mut builder = bindgen::Builder::default().header("wrapper.h");

    // Add include paths
    for path in include_paths {
        builder = builder.clang_arg(format!("-I{}", path.display()));
    }

    let bindings = builder
        // Generate bindings for mpv client and render APIs
        .allowlist_function("mpv_.*")
        .allowlist_type("mpv_.*")
        .allowlist_var("MPV_.*")
        // Don't generate bindings for platform-specific types
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

fn setup_windows() -> (Vec<PathBuf>, Option<PathBuf>) {
    // Check environment variable first, then common locations
    let mpv_root = match env::var("MPV_DIR") {
        Ok(path) => PathBuf::from(path),
        Err(_) => {
            let candidates = vec![PathBuf::from("C:\\mpv-dev"), PathBuf::from("C:\\mpv")];
            candidates.into_iter()
                .find(|p| p.exists())
                .expect("libmpv not found. Please:\n\
                         1. Download mpv-dev-x86_64-*.7z from https://sourceforge.net/projects/mpv-player-windows/files/libmpv/\n\
                         2. Extract to C:\\mpv-dev or C:\\mpv\n\
                         3. Or set MPV_DIR environment variable to the extracted location")
        }
    };

    let include_path = mpv_root.join("include");
    let lib_path = mpv_root.clone();

    if !include_path.exists() {
        panic!("MPV include directory not found at {}. Make sure you downloaded the development build (mpv-dev-*.7z)",
               include_path.display());
    }

    (vec![include_path], Some(lib_path))
}

fn setup_unix() -> (Vec<PathBuf>, Option<PathBuf>) {
    // Find libmpv using pkg-config
    let mpv = pkg_config::Config::new()
        .probe("mpv")
        .expect("mpv not found. Install with: brew install mpv (macOS) or apt-get install libmpv-dev (Linux)");

    (mpv.include_paths, None)
}
