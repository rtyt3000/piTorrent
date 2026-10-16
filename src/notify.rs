//! Desktop notifications.

const APP_ID: &str = "piTorrent";

/// Windows shows toasts only for registered app ids; registering ours gives them our name.
pub fn init() {
    #[cfg(windows)]
    {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(format!("Software\\Classes\\AppUserModelId\\{APP_ID}"));
        if let Ok((key, _)) = key {
            let _ = key.set_value("DisplayName", &APP_ID);
        }
    }
}

pub fn send(title: String, body: String) {
    std::thread::spawn(move || {
        let mut n = notify_rust::Notification::new();
        n.summary(&title).body(&body);
        #[cfg(windows)]
        n.app_id(APP_ID);
        #[cfg(not(windows))]
        n.appname(APP_ID);
        let _ = n.show();
    });
}
