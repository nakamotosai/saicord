fn main() {
    println!("cargo:rerun-if-changed=assets/saicord.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/saicord.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icon embed failed: {e}");
        }
    }
}
