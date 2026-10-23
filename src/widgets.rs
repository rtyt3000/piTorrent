use crate::icons::{Ico, icon};
use crate::theme::{self, Palette};
use iced::widget::{Space, button, column, container, row, text};
use iced::font::{Family, Weight};
use std::sync::OnceLock;
use iced::{Color, Element, Font, Length, alignment};

static SANS_NAME: OnceLock<&'static str> = OnceLock::new();

/// Picks the UI font family; only takes effect when called before the first `sans()`.
pub fn set_sans(name: &str) {
    let _ = SANS_NAME.set(Box::leak(name.to_owned().into_boxed_str()));
}

fn sans_name() -> &'static str {
    SANS_NAME.get().copied().unwrap_or("Geist")
}

pub fn sans() -> Font {
    Font::with_name(sans_name())
}
static MONO_NAME: OnceLock<&'static str> = OnceLock::new();

/// Picks the monospace family; only takes effect when called before the first `mono_font()`.
pub fn set_mono(name: &str) {
    let _ = MONO_NAME.set(Box::leak(name.to_owned().into_boxed_str()));
}

pub fn mono_font() -> Font {
    Font::with_name(MONO_NAME.get().copied().unwrap_or("Geist Mono"))
}

/// Global text scale; every font size in the UI goes through `fs`.
pub const UI_SCALE: f32 = 1.15;

pub fn fs(size: f32) -> f32 {
    size * UI_SCALE
}

pub fn tx<'a>(s: impl Into<String>, size: f32, color: Color) -> iced::widget::Text<'a> {
    text(s.into()).size(fs(size)).color(color)
}

pub fn mono<'a>(s: impl Into<String>, size: f32, color: Color) -> iced::widget::Text<'a> {
    tx(s, size, color).font(mono_font())
}

pub fn semibold<'a>(s: impl Into<String>, size: f32, color: Color) -> iced::widget::Text<'a> {
    tx(s, size, color).font(Font {
        family: Family::Name(sans_name()),
        weight: Weight::Semibold,
        ..Font::DEFAULT
    })
}

pub fn hspace<'a, M: 'a>() -> Element<'a, M> {
    Space::new().width(Length::Fill).into()
}

pub fn switch<'a, M: Clone + 'a>(p: Palette, on: bool, msg: M) -> Element<'a, M> {
    let knob = container(Space::new())
        .width(16)
        .height(16)
        .style(theme::fill(Color::WHITE, 8.0));
    let x = if on {
        alignment::Horizontal::Right
    } else {
        alignment::Horizontal::Left
    };
    button(
        container(knob)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(2)
            .align_x(x),
    )
    .width(36)
    .height(20)
    .padding(0)
    .style(theme::switch_track(p, on))
    .on_press(msg)
    .into()
}

/// `state`: Some(true) = all, Some(false) = none, None = mixed
pub fn checkbox<'a, M: Clone + 'a>(p: Palette, state: Option<bool>, msg: M) -> Element<'a, M> {
    let on = state != Some(false);
    let inner: Element<'a, M> = match state {
        Some(true) => icon(Ico::Check, 11.0, p.acc_ink),
        None => icon(Ico::Dash, 11.0, p.acc_ink),
        Some(false) => Space::new().into(),
    };
    button(
        container(inner)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .width(16)
    .height(16)
    .padding(0)
    .style(theme::checkbox_box(p, on))
    .on_press(msg)
    .into()
}

pub fn segmented<'a, M: Clone + 'a>(
    p: Palette,
    items: Vec<(String, bool, M)>,
) -> Element<'a, M> {
    let buttons = items.into_iter().map(|(label, active, msg)| {
        button(tx(label, 12.5, if active { p.txt } else { p.mute }))
            .padding([5, 14])
            .style(theme::tab(p, active, 7.0))
            .on_press(msg)
            .into()
    });
    container(row(buttons).spacing(2))
        .padding(2)
        .style(theme::input_box(p))
        .into()
}

pub fn section_label<'a, M: 'a>(p: Palette, s: impl AsRef<str>) -> Element<'a, M> {
    tx(s.as_ref().to_uppercase(), 11.0, p.faint).into()
}

pub fn kv<'a, M: 'a>(p: Palette, k: &str, v: String) -> Element<'a, M> {
    column![section_label(p, k), tx(v, 13.0, p.txt)]
        .spacing(3)
        .width(Length::Fill)
        .into()
}

pub fn hline<'a, M: 'a>(p: Palette) -> Element<'a, M> {
    container(Space::new())
        .width(Length::Fill)
        .height(1)
        .style(theme::fill(p.line, 0.0))
        .into()
}

#[derive(Clone, Debug)]
pub struct TreeRow {
    pub name: String,
    pub depth: usize,
    pub folder: bool,
    pub size: u64,
    pub idxs: Vec<usize>,
    /// Folder path joined with `/`; the key of its open/closed state.
    pub path: String,
}

/// Files of one folder can be scattered through the torrent's file list, so the tree is built
/// by path (folders in order of first appearance) rather than from neighbouring entries.
pub fn build_tree(files: &[(Vec<String>, u64)]) -> Vec<TreeRow> {
    struct Node {
        name: String,
        folder: bool,
        size: u64,
        idxs: Vec<usize>,
        kids: Vec<Node>,
    }
    let node = |name: String, folder: bool| Node { name, folder, size: 0, idxs: Vec::new(), kids: Vec::new() };
    let mut root = node(String::new(), true);
    for (i, (comps, size)) in files.iter().enumerate() {
        let Some((name, dir)) = comps.split_last() else {
            continue;
        };
        let mut cur = &mut root;
        for part in dir {
            cur.size += size;
            cur.idxs.push(i);
            let pos = match cur.kids.iter().position(|k| k.folder && k.name == *part) {
                Some(pos) => pos,
                None => {
                    cur.kids.push(node(part.clone(), true));
                    cur.kids.len() - 1
                }
            };
            cur = &mut cur.kids[pos];
        }
        cur.size += size;
        cur.idxs.push(i);
        cur.kids.push(Node { size: *size, idxs: vec![i], ..node(name.clone(), false) });
    }

    fn flatten(n: &Node, depth: usize, prefix: &str, out: &mut Vec<TreeRow>) {
        for k in &n.kids {
            let path = if prefix.is_empty() { k.name.clone() } else { format!("{prefix}/{}", k.name) };
            out.push(TreeRow {
                name: k.name.clone(),
                depth,
                folder: k.folder,
                size: k.size,
                idxs: k.idxs.clone(),
                path: path.clone(),
            });
            if k.folder {
                flatten(k, depth + 1, &path, out);
            }
        }
    }
    let mut rows = Vec::new();
    flatten(&root, 0, "", &mut rows);
    rows
}

/// The rows that show: everything inside a closed folder is hidden.
pub fn visible<'a>(rows: &'a [TreeRow], open: &std::collections::HashSet<String>) -> Vec<&'a TreeRow> {
    let mut out = Vec::new();
    let mut hidden_below: Option<usize> = None;
    for r in rows {
        if let Some(d) = hidden_below {
            if r.depth > d {
                continue;
            }
            hidden_below = None;
        }
        out.push(r);
        if r.folder && !open.contains(&r.path) {
            hidden_below = Some(r.depth);
        }
    }
    out
}

pub fn tri_state(idxs: &[usize], is_on: impl Fn(usize) -> bool) -> Option<bool> {
    let n = idxs.iter().filter(|&&i| is_on(i)).count();
    if n == idxs.len() {
        Some(true)
    } else if n == 0 {
        Some(false)
    } else {
        None
    }
}
