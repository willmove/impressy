use std::path::PathBuf;

fn main() {
    let icon = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/icons/rastery.ico");
    println!("cargo:rerun-if-changed={}", icon.display());

    assert!(
        icon.is_file(),
        "Rastery Windows icon is missing; regenerate the application icons"
    );

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let icon = icon
        .to_str()
        .expect("Rastery Windows icon path must be valid UTF-8");
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon(icon)
        .set("ProductName", "Rastery")
        .set("FileDescription", "Rastery local image processing toolbox")
        .set("OriginalFilename", "rastery.exe");
    resource
        .compile()
        .expect("failed to embed the Rastery Windows icon");
}
