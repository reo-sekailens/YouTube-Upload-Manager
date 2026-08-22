fn main() {
    println!("cargo:rerun-if-env-changed=APP_RELEASE_CHANNEL");
    let release_channel = std::env::var("APP_RELEASE_CHANNEL")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| {
            value == "nightly"
                || value.strip_prefix("nightly-").is_some_and(|suffix| {
                    !suffix.is_empty()
                        && suffix
                            .chars()
                            .all(|character| character.is_ascii_lowercase())
                })
                || value
                    .strip_prefix('v')
                    .and_then(|value| value.split_once("-nightly."))
                    .is_some_and(|(version, suffix)| {
                        !version.is_empty()
                            && version
                                .chars()
                                .all(|character| character.is_ascii_digit() || character == '.')
                            && !suffix.is_empty()
                            && suffix
                                .chars()
                                .all(|character| character.is_ascii_lowercase())
                    })
        })
        .unwrap_or_else(|| "regular".to_string());
    println!("cargo:rustc-env=APP_RELEASE_CHANNEL={release_channel}");
    tauri_build::build()
}
