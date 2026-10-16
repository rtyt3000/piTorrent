//! Registers the app as a handler for .torrent files and magnet links.

#[cfg(windows)]
pub fn register() -> Result<(), String> {
    use winreg::{RegKey, enums::HKEY_CURRENT_USER};

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.to_string_lossy();
    let open_cmd = format!("\"{exe}\" \"%1\"");
    let io = |e: std::io::Error| e.to_string();

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let set_at = |path: &str, name: &str, value: &str| -> Result<(), String> {
        hkcu.create_subkey(path).map_err(io)?.0.set_value(name, &value).map_err(io)
    };
    let set = |path: &str, name: &str, value: &str| set_at(&format!("Software\\Classes\\{path}"), name, value);

    set("piTorrent.torrent", "", "BitTorrent file")?;
    set("piTorrent.torrent\\DefaultIcon", "", &format!("\"{exe}\",0"))?;
    set("piTorrent.torrent\\shell\\open\\command", "", &open_cmd)?;
    set(".torrent", "", "piTorrent.torrent")?;
    set(".torrent\\OpenWithProgids", "piTorrent.torrent", "")?;

    set("piTorrent.magnet", "", "URL:Magnet Protocol")?;
    set("piTorrent.magnet", "URL Protocol", "")?;
    set("piTorrent.magnet\\DefaultIcon", "", &format!("\"{exe}\",0"))?;
    set("piTorrent.magnet\\shell\\open\\command", "", &open_cmd)?;
    set("magnet", "", "URL:Magnet Protocol")?;
    set("magnet", "URL Protocol", "")?;
    set("magnet\\shell\\open\\command", "", &open_cmd)?;

    // Makes the app show up in Settings / Default apps, where the user has to confirm the choice:
    // Windows does not let programs change the default handler themselves.
    let caps = "Software\\piTorrent\\Capabilities";
    set_at(caps, "ApplicationName", "piTorrent")?;
    set_at(caps, "ApplicationDescription", "BitTorrent client")?;
    set_at(&format!("{caps}\\FileAssociations"), ".torrent", "piTorrent.torrent")?;
    set_at(&format!("{caps}\\URLAssociations"), "magnet", "piTorrent.magnet")?;
    set_at("Software\\RegisteredApplications", "piTorrent", caps)?;

    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", "ms-settings:defaultapps"])
        .spawn();
    Ok(())
}

/// Writes a desktop entry and makes it the default handler through `xdg-mime`.
#[cfg(target_os = "linux")]
pub fn register() -> Result<(), String> {
    use std::process::Command;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
    let dir = dirs::data_dir().ok_or("no data directory")?.join("applications");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=piTorrent\nComment=BitTorrent client\n\
         Exec=\"{exe}\" %U\nTerminal=false\nCategories=Network;FileTransfer;P2P;\n\
         MimeType=application/x-bittorrent;x-scheme-handler/magnet;\n"
    );
    std::fs::write(dir.join("piTorrent.desktop"), entry).map_err(|e| e.to_string())?;

    let _ = Command::new("update-desktop-database").arg(&dir).status();
    let status = Command::new("xdg-mime")
        .args(["default", "piTorrent.desktop", "application/x-bittorrent", "x-scheme-handler/magnet"])
        .status()
        .map_err(|e| format!("xdg-mime: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("xdg-mime failed: {status}")) }
}

#[cfg(not(any(windows, target_os = "linux")))]
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
