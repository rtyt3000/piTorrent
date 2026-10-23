
use iced::overlay::menu;
use iced::widget::{button, container, pick_list, progress_bar, text_editor, text_input};
use iced::{Background, Border, Color, Shadow, Theme};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Mode {
    Dark,
    Light,
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color,
    pub isl: Color,
    pub ibd: Color,
    pub elev: Color,
    pub line: Color,
    pub bd: Color,
    pub hov: Color,
    pub act: Color,
    pub sel: Color,
    pub txt: Color,
    pub txt2: Color,
    pub mute: Color,
    pub faint: Color,
    pub acc: Color,
    pub acc_ink: Color,
    pub acc_hov: Color,
    pub dl: Color,
    pub seed: Color,
    pub pause: Color,
    pub err: Color,
    pub track: Color,
    pub input: Color,
    pub off: Color,
}

const fn hex(v: u32) -> Color {
    Color::from_rgb(
        ((v >> 16) & 0xFF) as f32 / 255.0,
        ((v >> 8) & 0xFF) as f32 / 255.0,
        (v & 0xFF) as f32 / 255.0,
    )
}

const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color { r, g, b, a }
}

impl Palette {
    pub fn of(mode: Mode) -> Self {
        match mode {
            Mode::Dark => Self {
                bg: hex(0x1F2023),
                isl: hex(0x191A1C),
                ibd: Color::TRANSPARENT,
                elev: hex(0x26282C),
                line: hex(0x303236),
                bd: hex(0x4E5157),
                hov: rgba(1.0, 1.0, 1.0, 0.05),
                act: hex(0x33353B),
                sel: hex(0x33353B),
                txt: hex(0xDFE1E5),
                txt2: hex(0xCED0D6),
                mute: hex(0x8E9299),
                faint: hex(0x8E9299),
                acc: hex(0x3871E1),
                acc_ink: hex(0xFFFFFF),
                acc_hov: hex(0x4A85F5),
                dl: hex(0x548AF7),
                seed: hex(0x5FB865),
                pause: hex(0x868A91),
                err: hex(0xE55765),
                track: hex(0x43454A),
                input: hex(0x191A1C),
                off: hex(0x5A5D63),
            },
            Mode::Light => Self {
                bg: hex(0xF3F4F7),
                isl: hex(0xFFFFFF),
                ibd: hex(0xE6E8EC),
                elev: hex(0xF7F8FA),
                line: hex(0xEBECF0),
                bd: hex(0xC9CCD6),
                hov: rgba(0.0, 0.0, 0.0, 0.05),
                act: rgba(0.0, 0.0, 0.0, 0.08),
                sel: hex(0xE4E6EB),
                txt: hex(0x1E1F22),
                txt2: hex(0x383A42),
                mute: hex(0x5C6070),
                faint: hex(0x6C707E),
                acc: hex(0x3068E0),
                acc_ink: hex(0xFFFFFF),
                acc_hov: hex(0x2556C0),
                dl: hex(0x2E62D6),
                seed: hex(0x1F8A3B),
                pause: hex(0x8C909A),
                err: hex(0xC7222D),
                track: hex(0xDFE1E5),
                input: hex(0xFFFFFF),
                off: hex(0xA8ABB4),
            },
        }
    }

    /// Overrides one colour by its field name; returns false for an unknown name.
    pub fn set(&mut self, key: &str, c: Color) -> bool {
        let slot = match key {
            "bg" => &mut self.bg,
            "isl" => &mut self.isl,
            "ibd" => &mut self.ibd,
            "elev" => &mut self.elev,
            "line" => &mut self.line,
            "bd" => &mut self.bd,
            "hov" => &mut self.hov,
            "act" => &mut self.act,
            "sel" => &mut self.sel,
            "txt" => &mut self.txt,
            "txt2" => &mut self.txt2,
            "mute" => &mut self.mute,
            "faint" => &mut self.faint,
            "acc" => &mut self.acc,
            "acc_ink" => &mut self.acc_ink,
            "acc_hov" => &mut self.acc_hov,
            "dl" => &mut self.dl,
            "seed" => &mut self.seed,
            "pause" => &mut self.pause,
            "err" => &mut self.err,
            "track" => &mut self.track,
            "input" => &mut self.input,
            "off" => &mut self.off,
            _ => return false,
        };
        *slot = c;
        true
    }

    /// Makes the big surfaces see-through; text, accents and borders stay opaque.
    pub fn translucent(mut self, alpha: f32) -> Self {
        for c in [&mut self.bg, &mut self.isl, &mut self.elev, &mut self.input] {
            c.a *= alpha;
        }
        self
    }

    pub fn iced_theme(&self) -> Theme {
        Theme::custom(
            "piTorrent".to_string(),
            iced::theme::Palette {
                background: self.bg,
                text: self.txt,
                primary: self.acc,
                success: self.seed,
                warning: self.pause,
                danger: self.err,
            },
        )
    }
}

static RADIUS_SCALE: AtomicU32 = AtomicU32::new(0x3F80_0000); // 1.0f32

/// Scales every corner radius of the interface (a theme's "roundness").
pub fn set_radius_scale(scale: f32) {
    RADIUS_SCALE.store(scale.clamp(0.0, 2.0).to_bits(), Ordering::Relaxed);
}

fn border(color: Color, width: f32, radius: f32) -> Border {
    exact_border(color, width, radius * f32::from_bits(RADIUS_SCALE.load(Ordering::Relaxed)))
}

/// For pills and dots, whose shape must not change with the theme's roundness.
fn exact_border(color: Color, width: f32, radius: f32) -> Border {
    Border {
        color,
        width,
        radius: radius.into(),
    }
}


pub fn page(p: Palette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(p.bg)),
        text_color: Some(p.txt),
        ..Default::default()
    }
}

pub fn island(p: Palette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(p.isl)),
        border: border(p.ibd, 1.0, 14.0),
        ..Default::default()
    }
}

pub fn card(p: Palette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(p.elev)),
        border: border(Color::TRANSPARENT, 0.0, 12.0),
        ..Default::default()
    }
}

pub fn dialog(p: Palette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(p.isl)),
        border: border(p.bd, 1.0, 14.0),
        shadow: Shadow {
            color: rgba(0.0, 0.0, 0.0, 0.45),
            offset: iced::Vector::new(0.0, 24.0),
            blur_radius: 64.0,
        },
        ..Default::default()
    }
}

pub fn menu(p: Palette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(p.elev)),
        border: border(p.bd, 1.0, 10.0),
        shadow: Shadow {
            color: rgba(0.0, 0.0, 0.0, 0.35),
            offset: iced::Vector::new(0.0, 12.0),
            blur_radius: 32.0,
        },
        ..Default::default()
    }
}

pub fn input_box(p: Palette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(p.input)),
        border: border(p.bd, 1.0, 10.0),
        ..Default::default()
    }
}

pub fn fill(color: Color, radius: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(color)),
        border: exact_border(Color::TRANSPARENT, 0.0, radius),
        ..Default::default()
    }
}

pub fn badge(p: Palette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(p.track)),
        text_color: Some(p.txt),
        border: border(Color::TRANSPARENT, 0.0, 7.0),
        ..Default::default()
    }
}

/// `t` is how far the dialog has faded in (0..=1).
pub fn scrim(t: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(rgba(0.0, 0.0, 0.0, 0.5 * t))),
        ..Default::default()
    }
}


fn btn(bg: Option<Color>, fg: Color, b: Border) -> button::Style {
    button::Style {
        background: bg.map(Background::Color),
        text_color: fg,
        border: b,
        shadow: Shadow::default(),
        snap: true,
    }
}

fn hovered(st: button::Status) -> bool {
    matches!(st, button::Status::Hovered | button::Status::Pressed)
}

pub fn ghost(p: Palette, radius: f32) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, st| {
        let bg = hovered(st).then_some(p.hov);
        btn(bg, p.txt2, border(Color::TRANSPARENT, 0.0, radius))
    }
}

pub fn tab(p: Palette, active: bool, radius: f32) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, st| {
        let bg = if active {
            Some(p.act)
        } else {
            hovered(st).then_some(p.hov)
        };
        let fg = if active { p.txt } else { p.mute };
        btn(bg, fg, border(Color::TRANSPARENT, 0.0, radius))
    }
}

pub fn window_close(p: Palette) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, st| {
        let bg = hovered(st).then_some(p.err);
        btn(bg, p.txt2, border(Color::TRANSPARENT, 0.0, 0.0))
    }
}

pub fn primary(p: Palette) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, st| {
        let bg = match st {
            button::Status::Hovered | button::Status::Pressed => p.acc_hov,
            button::Status::Disabled => p.off,
            _ => p.acc,
        };
        btn(Some(bg), p.acc_ink, border(Color::TRANSPARENT, 0.0, 8.0))
    }
}

pub fn outline(p: Palette) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, st| btn(hovered(st).then_some(p.hov), p.txt, border(p.bd, 1.0, 8.0))
}

pub fn dropzone(p: Palette) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, st| btn(hovered(st).then_some(p.hov), p.txt2, border(p.bd, 1.0, 12.0))
}

pub fn link(p: Palette) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, st| btn(hovered(st).then_some(p.hov), p.dl, border(Color::TRANSPARENT, 0.0, 6.0))
}

pub fn row(p: Palette, selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, st| {
        let bg = if selected {
            Some(p.sel)
        } else {
            matches!(st, button::Status::Hovered).then_some(p.hov)
        };
        btn(bg, p.txt, border(Color::TRANSPARENT, 0.0, 9.0))
    }
}

pub fn switch_track(p: Palette, on: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, _| {
        btn(
            Some(if on { p.acc } else { p.off }),
            p.txt,
            exact_border(Color::TRANSPARENT, 0.0, 10.0),
        )
    }
}

pub fn checkbox_box(p: Palette, on: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, _| {
        btn(
            on.then_some(p.acc),
            p.acc_ink,
            border(if on { p.acc } else { p.off }, 1.0, 4.0),
        )
    }
}


pub fn input(p: Palette) -> impl Fn(&Theme, text_input::Status) -> text_input::Style {
    move |_, st| {
        let bd = match st {
            text_input::Status::Focused { .. } => p.dl,
            _ => p.bd,
        };
        text_input::Style {
            background: Background::Color(p.input),
            border: border(bd, 1.0, 8.0),
            icon: p.faint,
            placeholder: p.faint,
            value: p.txt,
            selection: Color { a: 0.35, ..p.dl },
        }
    }
}

pub fn editor(p: Palette) -> impl Fn(&Theme, text_editor::Status) -> text_editor::Style {
    move |_, st| {
        let bd = match st {
            text_editor::Status::Focused { .. } => p.dl,
            _ => p.bd,
        };
        text_editor::Style {
            background: Background::Color(p.input),
            border: border(bd, 1.0, 8.0),
            placeholder: p.faint,
            value: p.txt,
            selection: Color { a: 0.35, ..p.dl },
        }
    }
}

pub fn bar(p: Palette, color: Color) -> impl Fn(&Theme) -> progress_bar::Style {
    move |_| progress_bar::Style {
        background: Background::Color(p.track),
        bar: Background::Color(color),
        border: exact_border(Color::TRANSPARENT, 0.0, 3.0),
    }
}

pub fn pick(p: Palette) -> impl Fn(&Theme, pick_list::Status) -> pick_list::Style {
    move |_, st| {
        let bd = match st {
            pick_list::Status::Opened { .. } | pick_list::Status::Hovered => p.dl,
            _ => p.bd,
        };
        pick_list::Style {
            text_color: p.txt,
            placeholder_color: p.faint,
            handle_color: p.mute,
            background: Background::Color(p.input),
            border: border(bd, 1.0, 8.0),
        }
    }
}

pub fn pick_menu(p: Palette) -> impl Fn(&Theme) -> menu::Style {
    move |_| menu::Style {
        background: Background::Color(p.elev),
        border: border(p.bd, 1.0, 8.0),
        text_color: p.txt,
        selected_text_color: p.txt,
        selected_background: Background::Color(p.act),
        shadow: Shadow {
            color: rgba(0.0, 0.0, 0.0, 0.35),
            offset: iced::Vector::new(0.0, 8.0),
            blur_radius: 24.0,
        },
    }
}
