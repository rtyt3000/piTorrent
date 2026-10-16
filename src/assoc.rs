//! Registers the app as a handler for .torrent files and magnet links.

#[cfg(windows)]
pub fn register() -> Result<(), String> {
    use winreg::{RegKey, enums::HKEY_CURRENT_USER};

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.to_string_lossy();
    let open_cmd = format!("\"{exe}\" \"%1\"");
    let io = |e: std::io::Error| e.to_string();

    let classes = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey("Software\\Classes")
        .map_err(io)?
        .0;
    let set = |path: &str, name: &str, value: &str| -> Result<(), String> {
        classes.create_subkey(path).map_err(io)?.0.set_value(name, &value).map_err(io)
    };

    set("piTorrent.torrent", "", "BitTorrent file")?;
    set("piTorrent.torrent\\DefaultIcon", "", &format!("\"{exe}\",0"))?;
    set("piTorrent.torrent\\shell\\open\\command", "", &open_cmd)?;
    set(".torrent", "", "piTorrent.torrent")?;
    set(".torrent\\OpenWithProgids", "piTorrent.torrent", "")?;

    set("magnet", "", "URL:Magnet Protocol")?;
    set("magnet", "URL Protocol", "")?;
    set("magnet\\shell\\open\\command", "", &open_cmd)?;

    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", "ms-settings:defaultapps"])
        .spawn();
    Ok(())
}

#[cfg(not(windows))]
pub fn register() -> Result<(), String> {
    Err("not supported on this platform yet".into())
}

/// The first command-line argument that is a magnet link or a .torrent path.
pub fn open_arg() -> Option<String> {
    std::env::args().skip(1).find(|a| {
        let a = a.to_lowercase();
        a.starts_with("magnet:") || a.ends_with(".torrent")
    })
}
