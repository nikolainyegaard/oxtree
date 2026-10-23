//! On Windows the binary carries a manifest asking for administrator
//! rights, which reading the volume's MFT needs. link.exe merges the file
//! with a default fragment that says asInvoker, so /MANIFESTUAC:NO drops
//! that fragment first. Only the main binary gets it, not the bench.

fn main() {
    println!("cargo:rerun-if-changed=../../packaging/oxtree.manifest");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os == "windows" && env == "msvc" {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packaging/oxtree.manifest");
        println!("cargo:rustc-link-arg-bin=oxtree=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bin=oxtree=/MANIFESTUAC:NO");
        println!(
            "cargo:rustc-link-arg-bin=oxtree=/MANIFESTINPUT:{}",
            manifest.display()
        );
    }
}
