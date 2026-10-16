use crate::app_icon;
use crate::i18n::tr;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub enum TrayAction {
    Show,
    Quit,
}

pub struct Tray {
    _icon: TrayIcon,
    show: MenuItem,
    quit: MenuItem,
}

impl Tray {
    pub fn new() -> Option<Self> {
        let show = MenuItem::new(tr("tray.show"), true, None);
        let quit = MenuItem::new(tr("tray.quit"), true, None);
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
        Some(Self { _icon: tray, show, quit })
    }

    pub fn relabel(&self) {
        self.show.set_text(tr("tray.show"));
        self.quit.set_text(tr("tray.quit"));
    }

    pub fn poll(&self) -> Vec<TrayAction> {
        let mut out = Vec::new();
        while let Ok(e) = MenuEvent::receiver().try_recv() {
            if e.id == *self.show.id() {
                out.push(TrayAction::Show);
            } else if e.id == *self.quit.id() {
                out.push(TrayAction::Quit);
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
