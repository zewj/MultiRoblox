fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");

    // Build scripts run on the host, so check the *target* rather than cfg!(windows).
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "MultiRoblox");
        res.set(
            "FileDescription",
            "MultiRoblox - run multiple Roblox instances",
        );
        res.set("LegalCopyright", "MIT License");
        res.set("OriginalFilename", "MultiRoblox.exe");
        res.set_manifest(include_str!("assets/app.manifest"));
        if let Err(e) = res.compile() {
            println!("cargo:warning=failed to embed Windows resources: {e}");
        }
    }
}
