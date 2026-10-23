use iced::widget::svg;
use iced::widget::svg::Handle;
use iced::{Color, Element, Length};
use std::sync::OnceLock;

#[derive(Clone, Copy)]
pub enum Ico {
    Logo,
    Search,
    Plus,
    Chevron,
    File,
    Link,
    FileNew,
    Play,
    Pause,
    Trash,
    All,
    Downloading,
    Seeding,
    Completed,
    Paused,
    Error,
    Gear,
    Folder,
    Check,
    Dash,
    Upload,
    Close,
    ArrowDown,
    ArrowUp,
    Copy,
    FolderOpen,
    Square,
    Minimize,
}

const ALL: [Ico; 28] = [Ico::Logo, Ico::Search, Ico::Plus, Ico::Chevron, Ico::File, Ico::Link, Ico::FileNew, Ico::Play, Ico::Pause, Ico::Trash, Ico::All, Ico::Downloading, Ico::Seeding, Ico::Completed, Ico::Paused, Ico::Error, Ico::Gear, Ico::Folder, Ico::Check, Ico::Dash, Ico::Upload, Ico::Close, Ico::ArrowDown, Ico::ArrowUp, Ico::Copy, Ico::FolderOpen, Ico::Square, Ico::Minimize];

impl Ico {
    fn source(self) -> &'static [u8] {
        match self {
            Ico::Logo => include_bytes!("../assets/icons/logo.svg"),
            Ico::Search => include_bytes!("../assets/icons/search.svg"),
            Ico::Plus => include_bytes!("../assets/icons/plus.svg"),
            Ico::Chevron => include_bytes!("../assets/icons/chevron.svg"),
            Ico::File => include_bytes!("../assets/icons/file.svg"),
            Ico::Link => include_bytes!("../assets/icons/link.svg"),
            Ico::FileNew => include_bytes!("../assets/icons/file_new.svg"),
            Ico::Play => include_bytes!("../assets/icons/play.svg"),
            Ico::Pause => include_bytes!("../assets/icons/pause.svg"),
            Ico::Trash => include_bytes!("../assets/icons/trash.svg"),
            Ico::All => include_bytes!("../assets/icons/all.svg"),
            Ico::Downloading => include_bytes!("../assets/icons/downloading.svg"),
            Ico::Seeding => include_bytes!("../assets/icons/seeding.svg"),
            Ico::Completed => include_bytes!("../assets/icons/completed.svg"),
            Ico::Paused => include_bytes!("../assets/icons/paused.svg"),
            Ico::Error => include_bytes!("../assets/icons/error.svg"),
            Ico::Gear => include_bytes!("../assets/icons/gear.svg"),
            Ico::Folder => include_bytes!("../assets/icons/folder.svg"),
            Ico::Check => include_bytes!("../assets/icons/check.svg"),
            Ico::Dash => include_bytes!("../assets/icons/dash.svg"),
            Ico::Upload => include_bytes!("../assets/icons/upload.svg"),
            Ico::Close => include_bytes!("../assets/icons/close.svg"),
            Ico::ArrowDown => include_bytes!("../assets/icons/arrow_down.svg"),
            Ico::ArrowUp => include_bytes!("../assets/icons/arrow_up.svg"),
            Ico::Copy => include_bytes!("../assets/icons/copy.svg"),
            Ico::FolderOpen => include_bytes!("../assets/icons/folder_open.svg"),
            Ico::Square => include_bytes!("../assets/icons/square.svg"),
            Ico::Minimize => include_bytes!("../assets/icons/minimize.svg"),
        }
    }
}

fn handles() -> &'static Vec<Handle> {
    static H: OnceLock<Vec<Handle>> = OnceLock::new();
    H.get_or_init(|| ALL.iter().map(|i| Handle::from_memory(i.source())).collect())
}

/// The folder arrow: down when open, right when closed.
pub fn chevron<'a, M: 'a>(open: bool, size: f32, color: Color) -> Element<'a, M> {
    let angle = if open { 0.0 } else { -std::f32::consts::FRAC_PI_2 };
    svg(handles()[Ico::Chevron as usize].clone())
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .rotation(iced::Rotation::Solid(iced::Radians(angle)))
        .style(move |_, _| svg::Style { color: Some(color) })
        .into()
}

pub fn icon<'a, M: 'a>(i: Ico, size: f32, color: Color) -> Element<'a, M> {
    svg(handles()[i as usize].clone())
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_, _| svg::Style { color: Some(color) })
        .into()
}
