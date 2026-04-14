use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rustc-check-cfg=cfg(ghostty_embed_available)");
    println!("cargo:rerun-if-env-changed=GHOSTTY_SHELL_GHOSTTY_DIR");
    println!("cargo:rerun-if-env-changed=GHOSTTY_SHELL_GHOSTTY_KIT");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("missing manifest dir"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("app-shell should live under crates/");

    let ghostty_root = env::var_os("GHOSTTY_SHELL_GHOSTTY_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.join("target/ghostty-upstream/ghostty"));
    let ghostty_kit_dir = env::var_os("GHOSTTY_SHELL_GHOSTTY_KIT")
        .map(PathBuf::from)
        .unwrap_or_else(|| ghostty_root.join("macos/GhosttyKit.xcframework/macos-arm64"));

    let header_path = ghostty_kit_dir.join("Headers/ghostty.h");
    let library_path = ghostty_kit_dir.join("libghostty-fat.a");

    println!("cargo:rerun-if-changed={}", header_path.display());
    println!("cargo:rerun-if-changed={}", library_path.display());

    if !header_path.exists() || !library_path.exists() {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("missing OUT_DIR"));
    let bindings_path = out_dir.join("ghostty_bindings.rs");

    let bindings = bindgen::Builder::default()
        .header(header_path.display().to_string())
        .clang_arg("-DGHOSTTY_STATIC=1")
        .allowlist_function("ghostty_.*")
        .allowlist_type("ghostty_.*")
        .allowlist_var("GHOSTTY_.*")
        .generate_comments(false)
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("failed to generate ghostty bindings");

    bindings
        .write_to_file(&bindings_path)
        .expect("failed to write ghostty bindings");

    println!("cargo:rustc-cfg=ghostty_embed_available");
    println!(
        "cargo:rustc-link-search=native={}",
        ghostty_kit_dir.display()
    );
    println!("cargo:rustc-link-lib=static=ghostty-fat");
    println!("cargo:rustc-link-lib=dylib=c++");
    println!("cargo:rustc-link-arg=-mmacosx-version-min=13.0");

    for framework in [
        "AppKit",
        "Carbon",
        "CoreFoundation",
        "CoreGraphics",
        "CoreText",
        "CoreVideo",
        "Foundation",
        "IOSurface",
        "Metal",
        "QuartzCore",
        "Security",
    ] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
}
