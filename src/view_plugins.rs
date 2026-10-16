//! Screens of the plugin system: tracker search, the plugin list and the plugin settings dialog.

use crate::i18n::{tr, trf};
use crate::plugins::{FieldKind, SearchResult};
use crate::search::{SearchSort, SourceStatus};
use crate::theme::{self, Palette};
use crate::view::{El, GROW, centered_note, icon_button};
use crate::widgets::*;
use crate::{App, Msg};
use iced::widget::{Space, button, column, container, pick_list, row, scrollable, text_input};
use iced::{Alignment, Color, Length, Padding};

/// A category in the search filter; `None` is "all".
#[derive(Clone, PartialEq, Eq)]
pub struct SearchCat(pub Option<String>);

impl std::fmt::Display for SearchCat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            None => f.write_str(&tr("search.all_categories")),
            Some(c) => f.write_str(&category_name(c)),
        }
    }
}

pub fn category_name(id: &str) -> String {
    match id {
        "movies" | "tv" | "music" | "games" | "software" | "books" | "anime" => tr(&format!("cat.{id}")),
        other => other.to_string(),
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Choice {
    value: String,
    label: String,
}

impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

const W_SIZE: f32 = 90.0;
const W_PEERS: f32 = 64.0;
const W_SOURCE: f32 = 140.0;
const W_DATE: f32 = 96.0;
const W_ACTIONS: f32 = 150.0;

impl App {
    pub fn search_island(&self, p: Palette) -> El<'_> {
        let s = &self.sr;

        let mut cats = vec![SearchCat(None)];
        for known in ["movies", "tv", "music", "games", "software", "books", "anime"] {
            if self.plugins.searchable().iter().any(|pl| pl.meta.categories.iter().any(|c| c == known)) {
                cats.push(SearchCat(Some(known.to_string())));
            }
        }
        let category = pick_list(cats, Some(SearchCat(s.category.clone())), |c| Msg::SearchCategory(c.0))
            .text_size(fs(12.0))
            .style(theme::pick(p))
            .menu_style(theme::pick_menu(p));

        let go = button(semibold(tr("search.button"), 13.0, p.acc_ink))
            .padding([6, 16])
            .style(theme::primary(p));
        let go = if self.tracker_query.trim().is_empty() { go } else { go.on_press(Msg::SubmitSearch) };

        let summary = if !s.searched {
            String::new()
        } else if s.running() {
            tr("search.running")
        } else {
            trf("search.summary", &[s.results.len().to_string(), s.sources.len().to_string()])
        };

        let mut toolbar = row![category, go].spacing(8).align_y(Alignment::Center);
        if s.running() {
            toolbar = toolbar.push(
                button(tx(tr("search.cancel"), 13.0, p.txt))
                    .padding([6, 12])
                    .style(theme::outline(p))
                    .on_press(Msg::SearchStop),
            );
        }
        let toolbar = row![toolbar, hspace(), tx(summary, 12.0, p.mute)]
            .align_y(Alignment::Center)
            .padding([10, 12]);

        let mut body = column![toolbar, hline(p)];

        if s.searched && !s.sources.is_empty() {
            let chips = s.sources.iter().map(|(_, name, status)| {
                let (label, color) = match status {
                    SourceStatus::Running => (format!("{name} …"), Color { a: 0.45 + 0.55 * self.motion.pulse(), ..p.txt2 }),
                    SourceStatus::Done(n) => (format!("{name} · {n}"), p.txt2),
                    SourceStatus::Failed(_) => (format!("{name} ✕"), p.err),
                };
                container(tx(label, 12.0, color)).padding([3, 10]).style(theme::fill(p.elev, 10.0)).into()
            });
            let mut info = column![row(chips).spacing(8)].spacing(4);
            for (_, name, status) in &s.sources {
                if let SourceStatus::Failed(e) = status {
                    info = info.push(tx(format!("{name}: {e}"), 12.0, p.err));
                }
            }
            body = body.push(container(info).padding([8, 12]));
        }

        if s.results.is_empty() {
            let note = if self.plugins.searchable().is_empty() {
                tr("search.no_sources")
            } else if !s.searched {
                tr("search.hint")
            } else if s.running() {
                tr("search.running")
            } else {
                tr("search.none_found")
            };
            return container(column![body, centered_note(p, note)])
                .width(GROW)
                .height(GROW)
                .style(theme::island(p))
                .into();
        }

        let sort = s.sort;
        let head = |key: SearchSort, label: String, w: Length| -> El<'_> {
            let active = sort.0 == key;
            let mut r = row![tx(label.to_uppercase(), 11.0, if active { p.txt } else { p.faint })]
                .spacing(4)
                .align_y(Alignment::Center);
            if active {
                r = r.push(crate::icons::icon(
                    if sort.1 { crate::icons::Ico::ArrowUp } else { crate::icons::Ico::ArrowDown },
                    10.0,
                    p.txt,
                ));
            }
            button(r).padding([4, 0]).width(w).style(theme::ghost(p, 4.0)).on_press(Msg::SearchSortBy(key)).into()
        };
        let header = row![
            head(SearchSort::Title, tr("col.name"), GROW),
            head(SearchSort::Size, tr("col.size"), Length::Fixed(W_SIZE)),
            head(SearchSort::Seeders, tr("col.seeds"), Length::Fixed(W_PEERS)),
            head(SearchSort::Leechers, tr("col.leech"), Length::Fixed(W_PEERS)),
            head(SearchSort::Source, tr("col.source"), Length::Fixed(W_SOURCE)),
            head(SearchSort::Date, tr("col.date"), Length::Fixed(W_DATE)),
            Space::new().width(W_ACTIONS),
        ]
        .spacing(12)
        .padding([0, 24])
        .height(34)
        .align_y(Alignment::Center);

        let rows = column(s.results.iter().enumerate().map(|(i, r)| self.result_row(p, i, r))).spacing(2);
        container(column![body, header, scrollable(column![rows].padding([0, 8]).width(GROW)).height(GROW)])
            .width(GROW)
            .height(GROW)
            .style(theme::island(p))
            .into()
    }

    fn result_row<'a>(&'a self, p: Palette, i: usize, r: &'a SearchResult) -> El<'a> {
        let on = self.sr.selected == Some(i);
        let one_line = iced::widget::text::Wrapping::None;
        let m = |s: String| mono(s, 12.0, p.txt2).wrapping(one_line);
        let num = |n: Option<u32>, color: Color| match n {
            Some(n) => mono(n.to_string(), 12.0, color),
            None => mono("—", 12.0, p.faint),
        };
        let mut source = row![].spacing(6).align_y(Alignment::Center);
        if let Some(icon) = self.plugins.get(&r.plugin_id).and_then(|pl| pl.icon.clone()) {
            source = source.push(iced::widget::image(icon).width(16).height(16));
        }
        let source = source.push(tx(r.plugin_name.clone(), 12.0, p.txt2).wrapping(one_line));
        let sub = r.category.as_deref().map(category_name).unwrap_or_default();
        let name = container(
            column![tx(r.title.clone(), 13.0, p.txt).wrapping(one_line), tx(sub, 11.5, p.mute).wrapping(one_line)].spacing(2),
        )
        .width(GROW)
        .clip(true);
        let main = button(
            container(
                row![
                    name,
                    m(r.size.map(crate::fmt::bytes).unwrap_or_else(|| "—".into())).width(W_SIZE),
                    num(r.seeders, p.seed).width(W_PEERS),
                    num(r.leechers, p.err).width(W_PEERS),
                    source.width(W_SOURCE),
                    m(r.date.clone().unwrap_or_else(|| "—".into())).width(W_DATE),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .width(GROW)
            .center_y(Length::Fill),
        )
        .width(GROW)
        .height(54)
        .padding([0, 16])
        .style(theme::row(p, on))
        .on_press(Msg::SearchSelect(i));

        let busy = self.adding == Some(i);
        let add = button(tx(tr(if busy { "search.adding" } else { "action.add" }), 12.0, p.acc_ink))
            .padding([5, 12])
            .style(theme::primary(p));
        let add = if self.adding.is_some() { add } else { add.on_press(Msg::SearchAdd(i)) };
        let mut actions = row![add].spacing(4).align_y(Alignment::Center).width(W_ACTIONS - 12.0);
        if r.page_url.is_some() {
            actions = actions.push(icon_button(p, crate::icons::Ico::Link, Msg::SearchOpenPage(i), true));
        }
        row![main, container(actions).center_y(54)].spacing(4).align_y(Alignment::Center).into()
    }

    // ---------- plugin list ----------

    pub fn settings_plugins(&self, p: Palette) -> El<'_> {
        let mut cards: Vec<El> = Vec::new();
        for plugin in &self.plugins.plugins {
            let meta = &plugin.meta;
            let enabled = self.plugins.store.is_enabled(meta);
            let missing = self.plugins.store.missing_required(meta);
            let letters: String = meta.name.chars().filter(|c| c.is_alphanumeric()).take(2).collect();
            let badge = match &plugin.icon {
                Some(icon) => container(iced::widget::image(icon.clone()).width(24).height(24)),
                None => container(mono(letters, 13.0, p.dl)),
            }
            .width(38)
            .height(38)
            .center_x(38)
            .center_y(38)
            .style(theme::fill(Color { a: 0.18, ..p.dl }, 10.0));

            let (status, status_color) = if !missing.is_empty() {
                (trf("plugins.needs", &[missing.join(", ")]), p.pause)
            } else if enabled {
                (tr("plugins.on"), p.seed)
            } else {
                (tr("plugins.off"), p.mute)
            };
            let network = if meta.hosts.is_empty() {
                tr("plugins.no_network")
            } else {
                trf("plugins.network", &[meta.hosts.iter().map(|h| if h.starts_with("$setting:") { tr("plugins.host_from_settings") } else { h.clone() }).collect::<Vec<_>>().join(", ")])
            };
            let title = if plugin.bundled {
                format!("{} · {} · {}", meta.name, meta.version, tr("plugins.bundled"))
            } else {
                format!("{} · {}", meta.name, meta.version)
            };

            let mut buttons = row![].spacing(6).align_y(Alignment::Center);
            if !meta.settings.is_empty() {
                buttons = buttons.push(
                    button(tx(tr("plugins.configure"), 12.0, p.txt))
                        .padding([4, 10])
                        .style(theme::outline(p))
                        .on_press(Msg::PluginConfigure(meta.id.clone())),
                );
            }
            if !plugin.bundled {
                buttons = buttons.push(
                    button(tx(tr("plugins.remove"), 12.0, p.err))
                        .padding([4, 10])
                        .style(theme::outline(p))
                        .on_press(Msg::PluginRemove(meta.id.clone())),
                );
            }
            let switch_el: El = if missing.is_empty() {
                switch(p, enabled, Msg::PluginEnable(meta.id.clone()))
            } else {
                Space::new().into()
            };

            cards.push(
                container(
                    column![
                        row![
                            badge,
                            column![tx(title, 13.5, p.txt), tx(meta.description.get(), 12.0, p.mute)]
                                .spacing(3)
                                .width(GROW),
                            switch_el,
                        ]
                        .spacing(12),
                        Space::new().height(GROW),
                        tx(network, 11.5, p.faint),
                        row![tx(status, 12.0, status_color), hspace(), buttons].align_y(Alignment::Center),
                    ]
                    .spacing(8),
                )
                .padding(14)
                .width(GROW)
                .height(160)
                .style(theme::card(p))
                .into(),
            );
        }

        let mut grid = column![].spacing(12);
        let mut it = cards.into_iter();
        loop {
            let mut r = row![].spacing(12);
            let mut n = 0;
            for c in it.by_ref().take(2) {
                r = r.push(c);
                n += 1;
            }
            if n == 0 {
                break;
            }
            for _ in n..2 {
                r = r.push(Space::new().width(GROW));
            }
            grid = grid.push(r);
        }

        let mut col = column![
            row![
                tx(tr("plugins.intro"), 13.0, p.mute).width(GROW),
                button(tx(tr("plugins.open_folder"), 13.0, p.txt))
                    .padding([6, 12])
                    .style(theme::outline(p))
                    .on_press(Msg::PluginsFolder),
                button(tx(tr("plugins.install"), 13.0, p.acc_ink))
                    .padding([6, 12])
                    .style(theme::primary(p))
                    .on_press(Msg::PluginInstall),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            grid,
        ]
        .spacing(14)
        .padding(Padding { top: 16.0, right: 20.0, bottom: 20.0, left: 20.0 });
        for (file, why) in &self.plugins.failed {
            col = col.push(tx(trf("plugins.failed", &[file.clone(), why.clone()]), 12.0, p.err));
        }
        col.into()
    }

    // ---------- plugin settings ----------

    pub fn plugin_dialog_view(&self, p: Palette) -> El<'_> {
        let Some(d) = &self.plugin_dialog else {
            return Space::new().into();
        };
        let Some(plugin) = self.plugins.get(&d.id) else {
            return Space::new().into();
        };
        let meta = &plugin.meta;
        let mut fields = column![].spacing(14);
        let mut has_secret = false;

        for f in &meta.settings {
            let value = d.values.get(&f.key).cloned().unwrap_or_default();
            let label = if f.required { format!("{} *", f.title()) } else { f.title() };
            let help_text = f.help.as_ref().map(|h| h.get()).unwrap_or_default();
            let help: El = if help_text.is_empty() { Space::new().into() } else { tx(help_text.clone(), 12.0, p.mute).into() };
            let key = f.key.clone();
            let control: El = match f.kind {
                FieldKind::Bool => {
                    let on = value == "true";
                    fields = fields.push(self.switch_row(p, label, help_text, on, Msg::PluginFieldToggle(key)));
                    continue;
                }
                FieldKind::Select => {
                    let choices: Vec<Choice> = f
                        .options
                        .iter()
                        .map(|o| Choice { value: o.value.clone(), label: if o.label.is_empty() { o.value.clone() } else { o.label.get() } })
                        .collect();
                    let selected = choices.iter().find(|c| c.value == value).cloned();
                    pick_list(choices, selected, move |c: Choice| Msg::PluginField(key.clone(), c.value))
                        .text_size(fs(13.0))
                        .style(theme::pick(p))
                        .menu_style(theme::pick_menu(p))
                        .width(GROW)
                        .into()
                }
                _ => {
                    has_secret |= f.is_secret();
                    text_input("", &value)
                        .on_input(move |v| Msg::PluginField(key.clone(), v))
                        .secure(f.is_secret())
                        .size(fs(12.0))
                        .font(MONO)
                        .padding([6, 10])
                        .style(theme::input(p))
                        .into()
                }
            };
            fields = fields.push(column![tx(label, 12.0, p.mute), control, help].spacing(6));
        }
        if meta.settings.is_empty() {
            fields = fields.push(tx(tr("plugin.no_settings"), 13.0, p.mute));
        }

        let mut test_row = row![].spacing(10).align_y(Alignment::Center);
        if meta.has_check {
            let btn = button(tx(if d.testing { tr("plugin.testing") } else { tr("plugin.test") }, 13.0, p.txt))
                .padding([6, 12])
                .style(theme::outline(p));
            test_row = test_row.push(if d.testing { btn } else { btn.on_press(Msg::PluginTest) });
            match &d.test {
                Some(Ok(msg)) => {
                    test_row = test_row.push(tx(trf("plugin.test_ok", &[msg.clone().unwrap_or_default()]), 12.0, p.seed))
                }
                Some(Err(e)) => test_row = test_row.push(tx(e.clone(), 12.0, p.err)),
                None => {}
            }
        }

        let mut body = column![scrollable(container(fields).padding(Padding { top: 0.0, right: 16.0, bottom: 0.0, left: 2.0 })).height(Length::Shrink)].spacing(12);
        body = body.push(test_row);
        if has_secret {
            body = body.push(tx(tr("plugin.secret_note"), 12.0, p.mute));
        }
        let title = trf("plugin.settings_title", &[meta.name.clone()]);
        self.dialog_frame(p, title, container(body).max_height(520).into(), tr("plugin.save"), Some(Msg::PluginSave))
    }
}
