//! Interface animations. iced redraws only when something changes, so everything that moves is
//! driven from here: `App` asks for frames only while `animating()`, and the views read the
//! current values, which are computed from the real time at every draw.

use crate::engine::TorrentInfo;
use iced::animation::{Animation, Easing};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const DIALOG: Duration = Duration::from_millis(240);
/// Matches the polling interval, so a bar is always moving towards the next sample.
const BAR: Duration = Duration::from_millis(1000);
const PAGE: Duration = Duration::from_millis(320);
const TAB: Duration = Duration::from_millis(260);
const PULSE_PERIOD: f32 = 1.4;

pub struct Motion {
    epoch: Instant,
    dialog: Animation<bool>,
    bars: HashMap<usize, Animation<f32>>,
    page: Option<Instant>,
    tab: Option<Instant>,
    /// Nothing is drawn while the window sits in the tray, so nothing needs to animate.
    pub hidden: bool,
}

fn dialog_animation() -> Animation<bool> {
    Animation::new(false).easing(Easing::EaseOutCubic).duration(DIALOG)
}

impl Motion {
    pub fn new() -> Self {
        let now = Instant::now();
        Motion { epoch: now, dialog: dialog_animation(), bars: HashMap::new(), page: None, tab: None, hidden: false }
    }

    pub fn dialog_opened(&mut self) {
        self.dialog = dialog_animation();
        self.dialog.go_mut(true, Instant::now());
    }

    /// The main view or the list filter changed.
    pub fn page_changed(&mut self) {
        self.page = Some(Instant::now());
    }

    /// A tab of the details panel or of the settings changed.
    pub fn tab_changed(&mut self) {
        self.tab = Some(Instant::now());
    }

    /// 0 right after a page change, 1 once it has settled.
    pub fn page_t(&self) -> f32 {
        progress(self.page, PAGE)
    }

    pub fn tab_t(&self) -> f32 {
        progress(self.tab, TAB)
    }

    pub fn dialog_closed(&mut self) {
        self.dialog = dialog_animation();
    }

    /// 0 when a dialog has just opened, 1 once it has settled.
    pub fn dialog_t(&self) -> f32 {
        self.dialog.interpolate(0.0, 1.0, Instant::now())
    }

    /// Remembers the new progress of every torrent and lets the bars glide towards it.
    pub fn snapshot(&mut self, torrents: &[TorrentInfo]) {
        let now = Instant::now();
        self.bars.retain(|id, _| torrents.iter().any(|t| t.id == *id));
        for t in torrents {
            let pct = crate::fmt::percent(t.progress, t.total);
            match self.bars.get_mut(&t.id) {
                Some(bar) if bar.value() != pct => bar.go_mut(pct, now),
                Some(_) => {}
                None => {
                    self.bars.insert(t.id, Animation::new(pct).easing(Easing::Linear).duration(BAR));
                }
            }
        }
    }

    /// The percentage to draw for a torrent.
    pub fn bar(&self, id: usize, pct: f32) -> f32 {
        self.bars.get(&id).map_or(pct, |a| a.interpolate_with(|v| v, Instant::now()))
    }

    /// A slow 0..1 wave for "working on it" indicators.
    pub fn pulse(&self) -> f32 {
        let t = self.epoch.elapsed().as_secs_f32();
        0.5 + 0.5 * (t * std::f32::consts::TAU / PULSE_PERIOD).sin()
    }

    pub fn animating(&self, dialog_open: bool, searching: bool) -> bool {
        if self.hidden {
            return false;
        }
        let now = Instant::now();
        searching || self.page_t() < 1.0 || self.tab_t() < 1.0 || (dialog_open && self.dialog.is_animating(now)) || self.bars.values().any(|b| b.is_animating(now))
    }
}

fn progress(since: Option<Instant>, duration: Duration) -> f32 {
    let x = since.map_or(1.0, |s| (s.elapsed().as_secs_f32() / duration.as_secs_f32()).min(1.0));
    // Smooth at both ends: an abrupt start reads as a flicker.
    0.5 - 0.5 * (x * std::f32::consts::PI).cos()
}
