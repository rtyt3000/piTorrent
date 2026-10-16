use crate::i18n::tr;

pub fn bytes(n: u64) -> String {
    const KB: f64 = 1024.0;
    let f = n as f64;
    if f >= KB * KB * KB {
        format!("{:.1} {}", f / (KB * KB * KB), tr("unit.gb"))
    } else if f >= KB * KB {
        format!("{:.0} {}", f / (KB * KB), tr("unit.mb"))
    } else if f >= KB {
        format!("{:.0} {}", f / KB, tr("unit.kb"))
    } else {
        format!("{n} {}", tr("unit.b"))
    }
}

pub fn speed(bps: u64) -> String {
    if bps == 0 {
        return "—".into();
    }
    let f = bps as f64;
    if f >= 1024.0 * 1024.0 {
        format!("{:.1} {}/{}", f / 1024.0 / 1024.0, tr("unit.mb"), tr("unit.s"))
    } else {
        format!("{:.0} {}/{}", f / 1024.0, tr("unit.kb"), tr("unit.s"))
    }
}

pub fn eta(secs: Option<u64>) -> String {
    match secs {
        None => "—".into(),
        Some(s) if s < 60 => format!("{s} {}", tr("unit.sec")),
        Some(s) if s < 3600 => format!("{} {}", s / 60, tr("unit.min")),
        Some(s) if s < 86400 => {
            format!("{} {} {} {}", s / 3600, tr("unit.hour"), s % 3600 / 60, tr("unit.min"))
        }
        Some(s) => format!("{} {}", s / 86400, tr("unit.day")),
    }
}

pub fn percent(done: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (done as f64 / total as f64 * 100.0).min(100.0) as f32
    }
}
