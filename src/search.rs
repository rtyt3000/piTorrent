//! Tracker search: runs the enabled plugins in parallel and collects their results.

use crate::plugins::{Plugin, SearchResult, Values};
use crate::{App, Msg};
use iced::Task;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(300);
/// Searches hit real trackers, so they are never fired faster than this.
const MIN_INTERVAL: Duration = Duration::from_millis(1500);

#[derive(Clone, Debug, PartialEq)]
pub enum SourceStatus {
    Running,
    Done(usize),
    Failed(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SearchSort {
    Title,
    Size,
    Seeders,
    Leechers,
    Source,
    Date,
}

type CacheKey = (String, String, Option<String>);

pub struct SearchState {
    pub category: Option<String>,
    pub token: u64,
    pub cancel: Arc<AtomicBool>,
    pub sources: Vec<(String, String, SourceStatus)>,
    pub results: Vec<SearchResult>,
    pub sort: (SearchSort, bool),
    pub selected: Option<usize>,
    pub searched: bool,
    last_started: Option<Instant>,
    cache: HashMap<CacheKey, (Instant, Vec<SearchResult>)>,
}

impl Default for SearchState {
    fn default() -> Self {
        SearchState {
            category: None,
            token: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            sources: Vec::new(),
            results: Vec::new(),
            sort: (SearchSort::Seeders, false),
            selected: None,
            searched: false,
            last_started: None,
            cache: HashMap::new(),
        }
    }
}

impl SearchState {
    pub fn running(&self) -> bool {
        self.sources.iter().any(|(_, _, s)| *s == SourceStatus::Running)
    }

    pub fn sort_results(&mut self) {
        let (key, asc) = self.sort;
        self.results.sort_by(|a, b| {
            let ord = match key {
                SearchSort::Title => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
                SearchSort::Size => a.size.cmp(&b.size),
                SearchSort::Seeders => a.seeders.cmp(&b.seeders),
                SearchSort::Leechers => a.leechers.cmp(&b.leechers),
                SearchSort::Source => a.plugin_name.to_lowercase().cmp(&b.plugin_name.to_lowercase()),
                SearchSort::Date => a.date.cmp(&b.date),
            };
            if asc { ord } else { ord.reverse() }
        });
        self.selected = None;
    }

    pub fn set_sort(&mut self, key: SearchSort) {
        self.sort = match self.sort {
            (k, asc) if k == key => (k, !asc),
            // Text columns start ascending, numbers start with the biggest.
            _ => (key, matches!(key, SearchSort::Title | SearchSort::Source)),
        };
        self.sort_results();
    }

    fn finish(&mut self, id: &str, result: Result<Vec<SearchResult>, String>) {
        let Some(source) = self.sources.iter_mut().find(|(i, _, _)| i == id) else {
            return;
        };
        match result {
            Ok(found) => {
                source.2 = SourceStatus::Done(found.len());
                self.results.extend(found);
                self.sort_results();
            }
            Err(e) => source.2 = SourceStatus::Failed(e),
        }
    }

    #[cfg(test)]
    pub fn finish_for_test(&mut self, id: &str, result: Result<Vec<SearchResult>, String>) {
        self.finish(id, result);
    }
}

impl App {
    pub fn start_search(&mut self) -> Task<Msg> {
        let query = self.tracker_query.trim().to_string();
        if query.is_empty() {
            return Task::none();
        }
        let now = Instant::now();
        if self.sr.last_started.is_some_and(|t| now.duration_since(t) < MIN_INTERVAL) {
            self.notice = Some(crate::i18n::tr("search.too_fast"));
            return Task::none();
        }
        self.sr.last_started = Some(now);
        self.sr.cancel.store(true, Ordering::Relaxed);
        self.sr.cancel = Arc::new(AtomicBool::new(false));
        self.sr.token += 1;
        self.sr.results.clear();
        self.sr.selected = None;
        self.sr.searched = true;
        self.sr.sources.clear();
        self.notice = None;

        let token = self.sr.token;
        let category = self.sr.category.clone();
        let mut tasks = Vec::new();
        for plugin in self.plugins.searchable() {
            let id = plugin.meta.id.clone();
            let key = (id.clone(), query.to_lowercase(), category.clone());
            self.sr.cache.retain(|_, (t, _)| now.duration_since(*t) < CACHE_TTL);
            if let Some((_, cached)) = self.sr.cache.get(&key) {
                self.sr.sources.push((id.clone(), plugin.meta.name.clone(), SourceStatus::Running));
                let cached = cached.clone();
                tasks.push(Task::done(Msg::SearchFinished(token, id, Ok(cached))));
                continue;
            }
            self.sr.sources.push((id.clone(), plugin.meta.name.clone(), SourceStatus::Running));
            let settings = self.plugins.settings_of(&plugin);
            tasks.push(run_search(plugin, settings, query.clone(), category.clone(), self.sr.cancel.clone(), token));
        }
        Task::batch(tasks)
    }

    pub fn search_finished(&mut self, token: u64, id: String, result: Result<Vec<SearchResult>, String>) {
        if token != self.sr.token {
            return;
        }
        if let Ok(found) = &result {
            let key = (id.clone(), self.tracker_query.trim().to_lowercase(), self.sr.category.clone());
            self.sr.cache.insert(key, (Instant::now(), found.clone()));
        }
        self.sr.finish(&id, result);
    }

    pub fn stop_search(&mut self) {
        self.sr.cancel.store(true, Ordering::Relaxed);
        for (_, _, status) in &mut self.sr.sources {
            if *status == SourceStatus::Running {
                *status = SourceStatus::Failed(crate::i18n::tr("search.stopped"));
            }
        }
    }
}

fn run_search(
    plugin: Arc<Plugin>,
    settings: Values,
    query: String,
    category: Option<String>,
    cancel: Arc<AtomicBool>,
    token: u64,
) -> Task<Msg> {
    let id = plugin.meta.id.clone();
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || plugin.search(settings, &query, category, cancel))
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map_err(|e| e.0))
        },
        move |r| Msg::SearchFinished(token, id.clone(), r),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(title: &str, seeders: u32, plugin: &str) -> SearchResult {
        SearchResult {
            title: title.into(),
            seeders: Some(seeders),
            size: Some(seeders as u64 * 100),
            plugin_name: plugin.into(),
            magnet: Some("magnet:?xt=urn:btih:aa".into()),
            ..Default::default()
        }
    }

    fn state() -> SearchState {
        let mut s = SearchState::default();
        s.sources = vec![
            ("a".into(), "Alpha".into(), SourceStatus::Running),
            ("b".into(), "Beta".into(), SourceStatus::Running),
        ];
        s
    }

    #[test]
    fn results_from_several_sources_are_merged_and_sorted_by_seeders() {
        let mut s = state();
        s.finish("a", Ok(vec![result("low", 1, "Alpha"), result("high", 50, "Alpha")]));
        assert!(s.running());
        s.finish("b", Ok(vec![result("mid", 10, "Beta")]));
        assert!(!s.running());
        let titles: Vec<_> = s.results.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, ["high", "mid", "low"]);
        assert_eq!(s.sources[0].2, SourceStatus::Done(2));
    }

    #[test]
    fn a_failing_source_does_not_hide_the_others() {
        let mut s = state();
        s.finish("a", Err("Jackett answered 401".into()));
        s.finish("b", Ok(vec![result("x", 5, "Beta")]));
        assert_eq!(s.sources[0].2, SourceStatus::Failed("Jackett answered 401".into()));
        assert_eq!(s.results.len(), 1);
        assert!(!s.running());
    }

    #[test]
    fn clicking_a_column_toggles_direction() {
        let mut s = state();
        s.finish("a", Ok(vec![result("b", 1, "Alpha"), result("a", 2, "Alpha"), result("c", 3, "Alpha")]));
        s.set_sort(SearchSort::Title);
        assert_eq!(s.results[0].title, "a");
        s.set_sort(SearchSort::Title);
        assert_eq!(s.results[0].title, "c");
        s.set_sort(SearchSort::Size);
        assert_eq!(s.results[0].title, "c", "numbers start with the biggest");
    }
}
