use auto_launch::{AutoLaunch, AutoLaunchBuilder};

pub const MINIMIZED_ARG: &str = "--minimized";

const APP_NAME: &str = "piTorrent";
const LEGACY_APP_NAME: &str = "Tidepool";

fn launcher(name: &str) -> Result<AutoLaunch, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    AutoLaunchBuilder::new()
        .set_app_name(name)
        .set_app_path(&exe.to_string_lossy())
        .set_args(&[MINIMIZED_ARG])
        .build()
        .map_err(|e| e.to_string())
}

pub fn set(enabled: bool) -> Result<(), String> {
    // Drop the entry registered before the rename so it does not start a second copy.
    if let Ok(old) = launcher(LEGACY_APP_NAME) {
        let _ = old.disable();
    }
    let l = launcher(APP_NAME)?;
    if enabled { l.enable() } else { l.disable() }.map_err(|e| e.to_string())
}
