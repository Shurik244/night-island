fn main() {
    println!("cargo:rerun-if-changed=native/capture_helper.rs");
    let helper = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("night_island_capture.dll");
    let status = std::process::Command::new(std::env::var_os("RUSTC").unwrap())
        .args([
            "--crate-name",
            "night_island_capture",
            "--crate-type",
            "cdylib",
            "--edition=2024",
            "-C",
            "opt-level=z",
            "-C",
            "panic=abort",
            "--target",
        ])
        .arg(std::env::var("TARGET").unwrap())
        .arg("native/capture_helper.rs")
        .arg("-o")
        .arg(&helper)
        .status()
        .unwrap();
    assert!(status.success(), "Could not build capture integration");
    println!("cargo:rerun-if-changed=assets/icon/app.rc");
    println!("cargo:rerun-if-changed=assets/icon/night-island.ico");
    println!("cargo:rerun-if-env-changed=NI_BACKEND_HOST");
    let host = std::env::var("NI_BACKEND_HOST").unwrap_or_else(|_| "backend.invalid".into());
    println!("cargo:rustc-env=NI_BACKEND_HOST={host}");
    embed_resource::compile("assets/icon/app.rc", embed_resource::NONE).manifest_required().unwrap();
}
