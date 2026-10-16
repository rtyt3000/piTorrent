use crate::app_icon;
use crate::i18n::tr;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub enum TrayAction {
    Show,
    Quit,
}

const SHOW_ID: &str = "show";
const QUIT_ID: &str = "quit";

pub struct Tray {
    #[cfg(not(target_os = "linux"))]
    _icon: TrayIcon,
    #[cfg(not(target_os = "linux"))]
    show: MenuItem,
    #[cfg(not(target_os = "linux"))]
    quit: MenuItem,
    /// GTK objects are not `Send`, so on Linux they live on their own thread and get commands.
    #[cfg(target_os = "linux")]
    relabel: std::sync::mpsc::Sender<(String, String)>,
}

fn build() -> Option<(TrayIcon, MenuItem, MenuItem)> {
    let show = MenuItem::with_id(SHOW_ID, tr("tray.show"), true, None);
    let quit = MenuItem::with_id(QUIT_ID, tr("tray.quit"), true, None);
    let menu = Menu::new();
    menu.append_items(&[&show, &quit]).ok()?;
    let icon = Icon::from_rgba(app_icon::rgba(), app_icon::SIZE, app_icon::SIZE).ok()?;
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip("piTorrent")
        .with_icon(icon)
        .build()
        .ok()?;
    Some((tray, show, quit))
}

impl Tray {
    #[cfg(not(target_os = "linux"))]
    pub fn new() -> Option<Self> {
        let (_icon, show, quit) = build()?;
        Some(Self { _icon, show, quit })
    }

    /// The tray needs GTK initialized and its main loop running, which iced does not do.
    #[cfg(target_os = "linux")]
    pub fn new() -> Option<Self> {
        use std::sync::mpsc::{channel, RecvTimeoutError};
        use std::time::Duration;

        let (ready_tx, ready_rx) = channel();
        let (relabel, commands) = channel::<(String, String)>();
        std::thread::spawn(move || {
            let built = gtk::init().ok().and_then(|()| build());
            let _ = ready_tx.send(built.is_some());
            let Some((_tray, show, quit)) = built else { return };
            gtk::glib::timeout_add_local(Duration::from_millis(250), move || {
                while let Ok((s, q)) = commands.try_recv() {
                    show.set_text(s);
                    quit.set_text(q);
                }
                gtk::glib::ControlFlow::Continue
            });
            gtk::main();
        });
        match ready_rx.recv_timeout(Duration::from_secs(3)) {
            Ok(true) => Some(Self { relabel }),
            Ok(false) | Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => None,
        }
    }

    pub fn relabel(&self) {
        #[cfg(target_os = "linux")]
        let _ = self.relabel.send((tr("tray.show"), tr("tray.quit")));
        #[cfg(not(target_os = "linux"))]
        {
            self.show.set_text(tr("tray.show"));
            self.quit.set_text(tr("tray.quit"));
        }
    }

    pub fn poll(&self) -> Vec<TrayAction> {
        let mut out = Vec::new();
        while let Ok(e) = MenuEvent::receiver().try_recv() {
            match e.id.0.as_str() {
                SHOW_ID => out.push(TrayAction::Show),
                QUIT_ID => out.push(TrayAction::Quit),
                _ => {}
            }
        }
        while let Ok(e) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                out.push(TrayAction::Show);
            }
        }
        out
    }
}
