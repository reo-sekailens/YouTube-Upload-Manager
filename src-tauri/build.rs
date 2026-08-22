fn main() {
    println!("cargo:rerun-if-env-changed=APP_RELEASE_CHANNEL");
    let release_channel = std::env::var("APP_RELEASE_CHANNEL")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| value == "nightly")
        .unwrap_or_else(|| "regular".to_string());
    println!("cargo:rustc-env=APP_RELEASE_CHANNEL={release_channel}");
    tauri_build::build()
}
