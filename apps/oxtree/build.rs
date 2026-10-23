//! On Windows the binary carries a manifest asking for administrator
//! rights, which reading the volume's MFT needs; link.exe merges it with
//! its default one.

fn main() {
    println!("cargo:rerun-if-changed=../../packaging/oxtree.manifest");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os == "windows" && env == "msvc" {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packaging/oxtree.manifest");
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
            manifest.display()
        );
    }
}
