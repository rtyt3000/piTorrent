//! Global input state (cursor, modifiers, window size) kept outside of the message loop so
//! that mouse moves do not trigger a re-render, plus the keyboard shortcuts.

use crate::Msg;
use iced::event::Status;
use iced::keyboard::{self, Key, key::{Code, Named, Physical}};
use iced::{Event, Point, Size, mouse, window};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};

static CURSOR_X: AtomicU32 = AtomicU32::new(0);
static CURSOR_Y: AtomicU32 = AtomicU32::new(0);
static WIN_W: AtomicU32 = AtomicU32::new(1280f32.to_bits());
static WIN_H: AtomicU32 = AtomicU32::new(820f32.to_bits());
static CTRL: AtomicBool = AtomicBool::new(false);
static SHIFT: AtomicBool = AtomicBool::new(false);

pub fn cursor() -> Point {
    Point::new(f32::from_bits(CURSOR_X.load(Relaxed)), f32::from_bits(CURSOR_Y.load(Relaxed)))
}

pub fn window_size() -> Size {
    Size::new(f32::from_bits(WIN_W.load(Relaxed)), f32::from_bits(WIN_H.load(Relaxed)))
}

pub fn ctrl() -> bool {
    CTRL.load(Relaxed)
}

pub fn shift() -> bool {
    SHIFT.load(Relaxed)
}

fn store_modifiers(m: keyboard::Modifiers) {
    CTRL.store(m.control() || m.command(), Relaxed);
    SHIFT.store(m.shift(), Relaxed);
}

pub fn on_event(event: Event, status: Status, _: window::Id) -> Option<Msg> {
    match event {
        Event::Mouse(mouse::Event::CursorMoved { position }) => {
            CURSOR_X.store(position.x.to_bits(), Relaxed);
            CURSOR_Y.store(position.y.to_bits(), Relaxed);
            None
        }
        Event::Window(window::Event::Resized(size)) => {
            WIN_W.store(size.width.to_bits(), Relaxed);
            WIN_H.store(size.height.to_bits(), Relaxed);
            None
        }
        Event::Window(window::Event::FileDropped(p)) => Some(Msg::FileDropped(p)),
        Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => {
            store_modifiers(m);
            None
        }
        Event::Keyboard(keyboard::Event::KeyPressed { key, physical_key, modifiers, .. }) => {
            store_modifiers(modifiers);
            if status == Status::Captured {
                return None;
            }
            // Letter shortcuts use the physical key, so they work on any keyboard layout.
            let code = match physical_key {
                Physical::Code(code) => Some(code),
                Physical::Unidentified(_) => None,
            };
            match (key, code) {
                (Key::Named(Named::Delete), _) => Some(Msg::AskDelete),
                (Key::Named(Named::Escape), _) => Some(Msg::Escape),
                (_, Some(Code::KeyA)) if ctrl() => Some(Msg::SelectAll),
                (_, Some(Code::KeyV)) if ctrl() => Some(Msg::PasteShortcut),
                _ => None,
            }
        }
        _ => None,
    }
}
