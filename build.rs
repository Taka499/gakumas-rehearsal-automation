use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // Embed the Windows manifest that requests administrator privileges.
    //
    // The manifest forces elevation, which also propagates to the `cargo test`
    // harness binary and makes it un-runnable from a non-elevated shell (os
    // error 740). Setting GAKUMAS_NO_MANIFEST=1 skips embedding so the pure
    // unit tests (extract/reconcile/etc.) can run; normal and release builds
    // leave it unset and embed the manifest exactly as before.
    println!("cargo:rerun-if-env-changed=GAKUMAS_NO_MANIFEST");
    if env::var_os("GAKUMAS_NO_MANIFEST").is_none() {
        let _ = embed_resource::compile("gakumas-rehearsal-automation.rc", embed_resource::NONE);
    }

    // Copy resources and config to target directory
    copy_templates();
    copy_guide_images();
    copy_config();
}

/// Copies the template folder to the target directory so the executable can find reference images.
fn copy_templates() {
    let out_dir = env::var("OUT_DIR").unwrap();
    // OUT_DIR is something like target/release/build/gakumas-rehearsal-automation-xxx/out
    // We need to go up to target/release (or target/debug)
    let out_path = Path::new(&out_dir);
    let target_dir = out_path
        .ancestors()
        .nth(3) // Go up 3 levels: out -> hash -> build -> release
        .expect("Could not find target directory");

    let template_src = Path::new("resources/template");
    let template_dst = target_dir.join("resources").join("template");

    if template_src.exists() {
        copy_dir_recursive(template_src, &template_dst);
        // Tell Cargo to re-run if templates change
        println!("cargo:rerun-if-changed=resources/template/");
    }
}

/// Recursively copies a directory and its contents.
fn copy_dir_recursive(src: &Path, dst: &Path) {
    let _ = fs::create_dir_all(dst);

    if let Ok(entries) = fs::read_dir(src) {
        for entry in entries.flatten() {
            let src_path = entry.path();
            let file_name = src_path.file_name().unwrap();
            let dst_path = dst.join(file_name);

            if src_path.is_dir() {
                copy_dir_recursive(&src_path, &dst_path);
            } else {
                let _ = fs::copy(&src_path, &dst_path);
            }
        }
    }
}

/// Copies the guide images folder to the target directory for GUI instructions.
fn copy_guide_images() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir);
    let target_dir = out_path
        .ancestors()
        .nth(3)
        .expect("Could not find target directory");

    let guide_src = Path::new("resources/guide");
    let guide_dst = target_dir.join("resources").join("guide");

    if guide_src.exists() {
        copy_dir_recursive(guide_src, &guide_dst);
        println!("cargo:rerun-if-changed=resources/guide/");
    }
}

/// Copies config.json to the target directory.
fn copy_config() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir);
    let target_dir = out_path
        .ancestors()
        .nth(3)
        .expect("Could not find target directory");

    let config_src = Path::new("config.json");
    let config_dst = target_dir.join("config.json");

    if config_src.exists() {
        let _ = fs::copy(config_src, &config_dst);
        println!("cargo:rerun-if-changed=config.json");
    }
}
