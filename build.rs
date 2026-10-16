// Embeds the icon and version info into the Windows executable.
fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/app.ico");
        res.set("ProductName", "piTorrent");
        res.set("FileDescription", "piTorrent BitTorrent client");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the executable icon: {e}");
        }
    }
    println!("cargo:rerun-if-changed=assets/app.ico");
    println!("cargo:rerun-if-changed=build.rs");
}
