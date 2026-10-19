use crate::engine::{Detail, PIECE_CELLS, PriorityMove, State, TorrentInfo};
use crate::icons::{Ico, icon};
use crate::theme::{self, Mode, Palette};
use crate::i18n::{Language, plural, tr, trf};
use crate::widgets::*;
use crate::{
    AddMode, App, CatChoice, CategoryFilter, DetailTab, Dialog, Filter, Msg, PieceSize, SettingsTab, SortKey, View, fmt, geo, input, lines,
};
use iced::widget::{
    Space, button, column, container, mouse_area, pick_list, progress_bar, row, scrollable, stack,
    text_editor, text_input,
};
use iced::{Alignment, Color, Element, Length, Padding, alignment, mouse, window};

pub(crate) type El<'a> = Element<'a, Msg>;

pub(crate) const GROW: Length = Length::Fill;

fn state_color(p: Palette, s: State) -> Color {
    match s {
        State::Downloading | State::Checking => p.dl,
        State::Seeding | State::Completed => p.seed,
        State::Paused | State::Queued => p.pause,
        State::Error => p.err,
    }
}

fn state_label(s: State) -> String {
    tr(match s {
        State::Checking => "state.checking",
        State::Downloading => "state.downloading",
        State::Seeding => "state.seeding",
        State::Completed => "state.completed",
        State::Paused => "state.paused",
        State::Queued => "state.queued",
        State::Error => "state.error",
    })
}

fn filter_icon(f: Filter) -> Ico {
    match f {
        Filter::All => Ico::All,
        Filter::Downloading => Ico::Downloading,
        Filter::Seeding => Ico::Seeding,
        Filter::Completed => Ico::Completed,
        Filter::Paused => Ico::Paused,
        Filter::Error => Ico::Error,
    }
}

pub(crate) fn icon_button<'a>(p: Palette, ico: Ico, label_msg: Msg, enabled: bool) -> El<'a> {
    let b = button(
        container(icon(ico, 20.0, p.txt2))
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .width(30)
    .height(30)
    .padding(0)
    .style(theme::ghost(p, 8.0));
    if enabled { b.on_press(label_msg) } else { b }.into()
}

fn setting<'a>(p: Palette, label: String, hint: String, ctl: El<'a>, first: bool) -> El<'a> {
    let r = row![
        column![tx(label, 13.0, p.txt), tx(hint, 12.0, p.mute)].spacing(2).width(GROW),
        ctl
    ]
    .spacing(16)
    .align_y(Alignment::Center)
    .padding([10, 0]);
    if first { r.into() } else { column![hline(p), r].into() }
}

fn group<'a>(p: Palette, title: String, rows: Vec<El<'a>>) -> El<'a> {
    container(column![section_label(p, title), column(rows)].spacing(6))
        .padding(Padding { top: 14.0, right: 16.0, bottom: 4.0, left: 16.0 })
        .width(GROW)
        .style(theme::card(p))
        .into()
}

fn resize_grips<'a>() -> Vec<El<'a>> {
    use window::Direction as D;
    const E: f32 = 5.0;
    const C: f32 = 10.0;
    let grip = |dir: D, w: Length, h: Length, ax: alignment::Horizontal, ay: alignment::Vertical, it: mouse::Interaction| -> El<'a> {
        container(
            mouse_area(Space::new().width(w).height(h))
                .on_press(Msg::WindowResize(dir))
                .interaction(it),
        )
        .width(GROW)
        .height(GROW)
        .align_x(ax)
        .align_y(ay)
        .into()
    };
    use alignment::{Horizontal as H, Vertical as V};
    use mouse::Interaction as I;
    let (f, e, c) = (GROW, Length::Fixed(E), Length::Fixed(C));
    vec![
        grip(D::North, f, e, H::Left, V::Top, I::ResizingVertically),
        grip(D::South, f, e, H::Left, V::Bottom, I::ResizingVertically),
        grip(D::West, e, f, H::Left, V::Top, I::ResizingHorizontally),
        grip(D::East, e, f, H::Right, V::Top, I::ResizingHorizontally),
        grip(D::NorthWest, c, c, H::Left, V::Top, I::ResizingDiagonallyDown),
        grip(D::NorthEast, c, c, H::Right, V::Top, I::ResizingDiagonallyUp),
        grip(D::SouthWest, c, c, H::Left, V::Bottom, I::ResizingDiagonallyUp),
        grip(D::SouthEast, c, c, H::Right, V::Bottom, I::ResizingDiagonallyDown),
    ]
}

fn note_top<'a>(p: Palette, text: String) -> El<'a> {
    container(tx(text, 14.0, p.mute)).width(GROW).center_x(GROW).padding([40, 16]).into()
}

const FLAG_W: f32 = 20.0;

/// Country flag for a peer address; blank space of the same width when the country is unknown.
fn flag_cell<'a>(addr: &str) -> El<'a> {
    match geo::country(addr).and_then(|cc| geo::flag(cc).map(|h| (cc, h))) {
        Some((cc, h)) => iced::widget::tooltip(
            iced::widget::image(h.clone()).width(FLAG_W).height(FLAG_W * 0.75),
            container(mono(String::from_utf8_lossy(&cc).into_owned(), 12.0, Color::WHITE)).padding([2, 6]).style(container::dark),
            iced::widget::tooltip::Position::Right,
        )
        .into(),
        None => Space::new().width(FLAG_W).into(),
    }
}

fn peer_state(raw: &str) -> String {
    match raw {
        "live" | "connecting" | "queued" | "dead" | "not_needed" => tr(&format!("peer.{raw}")),
        other => other.to_string(),
    }
}

pub(crate) fn centered_note<'a>(p: Palette, text: String) -> El<'a> {
    container(tx(text, 14.0, p.mute)).center_x(GROW).center_y(GROW).into()
}

fn menu_item<'a>(p: Palette, ico: Ico, label: String, msg: Msg) -> El<'a> {
    button(
        container(
            row![icon(ico, 16.0, p.mute), tx(label, 13.0, p.txt)]
                .spacing(10)
                .align_y(Alignment::Center),
        )
        .center_y(Length::Fill),
    )
    .width(GROW)
    .height(32)
    .padding([0, 10])
    .style(theme::ghost(p, 7.0))
    .on_press(msg)
    .into()
}

impl App {
    pub fn view(&self) -> El<'_> {
        let p = self.palette();
        let page = container(
            column![self.header(p), self.body(p), self.footer(p)]
                .spacing(8)
                .padding(Padding { top: 0.0, right: 8.0, bottom: 8.0, left: 8.0 }),
        )
        .width(GROW)
        .height(GROW)
        .style(theme::page(p));

        let mut layers: Vec<El> = vec![page.into()];
        if self.menu {
            layers.push(mouse_area(Space::new().width(GROW).height(GROW)).on_press(Msg::CloseMenu).into());
            layers.push(
                container(self.add_menu(p))
                    .padding(Padding { top: 102.0, left: 68.0, right: 0.0, bottom: 0.0 })
                    .into(),
            );
        }
        if let Some(pos) = self.context {
            layers.push(
                mouse_area(Space::new().width(GROW).height(GROW))
                    .on_press(Msg::CloseContext)
                    .on_right_press(Msg::CloseContext)
                    .into(),
            );
            let win = input::window_size();
            let (x, y) = (
                pos.x.min(win.width - 300.0).max(0.0),
                pos.y.min(win.height - 450.0).max(0.0),
            );
            layers.push(
                container(self.context_menu(p))
                    .padding(Padding { top: y, left: x, right: 0.0, bottom: 0.0 })
                    .into(),
            );
        }
        if let Some(d) = self.dialog {
            let t = self.motion.dialog_t();
            let content = match d {
                Dialog::Add => self.add_dialog(p),
                Dialog::Create => self.create_dialog(p),
                Dialog::Delete => self.delete_dialog(p),
                Dialog::Category => self.category_dialog(p),
                Dialog::Move => self.move_dialog(p),
                Dialog::Plugin => self.plugin_dialog_view(p),
            };
            layers.push(
                mouse_area(container(Space::new()).width(GROW).height(GROW).style(theme::scrim(t)))
                    .on_press(Msg::CloseDialog)
                    .into(),
            );
            layers.push(
                container(mouse_area(content).on_press(Msg::Noop))
                    .center_x(GROW)
                    .center_y(GROW)
                    // Rises into place while the scrim fades in.
                    .padding(Padding { top: 16.0 + (1.0 - t) * 28.0, right: 16.0, bottom: 16.0, left: 16.0 })
                    .into(),
            );
        }
        layers.extend(resize_grips());
        stack(layers).into()
    }

    fn header(&self, p: Palette) -> El<'_> {
        let logo = iced::widget::image(crate::app_icon::handle()).width(24).height(24);
        let search = stack![
            text_input(
                &tr(if self.view == View::Search { "search.placeholder_trackers" } else { "search.placeholder" }),
                if self.view == View::Search { &self.tracker_query } else { &self.search },
            )
                .on_input(if self.view == View::Search { Msg::TrackerQuery } else { Msg::Search })
                .on_submit(Msg::SubmitSearch)
                .padding(Padding { top: 6.0, right: 12.0, bottom: 6.0, left: 34.0 })
                .size(fs(13.0))
                .style(theme::input(p)),
            container(icon(Ico::Search, 15.0, p.faint)).padding(Padding {
                top: 8.0,
                left: 11.0,
                right: 0.0,
                bottom: 0.0
            }),
        ];
        let control = |ico: Ico, msg: Msg, close: bool| -> El<'_> {
            let b = button(container(icon(ico, 14.0, p.txt2)).center_x(GROW).center_y(GROW))
                .width(46)
                .height(32)
                .padding(0)
                .on_press(msg);
            if close {
                b.style(theme::window_close(p)).into()
            } else {
                b.style(theme::ghost(p, 0.0)).into()
            }
        };
        let controls = row![
            control(Ico::Minimize, Msg::WindowMinimize, false),
            control(Ico::Square, Msg::WindowToggleMax, false),
            control(Ico::Close, Msg::WindowClose, true),
        ];
        let bar = row![
            row![logo, semibold("piTorrent", 14.0, p.txt)].spacing(10).align_y(Alignment::Center),
            container(container(search).max_width(440)).center_x(GROW),
            controls,
        ]
        .height(48)
        .align_y(Alignment::Center)
        .padding(Padding { top: 0.0, right: 0.0, bottom: 0.0, left: 8.0 })
        .spacing(16);
        mouse_area(bar).on_press(Msg::WindowDrag).on_double_click(Msg::WindowToggleMax).into()
    }

    fn body(&self, p: Palette) -> El<'_> {
        let main: El = match self.view {
            View::Torrents => column![self.list_island(p), self.details_island(p)].spacing(8).into(),
            View::Search => self.search_island(p),
            View::Settings => self.settings_island(p),
        };
        row![self.rail(p), main].spacing(6).height(GROW).into()
    }

    fn rail(&self, p: Palette) -> El<'_> {
        let mut col = column![].spacing(4).align_x(Alignment::Center);
        for f in Filter::ALL {
            let on = self.view == View::Torrents && self.filter == f;
            let count = self.snap.torrents.iter().filter(|t| f.test(t)).count();
            let btn = button(
                container(icon(filter_icon(f), 22.0, if on { p.txt } else { p.mute }))
                    .center_x(GROW)
                    .center_y(GROW),
            )
            .width(30)
            .height(30)
            .padding(0)
            .style(theme::tab(p, on, 8.0))
            .on_press(Msg::SetFilter(f));
            let overlay: El = if count > 0 {
                container(
                    container(tx(count.to_string(), 9.0, p.txt))
                        .padding([0, 4])
                        .style(theme::badge(p)),
                )
                .align_right(GROW)
                .align_top(GROW)
                .padding(Padding { top: -5.0, right: -6.0, bottom: 0.0, left: 0.0 })
                .into()
            } else {
                Space::new().into()
            };
            col = col.push(stack![btn, overlay].width(30).height(30));
        }
        let on = self.view == View::Settings;
        let gear = button(
            container(icon(Ico::Gear, 22.0, if on { p.txt } else { p.mute }))
                .center_x(GROW)
                .center_y(GROW),
        )
        .width(30)
        .height(30)
        .padding(0)
        .style(theme::tab(p, on, 8.0))
        .on_press(Msg::ShowView(View::Settings));
        let search_on = self.view == View::Search;
        let search = button(
            container(icon(Ico::Search, 22.0, if search_on { p.txt } else { p.mute }))
                .center_x(GROW)
                .center_y(GROW),
        )
        .width(30)
        .height(30)
        .padding(0)
        .style(theme::tab(p, search_on, 8.0))
        .on_press(Msg::ShowView(View::Search));
        column![col, Space::new().height(GROW), search, gear].spacing(4)
            .width(32)
            .padding(1)
            .align_x(Alignment::Center)
            .into()
    }

    fn footer(&self, p: Palette) -> El<'_> {
        let s = &self.snap;
        let mut r = row![
            icon(Ico::ArrowDown, 13.0, p.dl),
            mono(fmt::speed(s.down_bps), 12.0, p.txt),
            Space::new().width(8),
            icon(Ico::ArrowUp, 13.0, p.seed),
            mono(fmt::speed(s.up_bps), 12.0, p.txt),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        if let Some(n) = s.dht_nodes {
            r = r.push(Space::new().width(14)).push(tx(trf("status.dht", &[n.to_string()]), 12.0, p.mute));
        }
        if let Some(n) = &self.notice {
            r = r.push(Space::new().width(14)).push(tx(n.clone(), 12.0, p.err));
        }
        let port = match (&self.engine, s.port) {
            (None, _) => tr("status.starting"),
            (_, Some(port)) => trf("status.port", &[port.to_string()]),
            _ => tr("status.offline"),
        };
        row![r, hspace(), tx(port, 12.0, p.mute)]
            .height(28)
            .padding([0, 12])
            .align_y(Alignment::Center)
            .into()
    }

    fn context_menu(&self, p: Palette) -> El<'_> {
        let sep = || container(hline(p)).padding([4, 6]);
        container(
            column![
                menu_item(p, Ico::Play, tr("ctx.resume"), Msg::Resume),
                menu_item(p, Ico::Pause, tr("ctx.pause"), Msg::Pause),
                sep(),
                menu_item(p, Ico::ArrowUp, tr("ctx.priority_top"), Msg::Priority(PriorityMove::Top)),
                menu_item(p, Ico::ArrowUp, tr("ctx.priority_up"), Msg::Priority(PriorityMove::Up)),
                menu_item(p, Ico::ArrowDown, tr("ctx.priority_down"), Msg::Priority(PriorityMove::Down)),
                menu_item(p, Ico::ArrowDown, tr("ctx.priority_bottom"), Msg::Priority(PriorityMove::Bottom)),
                sep(),
                menu_item(p, Ico::File, tr("ctx.category"), Msg::OpenCategory),
                menu_item(p, Ico::Folder, tr("ctx.move"), Msg::OpenMove),
                sep(),
                menu_item(p, Ico::FolderOpen, tr("ctx.open_folder"), Msg::OpenFolder),
                menu_item(p, Ico::Link, tr("ctx.copy_magnet"), Msg::CopyMagnet),
                menu_item(p, Ico::Copy, tr("ctx.copy_hash"), Msg::CopyHash),
                sep(),
                menu_item(p, Ico::Trash, tr("ctx.delete"), Msg::AskDelete),
            ]
            .width(290),
        )
        .padding(4)
        .style(theme::menu(p))
        .into()
    }

    fn add_menu(&self, p: Palette) -> El<'_> {
        container(
            column![
                menu_item(p, Ico::File, tr("menu.from_file"), Msg::OpenAdd(AddMode::File)),
                menu_item(p, Ico::Link, tr("menu.from_magnet"), Msg::OpenAdd(AddMode::Magnet)),
                container(hline(p)).padding([4, 6]),
                menu_item(p, Ico::FileNew, tr("menu.create"), Msg::OpenCreate),
            ]
            .width(236),
        )
        .padding(4)
        .style(theme::menu(p))
        .into()
    }

    // ---------- список ----------

    fn list_island(&self, p: Palette) -> El<'_> {
        let visible = self.visible();
        let cur = self.current().map(|t| t.id);
        let has = cur.is_some();

        let add = button(
            container(
                row![
                    icon(Ico::Plus, 16.0, p.acc_ink),
                    semibold(tr("action.add"), 13.0, p.acc_ink),
                    icon(Ico::Chevron, 14.0, p.acc_ink),
                ]
                .spacing(7)
                .align_y(Alignment::Center),
            )
            .center_y(Length::Fill),
        )
        .height(30)
        .padding([0, 10])
        .style(theme::primary(p))
        .on_press(Msg::ToggleMenu);

        let cat_filter: El = if self.categories.list.is_empty() {
            Space::new().into()
        } else {
            let mut options = vec![CategoryFilter::All, CategoryFilter::Uncategorized];
            options.extend(self.categories.list.iter().map(|c| CategoryFilter::Named(c.name.clone())));
            pick_list(options, Some(self.category_filter.clone()), Msg::SetCategoryFilter)
                .text_size(fs(12.0))
                .style(theme::pick(p))
                .menu_style(theme::pick_menu(p))
                .into()
        };

        let toolbar = row![
            add,
            container(Space::new()).width(1).height(16).style(theme::fill(p.act, 0.0)),
            icon_button(p, Ico::Play, Msg::Resume, has),
            icon_button(p, Ico::Pause, Msg::Pause, has),
            icon_button(p, Ico::Trash, Msg::AskDelete, has),
            hspace(),
            cat_filter,
            tx(
                trf("list.shown", &[self.filter.label(), visible.len().to_string(), self.snap.torrents.len().to_string()]),
                12.0,
                p.mute
            ),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .padding([10, 12]);

        let sort = self.sort;
        let head = |key: SortKey, label: String, w: Length| -> El<'_> {
            let active = sort.filter(|(k, _)| *k == key);
            let mut r = row![tx(label.to_uppercase(), 11.0, if active.is_some() { p.txt } else { p.faint })]
                .spacing(4)
                .align_y(Alignment::Center);
            if let Some((_, asc)) = active {
                r = r.push(icon(if asc { Ico::ArrowUp } else { Ico::ArrowDown }, 10.0, p.txt));
            }
            button(r)
                .padding([4, 0])
                .width(w)
                .style(theme::ghost(p, 4.0))
                .on_press(Msg::Sort(key))
                .into()
        };
        let header = row![
            head(SortKey::Name, tr("col.name"), GROW),
            head(SortKey::Size, tr("col.size"), Length::Fixed(84.0)),
            head(SortKey::Progress, tr("col.progress"), Length::Fixed(170.0)),
            head(SortKey::Down, tr("col.down"), Length::Fixed(100.0)),
            head(SortKey::Up, tr("col.up"), Length::Fixed(100.0)),
            head(SortKey::Eta, tr("col.left"), Length::Fixed(84.0)),
        ]
        .spacing(12)
        .padding([0, 24])
        .height(34)
        .align_y(Alignment::Center);

        let rows = column(visible.iter().map(|t| {
            let on = self.multi.contains(&t.id) || (self.multi.is_empty() && Some(t.id) == cur);
            self.torrent_row(p, t, on)
        })).spacing(2);

        let list_body: El = if visible.is_empty() {
            centered_note(
                p,
                if self.engine.is_none() { tr("list.starting") } else { tr("list.empty") },
            )
        } else {
            scrollable(column![rows].padding([0, 8]).width(GROW)).height(GROW).into()
        };

        container(column![
            toolbar,
            hline(p),
            header,
            list_body,
        ])
        .width(GROW)
        .height(GROW)
        .style(theme::island(p))
        .into()
    }

    fn torrent_row<'a>(&'a self, p: Palette, t: &'a TorrentInfo, on: bool) -> El<'a> {
        let color = state_color(p, t.state);
        let (sub, sub_color) = match t.state {
            State::Error => (
                trf("state.error_with", &[t.error.clone().unwrap_or_else(|| tr("err.unknown"))]),
                p.err,
            ),
            State::Downloading | State::Seeding => (
                format!(
                    "{} · {}",
                    state_label(t.state),
                    plural(t.peers, "peers")
                ),
                if on { p.txt2 } else { p.mute },
            ),
            s => (state_label(s).to_string(), if on { p.txt2 } else { p.mute }),
        };
        let pct = fmt::percent(t.progress, t.total);
        let sub = match self.categories.of(&t.info_hash) {
            Some(c) => format!("{sub} · {c}"),
            None => sub,
        };
        let dot = container(Space::new()).width(8).height(8).style(theme::fill(color, 4.0));
        let one_line = iced::widget::text::Wrapping::None;
        let name = container(
            column![
                tx(t.name.clone(), 13.0, p.txt).wrapping(one_line),
                tx(sub, 11.5, sub_color).wrapping(one_line),
            ]
            .spacing(2),
        )
        .width(GROW)
        .clip(true);
        let m = |s: String| mono(s, 12.0, p.txt2).wrapping(iced::widget::text::Wrapping::None);
        let cell = row![
            progress_bar(0.0..=100.0, self.motion.bar(t.id, pct)).girth(5).style(theme::bar(p, color)),
            m(format!("{pct:.0}%")).width(34).align_x(alignment::Horizontal::Right),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let btn = button(
            container(row![
                row![dot, name].spacing(10).align_y(Alignment::Center).width(GROW).clip(true),
                m(fmt::bytes(t.total)).width(84),
                container(cell).width(170),
                m(fmt::speed(t.down_bps)).width(100),
                m(fmt::speed(t.up_bps)).width(100),
                m(fmt::eta(t.eta)).width(84),
            ]
            .spacing(12)
            .align_y(Alignment::Center))
            .width(GROW)
            .center_y(Length::Fill),
        )
        .width(GROW)
        .height(54)
        .padding([0, 16])
        .style(theme::row(p, on))
        .on_press(Msg::Select(t.id));
        mouse_area(btn).on_right_press(Msg::ContextMenu(t.id)).into()
    }

    // ---------- детали ----------

    fn details_island(&self, p: Palette) -> El<'_> {
        let cur = self.current();
        let tabs = [
            (DetailTab::General, tr("tab.general")),
            (DetailTab::Files, tr("tab.files")),
            (DetailTab::Peers, tr("tab.peers")),
        ]
        .into_iter()
        .map(|(t, label)| {
            let on = self.tab == t;
            button(tx(label, 13.0, if on { p.txt } else { p.txt2 }))
                .padding([5, 12])
                .style(theme::tab(p, on, 8.0))
                .on_press(Msg::Tab(t))
                .into()
        });
        let head = row![
            row(tabs).spacing(4),
            container(mono(cur.map(|t| t.name.clone()).unwrap_or_default(), 12.0, p.mute))
                .width(GROW)
                .align_right(GROW),
        ]
        .spacing(16)
        .padding([10, 12])
        .align_y(Alignment::Center);

        let detail = self
            .snap
            .detail
            .as_ref()
            .filter(|d| cur.is_some_and(|t| t.id == d.id));

        let body: El = match cur {
            None => centered_note(p, tr("detail.select")),
            Some(t) => scrollable(match self.tab {
                DetailTab::General => self.general_tab(p, t, detail),
                DetailTab::Files => self.files_tab(p, detail),
                DetailTab::Peers => self.peers_tab(p, detail),
            })
            .height(GROW)
            .into(),
        };

        let height = (input::window_size().height * 0.36).clamp(190.0, 292.0);
        container(column![head, hline(p), body])
            .width(GROW)
            .height(height)
            .style(theme::island(p))
            .into()
    }

    fn general_tab<'a>(&'a self, p: Palette, t: &'a TorrentInfo, d: Option<&'a Detail>) -> El<'a> {
        let ratio = if t.progress > 0 { t.uploaded as f64 / t.progress as f64 } else { 0.0 };
        let fields = [
            (tr("field.status"), state_label(t.state).to_string()),
            (tr("field.category"), self.categories.of(&t.info_hash).map(String::from).unwrap_or_else(|| "—".into())),
            (tr("col.size"), fmt::bytes(t.total)),
            (tr("field.downloaded"), fmt::bytes(t.progress)),
            (tr("field.uploaded"), fmt::bytes(t.uploaded)),
            (tr("field.ratio"), format!("{ratio:.2}")),
            (tr("tab.peers"), t.peers.to_string()),
        ];
        let mut grid = column![kv(p, &tr("field.folder"), t.folder.clone())].spacing(16);
        for chunk in fields.chunks(3) {
            let mut r = row![].spacing(24);
            for (k, v) in chunk {
                r = r.push(kv(p, k, v.clone()));
            }
            for _ in chunk.len()..3 {
                r = r.push(Space::new().width(GROW));
            }
            grid = grid.push(r);
        }

        let color = state_color(p, t.state);
        let cells: Vec<f32> = d.map(|d| d.cells.clone()).unwrap_or_else(|| vec![0.0; PIECE_CELLS]);
        let mut map = column![].spacing(2);
        for line in cells.chunks(PIECE_CELLS / 2) {
            map = map.push(row(line.iter().map(|&f| {
                let c = if f >= 0.999 {
                    color
                } else if f <= 0.0 {
                    p.track
                } else {
                    Color { r: p.track.r + (color.r - p.track.r) * f, g: p.track.g + (color.g - p.track.g) * f, b: p.track.b + (color.b - p.track.b) * f, a: 1.0 }
                };
                container(Space::new()).width(GROW).height(10).style(theme::fill(c, 2.0)).into()
            }))
            .spacing(2));
        }
        let pct = fmt::percent(t.progress, t.total);
        let caption = trf("pieces.caption", &[plural(t.pieces as usize, "pieces"), format!("{pct:.0}")]);

        row![
            container(grid).width(GROW),
            column![section_label(p, tr("pieces.title")), map, tx(caption, 12.0, p.mute)]
                .spacing(10)
                .width(GROW),
        ]
        .spacing(40)
        .padding([18, 20])
        .into()
    }

    fn files_tab<'a>(&'a self, p: Palette, d: Option<&'a Detail>) -> El<'a> {
        let Some(d) = d else {
            return note_top(p, tr("files.loading"));
        };
        if d.files.is_empty() {
            return note_top(p, tr("files.no_meta"));
        }
        let files: Vec<(Vec<String>, u64)> = d.files.iter().map(|f| (f.components.clone(), f.length)).collect();
        let tree = build_tree(&files);
        let picked: Vec<_> = d.files.iter().filter(|f| f.included).collect();
        let summary = trf(
            "files.summary",
            &[
                picked.len().to_string(),
                d.files.len().to_string(),
                fmt::bytes(picked.iter().map(|f| f.length).sum()),
            ],
        );
        let top = row![
            tx(summary, 12.0, p.mute).width(GROW),
            button(tx(tr("files.select_all"), 12.0, p.dl)).padding([3, 8]).style(theme::link(p)).on_press(Msg::FilesAll),
            button(tx(tr("files.select_none"), 12.0, p.dl)).padding([3, 8]).style(theme::link(p)).on_press(Msg::FilesNone),
        ]
        .spacing(8)
        .padding(Padding { top: 8.0, right: 16.0, bottom: 0.0, left: 16.0 })
        .align_y(Alignment::Center);

        let rows = tree.into_iter().map(|r| {
            let state = tri_state(&r.idxs, |i| d.files[i].included);
            let off = state == Some(false);
            let (done, total): (u64, u64) = r.idxs.iter().fold((0, 0), |a, &i| (a.0 + d.files[i].progress, a.1 + d.files[i].length));
            let pct = fmt::percent(done, total);
            let color = if off { p.off } else if pct >= 100.0 { p.seed } else { p.dl };
            let fade = if off { 0.5 } else { 1.0 };
            row![
                row![
                    Space::new().width((r.depth * 18) as f32),
                    checkbox(p, state, Msg::ToggleFile(r.idxs.clone())),
                    icon(if r.folder { Ico::Folder } else { Ico::File }, 15.0, p.mute),
                    container(tx(r.name.clone(), 13.0, Color { a: fade, ..p.txt }).wrapping(iced::widget::text::Wrapping::None))
                        .width(GROW)
                        .clip(true),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .width(GROW),
                mono(fmt::bytes(r.size), 12.0, p.txt2).width(84),
                container(progress_bar(0.0..=100.0, if off { 0.0 } else { pct }).girth(5).style(theme::bar(p, color))).width(140),
                mono(if off { "—".into() } else { format!("{pct:.0}%") }, 12.0, p.txt2)
                    .width(40)
                    .align_x(alignment::Horizontal::Right),
            ]
            .spacing(12)
            .height(36)
            .padding([0, 16])
            .align_y(Alignment::Center)
            .into()
        });
        column![top, column(rows).padding([4, 8])].into()
    }

    fn peers_tab<'a>(&'a self, p: Palette, d: Option<&'a Detail>) -> El<'a> {
        let head = |s: String| tx(s.to_uppercase(), 11.0, p.faint);
        let header = row![
            Space::new().width(FLAG_W),
            head(tr("peers.address")).width(Length::FillPortion(12)),
            head(tr("peers.client")).width(Length::FillPortion(12)),
            head(tr("peers.state")).width(100),
            head(tr("peers.received")).width(100),
            head(tr("field.uploaded")).width(100),
        ]
        .spacing(12)
        .padding([0, 16])
        .height(32)
        .align_y(Alignment::Center);
        let peers = d.map(|d| d.peers.as_slice()).unwrap_or_default();
        let rows = peers.iter().map(|x| {
            row![
                flag_cell(&x.addr),
                mono(x.addr.clone(), 12.0, p.txt).width(Length::FillPortion(12)),
                tx(x.client.clone(), 13.0, p.txt).width(Length::FillPortion(12)),
                tx(peer_state(&x.state), 12.0, p.txt2).width(100),
                mono(fmt::bytes(x.downloaded), 12.0, p.txt2).width(100),
                mono(fmt::bytes(x.uploaded), 12.0, p.txt2).width(100),
            ]
            .spacing(12)
            .padding([0, 16])
            .height(34)
            .align_y(Alignment::Center)
            .into()
        });
        let mut col = column![header, column(rows)].padding([4, 8]);
        if peers.is_empty() {
            col = col.push(note_top(p, tr("peers.none")));
        }
        col.into()
    }

    // ---------- настройки ----------

    fn settings_island(&self, p: Palette) -> El<'_> {
        let tabs = [(SettingsTab::Main, tr("settings.tab.main")), (SettingsTab::Trackers, tr("settings.tab.trackers"))]
            .into_iter()
            .map(|(t, label)| {
                let on = self.stab == t;
                button(tx(label, 13.0, if on { p.txt } else { p.txt2 }))
                    .padding([5, 12])
                    .style(theme::tab(p, on, 8.0))
                    .on_press(Msg::SettingsTab(t))
                    .into()
            });
        let head = row![semibold(tr("settings.title"), 15.0, p.txt), row(tabs).spacing(4)]
            .spacing(20)
            .padding(Padding { top: 10.0, right: 12.0, bottom: 10.0, left: 20.0 })
            .align_y(Alignment::Center);
        let body = match self.stab {
            SettingsTab::Main => self.settings_main(p),
            SettingsTab::Trackers => self.settings_plugins(p),
        };
        container(column![head, hline(p), scrollable(body).height(GROW)])
            .width(GROW)
            .height(GROW)
            .style(theme::island(p))
            .into()
    }

    fn settings_main(&self, p: Palette) -> El<'_> {
        let s = &self.settings;
        let theme_seg = segmented(
            p,
            vec![
                (tr("settings.dark").into(), s.theme == Mode::Dark, Msg::SetTheme(Mode::Dark)),
                (tr("settings.light").into(), s.theme == Mode::Light, Msg::SetTheme(Mode::Light)),
            ],
        );
        let lang_seg: El = pick_list(Language::ALL, Some(s.language), Msg::SetLanguage)
            .text_size(fs(13.0))
            .style(theme::pick(p))
            .menu_style(theme::pick_menu(p))
            .width(170)
            .into();
        let field = |value: String, w: f32, f: fn(String) -> Msg| -> El<'_> {
            text_input("", &value)
                .on_input(f)
                .size(fs(12.0))
                .font(MONO)
                .padding([6, 10])
                .width(w)
                .style(theme::input(p))
                .into()
        };
        let left = column![
            group(
                p,
                tr("settings.appearance"),
                vec![
                    setting(p, tr("settings.theme"), tr("settings.theme.hint"), theme_seg, true),
                    setting(p, tr("settings.language"), tr("settings.language.hint"), lang_seg, false),
                ]
            ),
            group(
                p,
                tr("settings.downloads"),
                vec![
                    setting(p, tr("settings.folder"), tr("settings.folder.hint"),
                        field(s.download_dir.clone(), 240.0, Msg::SetDir), true),
                    setting(p, tr("settings.start_now"), tr("settings.start_now.hint"),
                        switch(p, s.start_now, Msg::StartNowToggle), false),
                ]
            ),
        ]
        .spacing(12)
        .width(GROW);
        let limits = group(
                p,
                tr("settings.limits"),
                vec![
                    setting(p, tr("settings.limit_down"), tr("settings.limit.hint"),
                        field(s.down_limit.to_string(), 84.0, Msg::SetDownLimit), true),
                    setting(p, tr("settings.limit_up"), tr("settings.limit.hint"),
                        field(s.up_limit.to_string(), 84.0, Msg::SetUpLimit), false),
                ]
            );
        let left = left.push(limits).push(self.categories_group(p));
        let right = column![
            group(
                p,
                tr("settings.system"),
                vec![
                    setting(p, tr("settings.tray"), tr("settings.tray.hint"),
                        switch(p, s.close_to_tray, Msg::TrayToggle), true),
                    setting(p, tr("settings.autostart"), tr("settings.autostart.hint"),
                        switch(p, s.autostart, Msg::AutostartToggle), false),
                    setting(p, tr("settings.notify"), tr("settings.notify.hint"),
                        switch(p, s.notify_complete, Msg::NotifyToggle), false),
                    setting(p, tr("settings.assoc"), tr("settings.assoc.hint"),
                        button(tx(tr("settings.assoc.button"), 13.0, p.txt))
                            .padding([6, 12])
                            .style(theme::outline(p))
                            .on_press(Msg::RegisterAssoc)
                            .into(),
                        false),
                ]
            ),
            group(
                p,
                tr("settings.connection"),
                vec![
                    setting(p, tr("settings.port"), tr("settings.port.hint"),
                        field(s.port.to_string(), 84.0, Msg::SetPort), true),
                    setting(p, tr("settings.dht"), tr("settings.dht.hint"),
                        switch(p, s.dht, Msg::DhtToggle), false),
                ]
            ),
            group(
                p,
                tr("settings.queue"),
                vec![
                    setting(p, tr("settings.max_downloads"), tr("settings.max_downloads.hint"),
                        field(s.max_downloads.to_string(), 84.0, Msg::SetMaxDownloads), true),
                    setting(p, tr("settings.max_seeds"), tr("settings.max_seeds.hint"),
                        field(s.max_seeds.to_string(), 84.0, Msg::SetMaxSeeds), false),
                ]
            ),
        ]
        .spacing(12)
        .width(GROW);
        row![left, right].spacing(12).padding([16, 20]).into()
    }

    fn categories_group(&self, p: Palette) -> El<'_> {
        let mut rows: Vec<El> = Vec::new();
        for (i, c) in self.categories.list.iter().enumerate() {
            let hint = if c.path.is_empty() { tr("settings.cat.default_path") } else { c.path.clone() };
            rows.push(setting(
                p,
                c.name.clone(),
                hint,
                icon_button(p, Ico::Trash, Msg::CatRemove(c.name.clone()), true),
                i == 0,
            ));
        }
        let input = |placeholder: String, value: &str, f: fn(String) -> Msg| {
            text_input(&placeholder, value)
                .on_input(f)
                .size(fs(12.0))
                .font(MONO)
                .padding([6, 10])
                .style(theme::input(p))
        };
        let add_row: El = row![
            input(tr("settings.cat.name"), &self.new_cat_name, Msg::CatName).width(120),
            input(tr("settings.cat.path"), &self.new_cat_path, Msg::CatPath).width(GROW),
            icon_button(p, Ico::FolderOpen, Msg::CatBrowse, true),
            button(tx(tr("action.add"), 13.0, p.acc_ink))
                .padding([6, 12])
                .style(theme::primary(p))
                .on_press(Msg::CatAdd),
        ]
        .spacing(8)
        .padding([8, 0])
        .align_y(Alignment::Center)
        .into();
        rows.push(if rows.is_empty() { add_row } else { column![hline(p), add_row].into() });
        rows.push(tx(tr("settings.cat.hint"), 12.0, p.mute).into());
        group(p, tr("settings.categories"), rows)
    }

    // ---------- диалоги ----------

    pub(crate) fn dialog_frame<'a>(&self, p: Palette, title: String, body: El<'a>, ok: String, ok_msg: Option<Msg>) -> El<'a> {
        let close = button(container(icon(Ico::Close, 16.0, p.txt2)).center_x(GROW).center_y(GROW))
            .width(28)
            .height(28)
            .padding(0)
            .style(theme::ghost(p, 8.0))
            .on_press(Msg::CloseDialog);
        let head = row![semibold(title, 15.0, p.txt).width(GROW), close]
            .align_y(Alignment::Center)
            .padding(Padding { top: 14.0, right: 14.0, bottom: 10.0, left: 20.0 });
        let ok_btn = {
            let b = button(tx(ok, 13.0, p.acc_ink)).padding([6, 16]).style(theme::primary(p));
            match ok_msg { Some(m) => b.on_press(m), None => b }
        };
        let foot = row![
            hspace(),
            button(tx(tr("action.cancel"), 13.0, p.txt))
                .padding([6, 14])
                .style(theme::outline(p))
                .on_press(Msg::CloseDialog),
            ok_btn,
        ]
        .spacing(8)
        .padding([12, 20]);
        container(column![head, container(body).padding(Padding { top: 4.0, right: 20.0, bottom: 16.0, left: 20.0 }), hline(p), foot])
            .width(520)
            .style(theme::dialog(p))
            .into()
    }

    pub(crate) fn switch_row<'a>(&self, p: Palette, label: String, hint: String, on: bool, msg: Msg) -> El<'a> {
        column![
            hline(p),
            row![
                column![tx(label, 13.0, p.txt), tx(hint, 12.0, p.mute)].spacing(2).width(GROW),
                switch(p, on, msg),
            ]
            .spacing(16)
            .padding([10, 0])
            .align_y(Alignment::Center),
        ]
        .into()
    }

    fn add_dialog(&self, p: Palette) -> El<'_> {
        let a = &self.add;
        let seg = segmented(
            p,
            vec![
                (tr("add.tab_file").into(), a.mode == AddMode::File, Msg::AddMode(AddMode::File)),
                (tr("add.tab_magnet").into(), a.mode == AddMode::Magnet, Msg::AddMode(AddMode::Magnet)),
            ],
        );
        let mut body = column![container(seg)].spacing(14);

        let mut ready = false;
        match a.mode {
            AddMode::File => {
                let label = if a.loading {
                    tr("add.reading").to_string()
                } else if let Some(e) = &a.error {
                    trf("add.read_error", &[e.clone()])
                } else if let Some(pv) = &a.preview {
                    trf("add.pick_other", &[pv.name.clone()])
                } else {
                    tr("add.drop").to_string()
                };
                body = body.push(
                    button(
                        container(
                            column![icon(Ico::Upload, 22.0, p.mute), tx(label, 13.0, p.txt2)]
                                .spacing(8)
                                .align_x(Alignment::Center),
                        )
                        .center_x(GROW)
                        .center_y(GROW),
                    )
                    .width(GROW)
                    .height(104)
                    .style(theme::dropzone(p))
                    .on_press(Msg::PickTorrent),
                );
            }
            AddMode::Magnet => {
                let links = lines(&a.magnets);
                let can_fetch = links.len() == 1 && !a.fetching && a.preview.is_none();
                let fetch = button(tx(tr("add.fetch"), 13.0, p.txt)).padding([6, 12]).style(theme::outline(p));
                let paste = button(tx(tr("add.link_paste"), 13.0, p.txt))
                    .padding([6, 12])
                    .style(theme::outline(p))
                    .on_press(Msg::MagnetPaste);
                let status: El = if a.fetching {
                    row![
                        tx(tr("add.fetching"), 12.0, p.mute).width(GROW),
                        button(tx(tr("action.cancel"), 12.0, p.dl))
                            .padding([3, 8])
                            .style(theme::link(p))
                            .on_press(Msg::MagnetCancel),
                    ]
                    .align_y(Alignment::Center)
                    .into()
                } else if let Some(e) = &a.error {
                    tx(trf("add.fetch_failed", &[e.clone()]), 12.0, p.err).into()
                } else if links.len() > 1 {
                    tx(tr("add.multi"), 12.0, p.mute).into()
                } else if a.preview.is_none() {
                    tx(tr("add.magnet_note"), 12.0, p.mute).into()
                } else {
                    Space::new().into()
                };
                // Adding doesn't wait for the metadata: the engine fetches it itself.
                ready = a.preview.is_none() && !links.is_empty();
                body = body.push(
                    column![
                        tx(tr("add.magnets"), 12.0, p.mute),
                        text_editor(&a.magnets)
                            .placeholder("magnet:?xt=urn:btih:…")
                            .on_action(Msg::Magnets)
                            .size(fs(12.0))
                            .font(MONO)
                            .height(72)
                            .style(theme::editor(p)),
                        row![paste, if can_fetch { fetch.on_press(Msg::MagnetFetch) } else { fetch }, hspace()].spacing(8),
                        status,
                    ]
                    .spacing(8),
                );
            }
        }

        // The resolved file list looks the same for a .torrent file and for a magnet link.
        if let Some(pv) = &a.preview {
            ready = a.skip.len() < pv.files.len();
            let picked: Vec<_> = pv.files.iter().enumerate().filter(|(i, _)| !a.skip.contains(i)).collect();
            let summary = trf(
                "add.what",
                &[
                    picked.len().to_string(),
                    pv.files.len().to_string(),
                    fmt::bytes(picked.iter().map(|(_, f)| f.1).sum()),
                ],
            );
            let rows = a.tree.iter().map(|r| {
                let state = tri_state(&r.idxs, |i| !a.skip.contains(&i));
                let fade = if state == Some(false) { 0.5 } else { 1.0 };
                row![
                    Space::new().width((r.depth * 18) as f32),
                    checkbox(p, state, Msg::AddToggle(r.idxs.clone())),
                    icon(if r.folder { Ico::Folder } else { Ico::File }, 15.0, p.mute),
                    container(tx(r.name.clone(), 13.0, Color { a: fade, ..p.txt }).wrapping(iced::widget::text::Wrapping::None))
                        .width(GROW)
                        .clip(true),
                    mono(fmt::bytes(r.size), 12.0, Color { a: fade, ..p.txt2 })
                        .width(76)
                        .align_x(alignment::Horizontal::Right),
                ]
                .spacing(8)
                .height(32)
                .padding([0, 10])
                .align_y(Alignment::Center)
                .into()
            });
            body = body.push(
                column![
                    row![
                        tx(summary, 12.0, p.mute).width(GROW),
                        button(tx(tr("add.all"), 12.0, p.dl)).padding([3, 8]).style(theme::link(p)).on_press(Msg::AddAll),
                        button(tx(tr("add.none"), 12.0, p.dl)).padding([3, 8]).style(theme::link(p)).on_press(Msg::AddNone),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                    container(scrollable(column(rows)).height(Length::Shrink))
                        .max_height(176)
                        .padding(4)
                        .style(theme::input_box(p)),
                ]
                .spacing(8),
            );
        }

        if !self.categories.list.is_empty() {
            let mut choices = vec![CatChoice(None)];
            choices.extend(self.categories.list.iter().map(|c| CatChoice(Some(c.name.clone()))));
            body = body.push(
                column![
                    tx(tr("add.category"), 12.0, p.mute),
                    pick_list(choices, Some(CatChoice(a.category.clone())), |c| Msg::AddCategory(c.0))
                        .text_size(fs(13.0))
                        .style(theme::pick(p))
                        .menu_style(theme::pick_menu(p))
                        .width(GROW),
                ]
                .spacing(6),
            );
        }
        body = body
            .push(
                column![
                    tx(tr("add.folder"), 12.0, p.mute),
                    text_input("", &a.folder)
                        .on_input(Msg::AddFolder)
                        .size(fs(12.0))
                        .font(MONO)
                        .padding([6, 10])
                        .style(theme::input(p)),
                ]
                .spacing(6),
            )
            .push(self.switch_row(p, tr("add.start"), tr("add.start.hint"), a.start, Msg::AddStartToggle));

        self.dialog_frame(p, tr("add.title"), body.into(), tr("action.add"), ready.then_some(Msg::ConfirmAdd))
    }

    fn create_dialog(&self, p: Palette) -> El<'_> {
        let c = &self.create;
        let ghost_btn = |label: String, msg: Msg| -> El<'_> {
            button(tx(label, 13.0, p.txt)).padding([6, 12]).style(theme::outline(p)).on_press(msg).into()
        };
        let path_row = row![
            text_input(&tr("create.path.placeholder"), &c.path)
                .on_input(Msg::CreatePath)
                .size(fs(12.0))
                .font(MONO)
                .padding([6, 10])
                .width(GROW)
                .style(theme::input(p)),
            ghost_btn(tr("create.file"), Msg::PickCreate(false)),
            ghost_btn(tr("create.dir"), Msg::PickCreate(true)),
        ]
        .spacing(8);

        let mut body = column![
            column![tx(tr("create.path"), 12.0, p.mute), path_row].spacing(6),
            column![
                tx(tr("create.trackers"), 12.0, p.mute),
                text_editor(&c.trackers)
                    .placeholder("https://tracker.example.org/announce")
                    .on_action(Msg::Trackers)
                    .size(fs(12.0))
                    .font(MONO)
                    .height(72)
                    .style(theme::editor(p)),
            ]
            .spacing(6),
            column![
                tx(tr("create.piece"), 12.0, p.mute),
                pick_list(PieceSize::ALL, Some(c.piece), Msg::PieceSize)
                    .text_size(fs(13.0))
                    .style(theme::pick(p))
                    .menu_style(theme::pick_menu(p))
                    .width(GROW),
            ]
            .spacing(6),
        ]
        .spacing(14);
        if let Some(sz) = c.size {
            body = body.push(tx(trf("create.total", &[fmt::bytes(sz)]), 12.0, p.mute));
        }
        body = body.push(self.switch_row(
            p,
            tr("create.seed"),
            tr("create.seed.hint"),
            c.seed_now,
            Msg::SeedNowToggle,
        ));
        let ok = (!c.path.trim().is_empty()).then_some(Msg::ConfirmCreate);
        self.dialog_frame(p, tr("create.title"), body.into(), tr("action.create"), ok)
    }

    fn category_dialog(&self, p: Palette) -> El<'_> {
        let sel = self.selected_torrents();
        let first = sel.first().and_then(|t| self.categories.of(&t.info_hash));
        let common = sel.iter().all(|t| self.categories.of(&t.info_hash) == first);

        let body: El = if self.categories.list.is_empty() {
            column![
                tx(tr("category.empty"), 13.0, p.mute),
                button(tx(tr("category.open_settings"), 13.0, p.txt))
                    .padding([6, 12])
                    .style(theme::outline(p))
                    .on_press(Msg::ShowView(View::Settings)),
            ]
            .spacing(12)
            .into()
        } else {
            let mut choices: Vec<Option<String>> = vec![None];
            choices.extend(self.categories.list.iter().map(|c| Some(c.name.clone())));
            column(choices.into_iter().map(|c| {
                let active = common && c.as_deref() == first;
                let label = CatChoice(c.clone()).to_string();
                button(container(tx(label, 13.0, if active { p.txt } else { p.txt2 })).center_y(Length::Fill))
                    .width(GROW)
                    .height(34)
                    .padding([0, 12])
                    .style(theme::tab(p, active, 8.0))
                    .on_press(Msg::AssignCategory(c))
                    .into()
            }))
            .spacing(2)
            .into()
        };
        self.dialog_frame(p, tr("category.title"), body, tr("action.close"), Some(Msg::CloseDialog))
    }

    fn move_dialog(&self, p: Palette) -> El<'_> {
        let sel = self.selected_torrents();
        let name = match sel.as_slice() {
            [one] => one.name.clone(),
            many => trf("delete.many", &[many.len().to_string()]),
        };
        let body = column![
            tx(name, 13.0, p.txt),
            tx(tr("move.hint"), 12.0, p.mute),
            column![
                tx(tr("move.folder"), 12.0, p.mute),
                row![
                    text_input("", &self.move_path)
                        .on_input(Msg::MovePath)
                        .size(fs(12.0))
                        .font(MONO)
                        .padding([6, 10])
                        .width(GROW)
                        .style(theme::input(p)),
                    button(tx(tr("action.browse"), 13.0, p.txt))
                        .padding([6, 12])
                        .style(theme::outline(p))
                        .on_press(Msg::MoveBrowse),
                ]
                .spacing(8),
            ]
            .spacing(6),
        ]
        .spacing(12);
        let ok = (!self.move_path.trim().is_empty()).then_some(Msg::ConfirmMove);
        self.dialog_frame(p, tr("move.title"), body.into(), tr("action.move"), ok)
    }

    fn delete_dialog(&self, p: Palette) -> El<'_> {
        let sel = self.selected_torrents();
        let name = match sel.as_slice() {
            [one] => one.name.clone(),
            many => trf("delete.many", &[many.len().to_string()]),
        };
        let body = column![
            tx(name, 13.0, p.txt),
            self.switch_row(
                p,
                tr("delete.files"),
                tr("delete.files.hint"),
                self.delete_files,
                Msg::DeleteFilesToggle,
            ),
        ]
        .spacing(10);
        self.dialog_frame(p, tr("delete.title"), body.into(), tr("action.delete"), Some(Msg::ConfirmDelete))
    }
}
