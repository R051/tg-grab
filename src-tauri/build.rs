fn main() {
    let desktop = std::env::var("CARGO_FEATURE_DESKTOP").is_ok();
    if desktop {
        tauri_build::build();
    } else {
        println!("cargo:rerun-if-changed=build.rs");
    }
}
