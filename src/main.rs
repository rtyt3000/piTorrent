#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_icon;
mod assoc;
mod autostart;
mod categories;
mod engine;
mod fmt;
mod i18n;
mod notify;
mod platform;
mod plugins;
mod search;
mod icons;
mod input;
mod motion;
mod settings;
mod single_instance;
mod theme;
mod tray;
mod view;
mod view_plugins;
mod widgets;

use categories::Categories;
use plugins::{Manager, SearchResult, Values};
use search::{SearchSort, SearchState};
use engine::{AddOpts, MagnetPreview, PriorityMove, Preview, Shared, Snapshot, State, TorrentInfo};
use iced::widget::text_editor;
use i18n::{Language, tr, trf};
use iced::{Size, Subscription, Task, Theme, window};
use settings::Settings;
use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use tray::{Tray, TrayAction};
use theme::{Mode, Palette};
use widgets::{TreeRow, build_tree};

fn main() -> iced::Result {
    let minimized = std::env::args().any(|a| a == autostart::MINIMIZED_ARG);
    let arg = assoc::open_arg();
    let has_arg = arg.is_some();
    if !single_instance::acquire(minimized, arg) {
        return Ok(());
    }
    notify::init();
    let start_hidden = minimized && !has_arg && Settings::load().close_to_tray;
    iced::application(App::new, App::update, App::view)
        .title("piTorrent")
        .subscription(App::subscription)
        .theme(App::theme)
        .font(include_bytes!("../assets/fonts/Geist-Regular.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/Geist-Medium.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/Geist-SemiBold.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/GeistMono-Regular.ttf").as_slice())
        .default_font(widgets::SANS)
        .window(window::Settings {
            size: Size::new(1280.0, 820.0),
            min_size: Some(Size::new(960.0, 600.0)),
            position: window::Position::Centered,
            exit_on_close_request: false,
            decorations: false,
            visible: !start_hidden,
            icon: window::icon::from_rgba(app_icon::rgba(), app_icon::SIZE, app_icon::SIZE).ok(),
            ..Default::default()
        })
        .run()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Downloading,
    Seeding,
    Completed,
    Paused,
    Error,
}

impl Filter {
    pub const ALL: [Filter; 6] = [
        Filter::All,
        Filter::Downloading,
        Filter::Seeding,
        Filter::Completed,
        Filter::Paused,
        Filter::Error,
    ];

    pub fn label(self) -> String {
        tr(match self {
            Filter::All => "filter.all",
            Filter::Downloading => "filter.downloading",
            Filter::Seeding => "filter.seeding",
            Filter::Completed => "filter.completed",
            Filter::Paused => "filter.paused",
            Filter::Error => "filter.error",
        })
    }

    pub fn test(self, t: &TorrentInfo) -> bool {
        match self {
            Filter::All => true,
            Filter::Downloading => {
                matches!(t.state, State::Downloading | State::Checking)
                    || (t.state == State::Queued && !t.finished)
            }
            Filter::Seeding => t.state == State::Seeding || (t.state == State::Queued && t.finished),
            Filter::Completed => t.finished,
            Filter::Paused => matches!(t.state, State::Paused),
            Filter::Error => t.state == State::Error,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Torrents,
    Search,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DetailTab {
    General,
    Files,
    Peers,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Main,
    Trackers,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AddMode {
    File,
    Magnet,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Size,
    Progress,
    Down,
    Up,
    Eta,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Dialog {
    Add,
    Create,
    Delete,
    Category,
    Move,
    Plugin,
}

#[derive(Clone, PartialEq, Eq)]
pub enum CategoryFilter {
    All,
    Uncategorized,
    Named(String),
}

impl std::fmt::Display for CategoryFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CategoryFilter::All => f.write_str(&tr("category.all")),
            CategoryFilter::Uncategorized => f.write_str(&tr("category.none")),
            CategoryFilter::Named(n) => f.write_str(n),
        }
    }
}

/// A category choice in a pick list, `None` meaning "no category".
#[derive(Clone, PartialEq, Eq)]
pub struct CatChoice(pub Option<String>);

impl std::fmt::Display for CatChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(n) => f.write_str(n),
            None => f.write_str(&tr("category.none")),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PieceSize {
    Auto,
    K256,
    M1,
    M2,
    M4,
    M8,
}

impl PieceSize {
    pub const ALL: [PieceSize; 6] = [
        PieceSize::Auto,
        PieceSize::K256,
        PieceSize::M1,
        PieceSize::M2,
        PieceSize::M4,
        PieceSize::M8,
    ];

    fn bytes(self) -> Option<u32> {
        match self {
            PieceSize::Auto => None,
            PieceSize::K256 => Some(256 << 10),
            PieceSize::M1 => Some(1 << 20),
            PieceSize::M2 => Some(2 << 20),
            PieceSize::M4 => Some(4 << 20),
            PieceSize::M8 => Some(8 << 20),
        }
    }
}

impl std::fmt::Display for PieceSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.bytes() {
            None => f.write_str(&tr("piece.auto")),
            Some(b) if b < 1 << 20 => write!(f, "{} {}", b >> 10, tr("unit.kib")),
            Some(b) => write!(f, "{} {}", b >> 20, tr("unit.mib")),
        }
    }
}

pub struct AddState {
    pub mode: AddMode,
    pub bytes: Option<Vec<u8>>,
    pub preview: Option<Preview>,
    pub tree: Vec<TreeRow>,
    pub skip: HashSet<usize>,
    pub magnets: text_editor::Content,
    pub folder: String,
    pub start: bool,
    pub loading: bool,
    pub error: Option<String>,
    pub category: Option<String>,
    /// A magnet link's metadata is being fetched from the network.
    pub fetching: bool,
    /// Bumped whenever the pending fetch becomes irrelevant, so late results are ignored.
    pub fetch_token: u64,
    pub trackers: Vec<String>,
}

pub struct CreateState {
    pub path: String,
    pub size: Option<u64>,
    pub trackers: text_editor::Content,
    pub piece: PieceSize,
    pub seed_now: bool,
    pub busy: bool,
}

/// The plugin settings being edited; nothing is saved until the user confirms.
pub struct PluginDialog {
    pub id: String,
    pub values: Values,
    pub testing: bool,
    pub test: Option<Result<Option<String>, String>>,
}

pub struct App {
    pub engine: Option<Shared>,
    pub settings: Settings,
    pub snap: Snapshot,
    pub view: View,
    pub filter: Filter,
    pub search: String,
    pub selected: Option<usize>,
    pub multi: BTreeSet<usize>,
    pub anchor: Option<usize>,
    pub context: Option<iced::Point>,
    pub sort: Option<(SortKey, bool)>,
    pub tab: DetailTab,
    pub stab: SettingsTab,
    pub menu: bool,
    pub dialog: Option<Dialog>,
    pub add: AddState,
    pub create: CreateState,
    pub delete_files: bool,
    pub notice: Option<String>,
    pub tray: Option<Tray>,
    pub polling: bool,
    pub corners_done: bool,
    pub categories: Categories,
    pub category_filter: CategoryFilter,
    pub new_cat_name: String,
    pub new_cat_path: String,
    pub move_path: String,
    pub pending_open: Option<String>,
    pub plugins: Manager,
    pub sr: SearchState,
    pub tracker_query: String,
    pub plugin_dialog: Option<PluginDialog>,
    pub motion: motion::Motion,
    /// The last click on a torrent row, to recognise a double click.
    pub last_click: Option<(usize, std::time::Instant)>,
}

#[derive(Clone)]
pub enum Msg {
    Started(Result<Shared, String>),
    Tick,
    Frame(std::time::Instant),
    Snap(Snapshot),
    Noop,
    Done(Result<(), String>),
    Search(String),
    SetFilter(Filter),
    ShowView(View),
    Select(usize),
    ContextMenu(usize),
    CloseContext,
    SelectAll,
    Escape,
    Sort(SortKey),
    OpenFolder,
    CopyMagnet,
    CopyHash,
    Tab(DetailTab),
    SettingsTab(SettingsTab),
    ToggleMenu,
    CloseMenu,
    OpenAdd(AddMode),
    OpenCreate,
    CloseDialog,
    Resume,
    Pause,
    AskDelete,
    DeleteFilesToggle,
    ConfirmDelete,
    ToggleFile(Vec<usize>),
    FilesAll,
    FilesNone,
    AddMode(AddMode),
    PickTorrent,
    TorrentPicked(Option<(String, Vec<u8>)>),
    Previewed(Result<Preview, String>),
    AddToggle(Vec<usize>),
    AddAll,
    AddNone,
    AddFolder(String),
    AddStartToggle,
    Magnets(text_editor::Action),
    ConfirmAdd,
    FileDropped(PathBuf),
    CreatePath(String),
    PickCreate(bool),
    CreatePicked(Option<PathBuf>),
    Trackers(text_editor::Action),
    PieceSize(PieceSize),
    SeedNowToggle,
    ConfirmCreate,
    SetTheme(Mode),
    SetLanguage(Language),
    TrayToggle,
    AutostartToggle,
    TrayPoll,
    CloseRequested(window::Id),
    WindowDrag,
    WindowMinimize,
    WindowToggleMax,
    WindowClose,
    WindowResize(window::Direction),
    SetDir(String),
    StartNowToggle,
    SetPort(String),
    DhtToggle,
    SetDownLimit(String),
    SetUpLimit(String),
    SetMaxDownloads(String),
    SetMaxSeeds(String),
    TrackerQuery(String),
    SubmitSearch,
    SearchCategory(Option<String>),
    SearchStop,
    SearchFinished(u64, String, Result<Vec<SearchResult>, String>),
    SearchSortBy(SearchSort),
    SearchSelect(usize),
    SearchAdd(usize),
    SearchOpenPage(usize),
    ResultResolved(Result<plugins::Resolved, String>),
    TorrentFetched(Result<Vec<u8>, String>),
    PluginEnable(String),
    PluginConfigure(String),
    PluginField(String, String),
    PluginFieldToggle(String),
    PluginSave,
    PluginTest,
    PluginTested(Result<Option<String>, String>),
    PluginInstall,
    PluginFilePicked(Option<PathBuf>),
    PluginRemove(String),
    PluginsFolder,
    Added(Result<Vec<String>, String>, Option<String>),
    SetCategoryFilter(CategoryFilter),
    OpenCategory,
    AssignCategory(Option<String>),
    CatName(String),
    CatPath(String),
    CatAdd,
    CatRemove(String),
    CatBrowse,
    CatPicked(Option<PathBuf>),
    AddCategory(Option<String>),
    Priority(PriorityMove),
    OpenMove,
    MovePath(String),
    MoveBrowse,
    MovePicked(Option<PathBuf>),
    ConfirmMove,
    NotifyToggle,
    RegisterAssoc,
    OpenArg(String),
    PasteShortcut,
    ClipboardPasted(Option<String>),
    ClipboardForMagnet(Option<String>),
    MagnetPaste,
    MagnetPasted(Option<String>),
    MagnetFetch,
    MagnetFetched(u64, Result<MagnetPreview, String>),
    MagnetCancel,
}

pub fn lines(c: &text_editor::Content) -> Vec<String> {
    c.text()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn num(s: &str) -> u32 {
    s.trim().parse().unwrap_or(0)
}

impl App {
    fn new() -> (Self, Task<Msg>) {
        let settings = Settings::load();
        i18n::set(settings.language.resolve());
        if settings.autostart {
            let _ = autostart::set(true);
        }
        let start = settings.clone();
        let app = App {
            engine: None,
            add: AddState {
                mode: AddMode::File,
                bytes: None,
                preview: None,
                tree: Vec::new(),
                skip: HashSet::new(),
                magnets: text_editor::Content::new(),
                folder: settings.download_dir.clone(),
                start: settings.start_now,
                loading: false,
                error: None,
                category: None,
                fetching: false,
                fetch_token: 0,
                trackers: Vec::new(),
            },
            create: CreateState {
                path: String::new(),
                size: None,
                trackers: text_editor::Content::new(),
                piece: PieceSize::Auto,
                seed_now: true,
                busy: false,
            },
            settings,
            snap: Snapshot::default(),
            view: View::Torrents,
            filter: Filter::All,
            search: String::new(),
            selected: None,
            multi: BTreeSet::new(),
            anchor: None,
            context: None,
            sort: None,
            tab: DetailTab::General,
            stab: SettingsTab::Main,
            menu: false,
            dialog: None,
            delete_files: false,
            notice: None,
            tray: Tray::new(),
            polling: false,
            corners_done: false,
            categories: Categories::load(),
            category_filter: CategoryFilter::All,
            new_cat_name: String::new(),
            new_cat_path: String::new(),
            move_path: String::new(),
            pending_open: single_instance::take_startup_arg(),
            plugins: Manager::load(
                settings::app_dir().join("plugins"),
                Manager::default_store(settings::app_dir().join("plugins.json")),
            ),
            sr: SearchState::default(),
            tracker_query: String::new(),
            plugin_dialog: None,
            motion: motion::Motion::new(),
            last_click: None,
        };
        (
            app,
            Task::perform(async move { engine::Engine::start(&start).await }, Msg::Started),
        )
    }

    pub fn palette(&self) -> Palette {
        Palette::of(self.settings.theme)
    }

    fn theme(&self) -> Theme {
        self.palette().iced_theme()
    }

    pub fn visible(&self) -> Vec<&TorrentInfo> {
        let q = self.search.to_lowercase();
        self.snap
            .torrents
            .iter()
            .filter(|t| self.filter.test(t) && (q.is_empty() || t.name.to_lowercase().contains(&q)))
            .filter(|t| match &self.category_filter {
                CategoryFilter::All => true,
                CategoryFilter::Uncategorized => self.categories.of(&t.info_hash).is_none(),
                CategoryFilter::Named(n) => self.categories.of(&t.info_hash) == Some(n.as_str()),
            })
            .collect::<Vec<_>>()
            .tap_sort(self.sort)
    }

    /// Ids the toolbar and context menu act on: the multi-selection, or the focused torrent.
    pub fn targets(&self) -> Vec<usize> {
        let v = self.visible();
        let picked: Vec<usize> = v.iter().map(|t| t.id).filter(|id| self.multi.contains(id)).collect();
        if picked.is_empty() {
            self.current().map(|t| vec![t.id]).unwrap_or_default()
        } else {
            picked
        }
    }

    fn selected_torrents(&self) -> Vec<TorrentInfo> {
        let ids = self.targets();
        self.snap.torrents.iter().filter(|t| ids.contains(&t.id)).cloned().collect()
    }

    pub fn current(&self) -> Option<&TorrentInfo> {
        let v = self.visible();
        v.iter()
            .find(|t| Some(t.id) == self.selected)
            .or(v.first())
            .copied()
    }

    fn subscription(&self) -> Subscription<Msg> {
        Subscription::batch([
            iced::time::every(Duration::from_secs(1)).map(|_| Msg::Tick),
            iced::time::every(Duration::from_millis(200)).map(|_| Msg::TrayPoll),
            window::close_requests().map(Msg::CloseRequested),
            iced::event::listen_with(input::on_event),
            if self.motion.animating(self.dialog.is_some(), self.view == View::Search && self.sr.running()) {
                window::frames().map(Msg::Frame)
            } else {
                Subscription::none()
            },
        ])
    }

    fn act<F, Fut>(&self, f: F) -> Task<Msg>
    where
        F: FnOnce(Shared) -> Fut,
        Fut: std::future::Future<Output = Result<(), String>> + Send + 'static,
    {
        match &self.engine {
            Some(e) => Task::perform(f(e.clone()), Msg::Done),
            None => Task::none(),
        }
    }

    fn open_torrent_file(&mut self, bytes: Vec<u8>) -> Task<Msg> {
        self.add.bytes = Some(bytes.clone());
        self.add.preview = None;
        self.add.tree.clear();
        self.add.skip.clear();
        self.add.error = None;
        self.add.loading = true;
        let Some(e) = self.engine.clone() else {
            return Task::none();
        };
        Task::perform(async move { e.preview(bytes).await }, Msg::Previewed)
    }

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        let was_open = self.dialog.is_some();
        let (page, tab) = ((self.view, self.filter), (self.tab, self.stab));
        let task = self.update_inner(msg);
        if page != (self.view, self.filter) {
            self.motion.page_changed();
        }
        if tab != (self.tab, self.stab) {
            self.motion.tab_changed();
        }
        match (was_open, self.dialog.is_some()) {
            (false, true) => self.motion.dialog_opened(),
            (true, false) => self.motion.dialog_closed(),
            _ => {}
        }
        task
    }

    fn update_inner(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Started(Ok(e)) => {
                self.engine = Some(e);
                let tick = self.update(Msg::Tick);
                return match self.pending_open.take() {
                    Some(arg) => Task::batch([tick, self.update(Msg::OpenArg(arg))]),
                    None => tick,
                };
            }
            Msg::Started(Err(e)) => self.notice = Some(trf("notice.engine", &[e])),
            Msg::Tick => {
                if self.polling {
                    return Task::none();
                }
                let (Some(e), cur) = (self.engine.clone(), self.current().map(|t| t.id)) else {
                    return Task::none();
                };
                let peers = self.tab == DetailTab::Peers;
                self.polling = true;
                return Task::perform(
                    async move { e.snapshot(cur, peers).await },
                    Msg::Snap,
                );
            }
            Msg::Frame(_) => {}
            Msg::Snap(s) => {
                self.motion.snapshot(&s.torrents);
                self.notify_completed(&s);
                self.snap = s;
                self.polling = false;
            }
            Msg::Noop => {}
            Msg::Done(r) => self.notice = r.err(),
            Msg::Search(s) => self.search = s,
            Msg::SetFilter(f) => {
                self.filter = f;
                self.view = View::Torrents;
            }
            Msg::ShowView(v) => self.view = v,
            Msg::Select(id) => {
                self.select(id);
                let now = std::time::Instant::now();
                let double = self.last_click.is_some_and(|(last, at)| last == id && now.duration_since(at) < Duration::from_millis(400));
                if double && !input::ctrl() && !input::shift() {
                    self.last_click = None;
                    if let Some(t) = self.snap.torrents.iter().find(|t| t.id == id) {
                        open_target(&t.folder);
                    }
                } else {
                    self.last_click = Some((id, now));
                }
            }
            Msg::ContextMenu(id) => {
                if !self.multi.contains(&id) {
                    self.multi = BTreeSet::from([id]);
                    self.anchor = Some(id);
                }
                self.selected = Some(id);
                self.context = Some(input::cursor());
            }
            Msg::CloseContext => self.context = None,
            Msg::SelectAll => {
                self.multi = self.visible().iter().map(|t| t.id).collect();
            }
            Msg::Escape => {
                if self.context.is_some() {
                    self.context = None;
                } else if self.dialog.is_some() {
                    self.dialog = None;
                } else if self.menu {
                    self.menu = false;
                } else {
                    self.multi.clear();
                }
            }
            Msg::Sort(key) => {
                self.sort = match self.sort {
                    Some((k, asc)) if k == key => if asc { Some((k, false)) } else { None },
                    _ => Some((key, true)),
                };
            }
            Msg::OpenFolder => {
                self.context = None;
                if let Some(t) = self.current() {
                    open_target(&t.folder);
                }
            }
            Msg::CopyMagnet => {
                self.context = None;
                let links: Vec<String> = self.selected_torrents().iter().map(magnet).collect();
                return iced::clipboard::write(links.join("\n"));
            }
            Msg::CopyHash => {
                self.context = None;
                let hashes: Vec<String> = self.selected_torrents().into_iter().map(|t| t.info_hash).collect();
                return iced::clipboard::write(hashes.join("\n"));
            }
            Msg::Tab(t) => self.tab = t,
            Msg::SettingsTab(t) => self.stab = t,
            Msg::ToggleMenu => self.menu = !self.menu,
            Msg::CloseMenu => self.menu = false,
            Msg::OpenAdd(m) => {
                self.menu = false;
                self.dialog = Some(Dialog::Add);
                self.add.mode = m;
                self.add.folder = self.settings.download_dir.clone();
                self.add.start = self.settings.start_now;
                self.add.category = None;
                if m == AddMode::Magnet {
                    return iced::clipboard::read().map(Msg::ClipboardForMagnet);
                }
            }
            Msg::OpenCreate => {
                self.menu = false;
                self.dialog = Some(Dialog::Create);
            }
            Msg::CloseDialog => {
                self.dialog = None;
                self.plugin_dialog = None;
                self.reset_preview();
            }
            Msg::Resume => {
                self.context = None;
                let ids = self.targets();
                return self.act(move |e| async move {
                    for id in ids {
                        e.resume(id).await?;
                    }
                    Ok(())
                });
            }
            Msg::Pause => {
                self.context = None;
                let ids = self.targets();
                return self.act(move |e| async move {
                    for id in ids {
                        e.pause(id).await?;
                    }
                    Ok(())
                });
            }
            Msg::AskDelete => {
                self.context = None;
                if self.dialog.is_none() && !self.targets().is_empty() {
                    self.delete_files = false;
                    self.dialog = Some(Dialog::Delete);
                }
            }
            Msg::DeleteFilesToggle => self.delete_files = !self.delete_files,
            Msg::ConfirmDelete => {
                self.dialog = None;
                let ids = self.targets();
                if !ids.is_empty() {
                    let files = self.delete_files;
                    self.snap.torrents.retain(|t| !ids.contains(&t.id));
                    self.multi.clear();
                    return self.act(move |e| async move {
                        for id in ids {
                            e.remove(id, files).await?;
                        }
                        Ok(())
                    });
                }
            }
            Msg::ToggleFile(idxs) => {
                let Some(d) = self.snap.detail.as_mut() else {
                    return Task::none();
                };
                let all_on = idxs.iter().all(|&i| d.files[i].included);
                for &i in &idxs {
                    d.files[i].included = !all_on;
                }
                return self.push_selection();
            }
            Msg::FilesAll | Msg::FilesNone => {
                let on = matches!(msg, Msg::FilesAll);
                if let Some(d) = self.snap.detail.as_mut() {
                    d.files.iter_mut().for_each(|f| f.included = on);
                }
                return self.push_selection();
            }
            Msg::AddMode(m) => {
                if self.add.mode != m {
                    self.add.mode = m;
                    self.reset_preview();
                    if m == AddMode::Magnet {
                        return iced::clipboard::read().map(Msg::ClipboardForMagnet);
                    }
                }
            }
            Msg::PickTorrent => {
                return Task::perform(
                    async {
                        let f = rfd::AsyncFileDialog::new()
                            .add_filter("Torrent", &["torrent"])
                            .pick_file()
                            .await?;
                        Some((f.file_name(), f.read().await))
                    },
                    Msg::TorrentPicked,
                );
            }
            Msg::TorrentPicked(Some((_, bytes))) => return self.open_torrent_file(bytes),
            Msg::TorrentPicked(None) => {}
            Msg::FileDropped(p) => {
                if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("torrent")) {
                    match std::fs::read(&p) {
                        Ok(bytes) => {
                            self.dialog = Some(Dialog::Add);
                            self.add.mode = AddMode::File;
                            self.add.folder = self.settings.download_dir.clone();
                            return self.open_torrent_file(bytes);
                        }
                        Err(e) => self.notice = Some(e.to_string()),
                    }
                }
            }
            Msg::Previewed(Ok(p)) => {
                self.add.tree = build_tree(&p.files);
                self.add.preview = Some(p);
                self.add.loading = false;
            }
            Msg::Previewed(Err(e)) => {
                self.add.loading = false;
                self.add.bytes = None;
                self.add.error = Some(e);
            }
            Msg::AddToggle(idxs) => {
                let all_on = idxs.iter().all(|i| !self.add.skip.contains(i));
                for i in idxs {
                    if all_on {
                        self.add.skip.insert(i);
                    } else {
                        self.add.skip.remove(&i);
                    }
                }
            }
            Msg::AddAll => self.add.skip.clear(),
            Msg::AddNone => {
                if let Some(p) = &self.add.preview {
                    self.add.skip = (0..p.files.len()).collect();
                }
            }
            Msg::AddFolder(s) => self.add.folder = s,
            Msg::AddStartToggle => self.add.start = !self.add.start,
            Msg::Magnets(a) => {
                let edited = a.is_edit();
                self.add.magnets.perform(a);
                if edited {
                    self.reset_preview();
                }
            }
            Msg::ClipboardForMagnet(Some(text)) => {
                let text = text.trim();
                if self.add.magnets.text().trim().is_empty() && is_magnet(text) {
                    return self.set_magnet_text(text);
                }
            }
            Msg::ClipboardForMagnet(None) => {}
            Msg::MagnetPaste => return iced::clipboard::read().map(Msg::MagnetPasted),
            Msg::MagnetPasted(Some(text)) => {
                if is_magnet(text.trim()) {
                    return self.set_magnet_text(text.trim());
                }
            }
            Msg::MagnetPasted(None) => {}
            Msg::MagnetFetch => return self.start_fetch(),
            Msg::MagnetFetched(token, result) => {
                if token == self.add.fetch_token && self.add.fetching {
                    self.add.fetching = false;
                    match result {
                        Ok(m) => {
                            self.add.tree = build_tree(&m.preview.files);
                            self.add.preview = Some(m.preview);
                            self.add.bytes = Some(m.bytes);
                            self.add.trackers = m.trackers;
                        }
                        Err(e) => self.add.error = Some(e),
                    }
                }
            }
            Msg::MagnetCancel => self.reset_preview(),
            Msg::PasteShortcut => {
                if self.dialog.is_none() && self.engine.is_some() {
                    return iced::clipboard::read().map(Msg::ClipboardPasted);
                }
            }
            Msg::ClipboardPasted(Some(text)) => {
                let text = text.trim().trim_matches('"');
                if is_magnet(text) {
                    return self.update(Msg::OpenArg(text.lines().next().unwrap_or(text).to_string()));
                }
                if text.to_lowercase().ends_with(".torrent") && std::path::Path::new(text).is_file() {
                    return self.update(Msg::FileDropped(PathBuf::from(text)));
                }
            }
            Msg::ClipboardPasted(None) => {}
            Msg::ConfirmAdd => return self.confirm_add(),
            Msg::CreatePath(s) => {
                self.create.path = s;
                self.create.size = None;
            }
            Msg::PickCreate(folder) => {
                return Task::perform(
                    async move {
                        let d = rfd::AsyncFileDialog::new();
                        if folder {
                            d.pick_folder().await
                        } else {
                            d.pick_file().await
                        }
                        .map(|h| h.path().to_path_buf())
                    },
                    Msg::CreatePicked,
                );
            }
            Msg::CreatePicked(Some(p)) => {
                self.create.size = Some(engine::dir_size(&p));
                self.create.path = p.to_string_lossy().into_owned();
            }
            Msg::CreatePicked(None) => {}
            Msg::Trackers(a) => self.create.trackers.perform(a),
            Msg::PieceSize(s) => self.create.piece = s,
            Msg::SeedNowToggle => self.create.seed_now = !self.create.seed_now,
            Msg::ConfirmCreate => return self.confirm_create(),
            Msg::SetTheme(m) => {
                self.settings.theme = m;
                self.settings.save();
            }
            Msg::SetLanguage(l) => {
                self.settings.language = l;
                self.settings.save();
                i18n::set(l.resolve());
                if let Some(t) = &self.tray {
                    t.relabel();
                }
            }
            Msg::AutostartToggle => {
                let enabled = !self.settings.autostart;
                match autostart::set(enabled) {
                    Ok(()) => {
                        self.settings.autostart = enabled;
                        self.settings.save();
                    }
                    Err(e) => self.notice = Some(e),
                }
            }
            Msg::TrayToggle => {
                self.settings.close_to_tray = !self.settings.close_to_tray;
                self.settings.save();
            }
            Msg::TrayPoll => {
                let mut actions = self.tray.as_ref().map(Tray::poll).unwrap_or_default();
                if single_instance::take_show_request() {
                    actions.push(TrayAction::Show);
                }
                let opens: Vec<Task<Msg>> = single_instance::take_open_requests()
                    .into_iter()
                    .map(|arg| self.update(Msg::OpenArg(arg)))
                    .collect();
                let mut round = Vec::new();
                if !self.corners_done {
                    self.corners_done = true;
                    round.push(
                        window::oldest()
                            .and_then(|id| window::run(id, platform::round_corners).discard()),
                    );
                }
                let tasks: Vec<_> = actions
                    .into_iter()
                    .map(|a| match a {
                        TrayAction::Show => {
                            self.motion.hidden = false;
                            window::oldest().and_then(|id| {
                                Task::batch([
                                    window::set_mode(id, window::Mode::Windowed),
                                    window::gain_focus(id),
                                ])
                            })
                        }
                        TrayAction::Quit => self.quit(),
                    })
                    .collect();
                return Task::batch(tasks.into_iter().chain(round).chain(opens));
            }
            Msg::WindowDrag => return window::oldest().and_then(window::drag),
            Msg::WindowMinimize => return window::oldest().and_then(|id| window::minimize(id, true)),
            Msg::WindowToggleMax => return window::oldest().and_then(window::toggle_maximize),
            Msg::WindowClose => {
                return window::oldest().and_then(|id| Task::done(Msg::CloseRequested(id)));
            }
            Msg::WindowResize(dir) => return window::oldest().and_then(move |id| window::drag_resize(id, dir)),
            Msg::CloseRequested(id) => {
                return if self.settings.close_to_tray && self.tray.is_some() {
                    self.motion.hidden = true;
                    window::set_mode(id, window::Mode::Hidden)
                } else {
                    self.quit()
                };
            }
            Msg::SetDir(s) => {
                self.settings.download_dir = s;
                self.settings.save();
            }
            Msg::StartNowToggle => {
                self.settings.start_now = !self.settings.start_now;
                self.settings.save();
            }
            Msg::SetPort(s) => {
                self.settings.port = num(&s).min(65535) as u16;
                self.settings.save();
            }
            Msg::DhtToggle => {
                self.settings.dht = !self.settings.dht;
                self.settings.save();
            }
            Msg::SetDownLimit(s) => {
                self.settings.down_limit = num(&s);
                self.apply_limits();
            }
            Msg::SetUpLimit(s) => {
                self.settings.up_limit = num(&s);
                self.apply_limits();
            }
            Msg::SetMaxDownloads(s) => {
                self.settings.max_downloads = num(&s).min(999);
                self.apply_queue();
            }
            Msg::SetMaxSeeds(s) => {
                self.settings.max_seeds = num(&s).min(999);
                self.apply_queue();
            }
            Msg::Added(Ok(hashes), category) => {
                if let Some(c) = category {
                    for h in &hashes {
                        self.categories.set(h, Some(&c));
                    }
                    self.categories.save();
                }
                self.notice = None;
            }
            Msg::Added(Err(e), _) => self.notice = Some(e),
            Msg::SetCategoryFilter(f) => self.category_filter = f,
            Msg::OpenCategory => {
                self.context = None;
                if !self.targets().is_empty() {
                    self.dialog = Some(Dialog::Category);
                }
            }
            Msg::AssignCategory(c) => {
                for t in self.selected_torrents() {
                    self.categories.set(&t.info_hash, c.as_deref());
                }
                self.categories.save();
                self.dialog = None;
            }
            Msg::CatName(s) => self.new_cat_name = s,
            Msg::CatPath(s) => self.new_cat_path = s,
            Msg::CatAdd => {
                if self.categories.add(&self.new_cat_name, &self.new_cat_path) {
                    self.categories.save();
                    self.new_cat_name.clear();
                    self.new_cat_path.clear();
                }
            }
            Msg::CatRemove(name) => {
                if self.category_filter == CategoryFilter::Named(name.clone()) {
                    self.category_filter = CategoryFilter::All;
                }
                self.categories.remove(&name);
                self.categories.save();
            }
            Msg::CatBrowse => return pick_folder(Msg::CatPicked),
            Msg::CatPicked(Some(p)) => self.new_cat_path = p.to_string_lossy().into_owned(),
            Msg::CatPicked(None) => {}
            Msg::AddCategory(c) => {
                self.add.folder = c
                    .as_deref()
                    .and_then(|n| self.categories.path_of(n))
                    .map(String::from)
                    .unwrap_or_else(|| self.settings.download_dir.clone());
                self.add.category = c;
            }
            Msg::Priority(mv) => {
                self.context = None;
                let ids = self.targets();
                if let Some(e) = &self.engine {
                    e.move_priority(&ids, mv);
                }
            }
            Msg::OpenMove => {
                self.context = None;
                if !self.targets().is_empty() {
                    self.move_path = self.settings.download_dir.clone();
                    self.dialog = Some(Dialog::Move);
                }
            }
            Msg::MovePath(s) => self.move_path = s,
            Msg::MoveBrowse => return pick_folder(Msg::MovePicked),
            Msg::MovePicked(Some(p)) => self.move_path = p.to_string_lossy().into_owned(),
            Msg::MovePicked(None) => {}
            Msg::ConfirmMove => {
                self.dialog = None;
                let ids = self.targets();
                let base = PathBuf::from(self.move_path.trim());
                return self.act(move |e| async move {
                    for id in ids {
                        e.move_torrent(id, base.clone()).await?;
                    }
                    Ok(())
                });
            }
            Msg::NotifyToggle => {
                self.settings.notify_complete = !self.settings.notify_complete;
                self.settings.save();
            }
            Msg::RegisterAssoc => return Task::perform(async { assoc::register() }, Msg::Done),
            Msg::OpenArg(arg) => {
                if self.engine.is_none() {
                    self.pending_open = Some(arg);
                } else if arg.to_lowercase().starts_with("magnet:") {
                    self.dialog = Some(Dialog::Add);
                    self.add.mode = AddMode::Magnet;
                    self.add.folder = self.settings.download_dir.clone();
                    self.add.start = self.settings.start_now;
                    self.add.category = None;
                    return self.set_magnet_text(&arg);
                } else {
                    return self.update(Msg::FileDropped(PathBuf::from(arg)));
                }
            }
            Msg::TrackerQuery(q) => self.tracker_query = q,
            Msg::SubmitSearch => return self.start_search(),
            Msg::SearchCategory(c) => self.sr.category = c,
            Msg::SearchStop => self.stop_search(),
            Msg::SearchFinished(token, id, result) => self.search_finished(token, id, result),
            Msg::SearchSortBy(k) => self.sr.set_sort(k),
            Msg::SearchSelect(i) => self.sr.selected = Some(i),
            Msg::SearchAdd(i) => {
                if let Some(r) = self.sr.results.get(i).cloned() {
                    return self.add_result(r);
                }
            }
            Msg::SearchOpenPage(i) => {
                if let Some(url) = self.sr.results.get(i).and_then(|r| r.page_url.clone()) {
                    open_target(&url);
                }
            }
            Msg::ResultResolved(Ok(res)) => {
                if let Some(m) = res.magnet {
                    return self.update(Msg::OpenArg(m));
                }
                if let Some(b64) = res.torrent_base64 {
                    use base64::Engine as _;
                    return match base64::engine::general_purpose::STANDARD.decode(b64.trim()) {
                        Ok(bytes) => self.update(Msg::TorrentFetched(Ok(bytes))),
                        Err(e) => self.update(Msg::TorrentFetched(Err(e.to_string()))),
                    };
                }
                if let Some(url) = res.torrent_url {
                    return self.fetch_torrent_url(url, None);
                }
                self.notice = Some(tr("search.no_link"));
            }
            Msg::ResultResolved(Err(e)) => self.notice = Some(e),
            Msg::TorrentFetched(Ok(bytes)) => {
                self.dialog = Some(Dialog::Add);
                self.add.mode = AddMode::File;
                self.add.folder = self.settings.download_dir.clone();
                self.add.start = self.settings.start_now;
                self.add.category = None;
                return self.open_torrent_file(bytes);
            }
            Msg::TorrentFetched(Err(e)) => self.notice = Some(trf("search.fetch_failed", &[e])),
            Msg::PluginEnable(id) => {
                if let Some(pl) = self.plugins.get(&id) {
                    let on = self.plugins.store.is_enabled(&pl.meta);
                    self.plugins.store.set_enabled(&id, !on);
                }
            }
            Msg::PluginConfigure(id) => {
                if let Some(pl) = self.plugins.get(&id) {
                    self.plugin_dialog = Some(PluginDialog {
                        id,
                        values: self.plugins.settings_of(&pl),
                        testing: false,
                        test: None,
                    });
                    self.dialog = Some(Dialog::Plugin);
                }
            }
            Msg::PluginField(key, value) => {
                if let Some(d) = &mut self.plugin_dialog {
                    d.values.insert(key, value);
                    d.test = None;
                }
            }
            Msg::PluginFieldToggle(key) => {
                if let Some(d) = &mut self.plugin_dialog {
                    let on = d.values.get(&key).is_some_and(|v| v == "true");
                    d.values.insert(key, (!on).to_string());
                }
            }
            Msg::PluginSave => {
                if let Some(d) = self.plugin_dialog.take() {
                    if let Some(pl) = self.plugins.get(&d.id) {
                        for f in &pl.meta.settings {
                            let value = d.values.get(&f.key).cloned().unwrap_or_default();
                            if let Err(e) = self.plugins.store.set_value(&pl.meta, &f.key, &value) {
                                self.notice = Some(e);
                            }
                        }
                    }
                }
                self.dialog = None;
            }
            Msg::PluginTest => {
                let Some(d) = &mut self.plugin_dialog else {
                    return Task::none();
                };
                let Some(pl) = self.plugins.get(&d.id) else {
                    return Task::none();
                };
                d.testing = true;
                d.test = None;
                let values = d.values.clone();
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || pl.check(values, Arc::new(AtomicBool::new(false))))
                            .await
                            .map_err(|e| e.to_string())
                            .and_then(|r| r.map_err(|e| e.0))
                    },
                    Msg::PluginTested,
                );
            }
            Msg::PluginTested(result) => {
                if let Some(d) = &mut self.plugin_dialog {
                    d.testing = false;
                    d.test = Some(result);
                }
            }
            Msg::PluginInstall => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .add_filter("Plugin", &["js"])
                            .pick_file()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    Msg::PluginFilePicked,
                );
            }
            Msg::PluginFilePicked(Some(path)) => {
                if let Err(e) = self.plugins.install(&path) {
                    self.notice = Some(trf("plugins.install_failed", &[e.0]));
                }
            }
            Msg::PluginFilePicked(None) => {}
            Msg::PluginRemove(id) => self.plugins.remove(&id),
            Msg::PluginsFolder => {
                let _ = std::fs::create_dir_all(&self.plugins.dir);
                open_target(&self.plugins.dir.to_string_lossy());
            }
        }
        Task::none()
    }

    fn quit(&self) -> Task<Msg> {
        match self.engine.clone() {
            Some(e) => Task::future(async move { e.shutdown().await }).then(|_| iced::exit()),
            None => iced::exit(),
        }
    }

    fn select(&mut self, id: usize) {
        if input::shift() && let Some(anchor) = self.anchor {
            let ids: Vec<usize> = self.visible().iter().map(|t| t.id).collect();
            let pos = |x: usize| ids.iter().position(|&i| i == x);
            if let (Some(a), Some(b)) = (pos(anchor), pos(id)) {
                let (lo, hi) = (a.min(b), a.max(b));
                self.multi = ids[lo..=hi].iter().copied().collect();
                self.selected = Some(id);
                return;
            }
        }
        if input::ctrl() {
            if !self.multi.remove(&id) {
                self.multi.insert(id);
            }
            self.anchor = Some(id);
            self.selected = Some(id);
            return;
        }
        self.multi = BTreeSet::from([id]);
        self.anchor = Some(id);
        self.selected = Some(id);
    }

    fn apply_queue(&self) {
        self.settings.save();
        if let Some(e) = &self.engine {
            e.set_queue_limits(self.settings.max_downloads, self.settings.max_seeds);
        }
    }

    fn apply_limits(&self) {
        self.settings.save();
        if let Some(e) = &self.engine {
            e.set_limits(self.settings.down_limit, self.settings.up_limit);
        }
    }

    fn push_selection(&self) -> Task<Msg> {
        let Some(d) = &self.snap.detail else {
            return Task::none();
        };
        let id = d.id;
        let on: Vec<usize> = (0..d.files.len()).filter(|&i| d.files[i].included).collect();
        self.act(move |e| async move { e.set_only_files(id, on).await })
    }

    /// Forgets the resolved file list and cancels a pending metadata fetch.
    fn reset_preview(&mut self) {
        self.add.bytes = None;
        self.add.preview = None;
        self.add.tree.clear();
        self.add.skip.clear();
        self.add.trackers.clear();
        self.add.error = None;
        self.add.loading = false;
        self.add.fetching = false;
        self.add.fetch_token += 1;
    }

    fn set_magnet_text(&mut self, text: &str) -> Task<Msg> {
        self.dialog = Some(Dialog::Add);
        self.add.mode = AddMode::Magnet;
        self.add.magnets = text_editor::Content::with_text(text);
        self.start_fetch()
    }

    /// Resolves a single magnet link into a file list; several links are added without it.
    fn start_fetch(&mut self) -> Task<Msg> {
        self.reset_preview();
        let links = lines(&self.add.magnets);
        let ([link], Some(engine)) = (links.as_slice(), self.engine.clone()) else {
            return Task::none();
        };
        if !is_magnet(link) {
            return Task::none();
        }
        self.add.fetching = true;
        let (token, link) = (self.add.fetch_token, link.clone());
        Task::perform(async move { engine.preview_magnet(link).await }, move |r| Msg::MagnetFetched(token, r))
    }

    fn confirm_add(&mut self) -> Task<Msg> {
        let folder = self.add.folder.trim().to_string();
        let start = self.add.start;
        let category = self.add.category.clone();
        let Some(engine) = self.engine.clone() else {
            return Task::none();
        };
        if let (Some(bytes), Some(p)) = (self.add.bytes.clone(), &self.add.preview) {
            let only_files = (!self.add.skip.is_empty()).then(|| {
                (0..p.files.len())
                    .filter(|i| !self.add.skip.contains(i))
                    .collect::<Vec<_>>()
            });
            let trackers = self.add.trackers.clone();
            self.finish_add();
            return Task::perform(
                async move {
                    let opts = AddOpts { folder, start, only_files, trackers };
                    engine.add_bytes(bytes, opts).await.map(|h| vec![h])
                },
                move |r| Msg::Added(r, category.clone()),
            );
        }
        if self.add.mode != AddMode::Magnet {
            return Task::none();
        }
        let links = lines(&self.add.magnets);
        if links.is_empty() {
            return Task::none();
        }
        self.add.magnets = text_editor::Content::new();
        self.finish_add();
        Task::perform(
            async move {
                let mut hashes = Vec::new();
                for l in links {
                    let opts = AddOpts { folder: folder.clone(), start, only_files: None, trackers: Vec::new() };
                    hashes.push(engine.add_magnet(l, opts).await?);
                }
                Ok(hashes)
            },
            move |r| Msg::Added(r, category.clone()),
        )
    }

    /// Adds a search result: a magnet link goes through the usual window, a .torrent link is
    /// downloaded through the plugin's own session first, and a bare page is resolved by the plugin.
    fn add_result(&mut self, r: SearchResult) -> Task<Msg> {
        if let Some(m) = &r.magnet {
            return self.update(Msg::OpenArg(m.clone()));
        }
        let Some(plugin) = self.plugins.get(&r.plugin_id) else {
            return Task::none();
        };
        if let Some(url) = r.torrent_url.clone() {
            return self.fetch_torrent_url(url, Some(r.plugin_id));
        }
        if plugin.meta.has_resolve {
            let settings = self.plugins.settings_of(&plugin);
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || plugin.resolve(settings, &r, Arc::new(AtomicBool::new(false))))
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|res| res.map_err(|e| e.0))
                },
                Msg::ResultResolved,
            );
        }
        match r.page_url {
            Some(url) => open_target(&url),
            None => self.notice = Some(tr("search.no_link")),
        }
        Task::none()
    }

    fn fetch_torrent_url(&mut self, url: String, plugin_id: Option<String>) -> Task<Msg> {
        let plugin = plugin_id.or_else(|| self.sr.results.first().map(|r| r.plugin_id.clone()));
        let Some(plugin) = plugin.and_then(|id| self.plugins.get(&id)) else {
            return Task::none();
        };
        let settings = self.plugins.settings_of(&plugin);
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || plugin.fetch_torrent(settings, &url, Arc::new(AtomicBool::new(false))))
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|res| res.map_err(|e| e.0))
            },
            Msg::TorrentFetched,
        )
    }

    fn notify_completed(&self, new: &Snapshot) {
        if !self.settings.notify_complete {
            return;
        }
        for t in &new.torrents {
            let was_downloading = self.snap.torrents.iter().any(|o| o.id == t.id && !o.finished);
            if was_downloading && t.finished && t.state != State::Error {
                notify::send(tr("notify.completed"), t.name.clone());
            }
        }
    }

    fn finish_add(&mut self) {
        self.dialog = None;
        self.add.category = None;
        self.reset_preview();
    }

    fn confirm_create(&mut self) -> Task<Msg> {
        let path = PathBuf::from(self.create.path.trim());
        if !path.exists() {
            self.notice = Some(tr("notice.not_found"));
            return Task::none();
        }
        let trackers = lines(&self.create.trackers);
        let piece = self.create.piece.bytes();
        let seed_now = self.create.seed_now;
        self.dialog = None;
        let stem = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "torrent".into());
        self.act(move |e| async move {
            let (bytes, folder) = e.create(path, trackers, piece).await?;
            let target = rfd::AsyncFileDialog::new()
                .set_file_name(format!("{stem}.torrent"))
                .add_filter("Torrent", &["torrent"])
                .save_file()
                .await;
            if let Some(t) = target {
                t.write(&bytes).await.map_err(|e| e.to_string())?;
            }
            if seed_now {
                e.add_bytes(
                    bytes,
                    AddOpts {
                        folder: folder.to_string_lossy().into_owned(),
                        start: true,
                        only_files: None,
                        trackers: Vec::new(),
                    },
                )
                .await?;
            }
            Ok(())
        })
    }
}

trait SortExt<'a> {
    fn tap_sort(self, sort: Option<(SortKey, bool)>) -> Vec<&'a TorrentInfo>;
}

impl<'a> SortExt<'a> for Vec<&'a TorrentInfo> {
    fn tap_sort(mut self, sort: Option<(SortKey, bool)>) -> Self {
        let Some((key, asc)) = sort else {
            self.sort_by_key(|t| t.priority);
            return self;
        };
        self.sort_by(|a, b| {
            let ord = match key {
                SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortKey::Size => a.total.cmp(&b.total),
                SortKey::Progress => {
                    fmt::percent(a.progress, a.total).total_cmp(&fmt::percent(b.progress, b.total))
                }
                SortKey::Down => a.down_bps.cmp(&b.down_bps),
                SortKey::Up => a.up_bps.cmp(&b.up_bps),
                SortKey::Eta => a.eta.unwrap_or(u64::MAX).cmp(&b.eta.unwrap_or(u64::MAX)),
            };
            if asc { ord } else { ord.reverse() }
        });
        self
    }
}

fn is_magnet(text: &str) -> bool {
    text.to_lowercase().starts_with("magnet:")
}

fn pick_folder(done: fn(Option<PathBuf>) -> Msg) -> Task<Msg> {
    Task::perform(
        async { rfd::AsyncFileDialog::new().pick_folder().await.map(|h| h.path().to_path_buf()) },
        done,
    )
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn magnet(t: &TorrentInfo) -> String {
    format!("magnet:?xt=urn:btih:{}&dn={}", t.info_hash, percent_encode(&t.name))
}

/// Opens a folder in the file manager or an http(s) link in the browser. The target is passed
/// as a single argument, never through a shell.
fn open_target(target: &str) {
    if target.contains("://") && !(target.starts_with("http://") || target.starts_with("https://")) {
        return;
    }
    let opener = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(target).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: usize, name: &str, total: u64, eta: Option<u64>) -> TorrentInfo {
        TorrentInfo {
            id,
            priority: 0,
            info_hash: format!("hash{id}"),
            name: name.into(),
            folder: String::new(),
            total,
            progress: 0,
            uploaded: 0,
            state: State::Paused,
            finished: false,
            error: None,
            down_bps: 0,
            up_bps: 0,
            eta,
            peers: 0,
            pieces: 1,
        }
    }

    #[test]
    fn magnet_encodes_name() {
        assert_eq!(magnet(&t(1, "My Film (2020).mkv", 1, None)), "magnet:?xt=urn:btih:hash1&dn=My%20Film%20%282020%29.mkv");
        assert_eq!(percent_encode("Фильм"), "%D0%A4%D0%B8%D0%BB%D1%8C%D0%BC");
    }

    #[test]
    fn sorting_by_name_size_and_eta() {
        let items = [t(1, "b", 30, None), t(2, "A", 10, Some(5)), t(3, "c", 20, Some(1))];
        let ids = |sort| {
            items.iter().collect::<Vec<_>>().tap_sort(sort).iter().map(|x| x.id).collect::<Vec<_>>()
        };
        assert_eq!(ids(None), [1, 2, 3]);
        assert_eq!(ids(Some((SortKey::Name, true))), [2, 1, 3]);
        assert_eq!(ids(Some((SortKey::Size, false))), [1, 3, 2]);
        assert_eq!(ids(Some((SortKey::Eta, true))), [3, 2, 1]);
    }
}
