use crate::icons::{Ico, icon};
use crate::theme::{self, Palette};
use iced::widget::{Space, button, column, container, row, text};
use iced::font::{Family, Weight};
use iced::{Color, Element, Font, Length, alignment};

pub const SANS: Font = Font::with_name("Geist");
pub const MONO: Font = Font::with_name("Geist Mono");

/// Global text scale; every font size in the UI goes through `fs`.
pub const UI_SCALE: f32 = 1.15;

pub fn fs(size: f32) -> f32 {
    size * UI_SCALE
}

pub fn tx<'a>(s: impl Into<String>, size: f32, color: Color) -> iced::widget::Text<'a> {
    text(s.into()).size(fs(size)).color(color)
}

pub fn mono<'a>(s: impl Into<String>, size: f32, color: Color) -> iced::widget::Text<'a> {
    tx(s, size, color).font(MONO)
}

pub fn semibold<'a>(s: impl Into<String>, size: f32, color: Color) -> iced::widget::Text<'a> {
    tx(s, size, color).font(Font {
        family: Family::Name("Geist"),
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
}

pub fn build_tree(files: &[(Vec<String>, u64)]) -> Vec<TreeRow> {
    let mut rows: Vec<TreeRow> = Vec::new();
    let mut open: Vec<String> = Vec::new();
    for (i, (comps, size)) in files.iter().enumerate() {
        let Some((name, dir)) = comps.split_last() else {
            continue;
        };
        let common = open.iter().zip(dir).take_while(|(a, b)| a == b).count();
        for (d, part) in dir.iter().enumerate().skip(common) {
            rows.push(TreeRow {
                name: part.clone(),
                depth: d,
                folder: true,
                size: 0,
                idxs: Vec::new(),
            });
        }
        open = dir.to_vec();
        rows.push(TreeRow {
            name: name.clone(),
            depth: dir.len(),
            folder: false,
            size: *size,
            idxs: vec![i],
        });
    }
    for i in 0..rows.len() {
        if !rows[i].folder {
            continue;
        }
        let depth = rows[i].depth;
        let (mut idxs, mut size) = (Vec::new(), 0);
        for r in rows[i + 1..].iter().take_while(|r| r.depth > depth) {
            if !r.folder {
                idxs.extend(&r.idxs);
                size += r.size;
            }
        }
        rows[i].idxs = idxs;
        rows[i].size = size;
    }
    rows
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
