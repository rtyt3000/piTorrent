use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Lang {
    Ru,
    En,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Language {
    Auto,
    Ru,
    En,
}

impl Language {
    pub const ALL: [Language; 3] = [Language::Auto, Language::Ru, Language::En];

    pub fn resolve(self) -> Lang {
        match self {
            Language::Ru => Lang::Ru,
            Language::En => Lang::En,
            Language::Auto => match sys_locale::get_locale() {
                Some(l) if l.to_lowercase().starts_with("ru") => Lang::Ru,
                _ => Lang::En,
            },
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Language::Auto => f.write_str(&tr("settings.language.auto")),
            Language::Ru => f.write_str("Русский"),
            Language::En => f.write_str("English"),
        }
    }
}

type Table = HashMap<String, String>;

static CURRENT: AtomicU8 = AtomicU8::new(1);

fn table(lang: Lang) -> &'static Table {
    static RU: OnceLock<Table> = OnceLock::new();
    static EN: OnceLock<Table> = OnceLock::new();
    let (cell, src) = match lang {
        Lang::Ru => (&RU, include_str!("../assets/locales/ru.json")),
        Lang::En => (&EN, include_str!("../assets/locales/en.json")),
    };
    cell.get_or_init(|| serde_json::from_str(src).expect("invalid locale file"))
}

pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == 0 { Lang::Ru } else { Lang::En }
}

pub fn tr(key: &str) -> String {
    table(current())
        .get(key)
        .or_else(|| table(Lang::En).get(key))
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

/// Translates `key` and fills each `{}` with the next argument.
pub fn trf(key: &str, args: &[String]) -> String {
    let mut args = args.iter();
    tr(key)
        .split("{}")
        .enumerate()
        .fold(String::new(), |mut out, (i, part)| {
            if i > 0 {
                out.push_str(args.next().map(String::as_str).unwrap_or(""));
            }
            out.push_str(part);
            out
        })
}

/// "5 peers" / "5 пиров", using the `plural.<noun>.{one,few,many}` keys.
pub fn plural(n: usize, noun: &str) -> String {
    let form = match current() {
        Lang::Ru => match (n % 100, n % 10) {
            (11..=14, _) => "many",
            (_, 1) => "one",
            (_, 2..=4) => "few",
            _ => "many",
        },
        Lang::En => if n == 1 { "one" } else { "many" },
    };
    format!("{n} {}", tr(&format!("plural.{noun}.{form}")))
}
