//! The RECENT projects list (QOL pass): the last [`RECENT_CAP`] project files opened or
//! saved, most recent first, persisted in eframe storage under [`RECENT_KEY`].
//!
//! [`RecentList::push`] moves a path to the front (no duplicates) and drops the oldest past
//! the cap; [`RecentList::next_after`] gives the entry to cycle to with Ctrl+Shift+O: the one
//! after the current project (wrapping), or the most recent when the current project is not
//! in the list; [`RecentList::retain_existing`] forgets files that no longer exist.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Most entries kept.
pub const RECENT_CAP: usize = 8;
/// eframe storage key.
pub const RECENT_KEY: &str = "recent_projects";

/// The recent project files, most recent first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentList {
    paths: Vec<PathBuf>,
}

impl RecentList {
    /// An empty list.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A list from stored paths (deduplicated, capped).
    #[must_use]
    pub fn from_paths(paths: impl IntoIterator<Item = PathBuf>) -> Self {
        let mut list = Self::new();
        for path in paths.into_iter().collect::<Vec<_>>().into_iter().rev() {
            list.push(path);
        }
        list
    }

    /// The entries, most recent first.
    #[must_use]
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.paths.len()
    }

    /// `true` when nothing has been opened or saved yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Puts `path` at the front (removing an older copy) and drops entries past the cap.
    pub fn push(&mut self, path: PathBuf) {
        self.paths.retain(|p| *p != path);
        self.paths.insert(0, path);
        self.paths.truncate(RECENT_CAP);
    }

    /// Removes `path`, if listed.
    pub fn remove(&mut self, path: &Path) {
        self.paths.retain(|p| p != path);
    }

    /// Forgets every entry.
    pub fn clear(&mut self) {
        self.paths.clear();
    }

    /// Keeps only the entries whose file still exists.
    pub fn retain_existing(&mut self) {
        self.paths.retain(|p| p.is_file());
    }

    /// The entry to cycle to from `current`: the one after it (wrapping to the front), or
    /// the most recent entry when `current` is `None` or not listed. `None` for an empty list
    /// or when the only entry is the current one.
    #[must_use]
    pub fn next_after(&self, current: Option<&Path>) -> Option<&Path> {
        if self.paths.is_empty() {
            return None;
        }
        let position = current.and_then(|c| self.paths.iter().position(|p| p == c));
        match position {
            Some(i) => {
                if self.paths.len() == 1 {
                    None
                } else {
                    Some(&self.paths[(i + 1) % self.paths.len()])
                }
            }
            None => Some(&self.paths[0]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str) -> PathBuf {
        PathBuf::from(format!("C:/songs/{name}.beatslopper.json"))
    }

    #[test]
    fn push_moves_to_the_front_without_duplicates_and_caps_at_eight() {
        let mut list = RecentList::new();
        for i in 0..10 {
            list.push(p(&format!("beat{i}")));
        }
        assert_eq!(list.len(), RECENT_CAP);
        assert_eq!(list.paths()[0], p("beat9"));
        assert_eq!(list.paths()[7], p("beat2"), "the oldest two fell off");

        list.push(p("beat5"));
        assert_eq!(list.len(), RECENT_CAP);
        assert_eq!(list.paths()[0], p("beat5"));
        assert_eq!(list.paths().iter().filter(|x| **x == p("beat5")).count(), 1);
        assert_eq!(list.paths()[1], p("beat9"));
    }

    #[test]
    fn next_after_cycles_and_wraps() {
        let mut list = RecentList::new();
        list.push(p("c"));
        list.push(p("b"));
        list.push(p("a")); // order: a b c
        assert_eq!(list.next_after(None), Some(p("a").as_path()));
        assert_eq!(list.next_after(Some(&p("a"))), Some(p("b").as_path()));
        assert_eq!(
            list.next_after(Some(&p("c"))),
            Some(p("a").as_path()),
            "wraps"
        );
        assert_eq!(
            list.next_after(Some(&p("unknown"))),
            Some(p("a").as_path()),
            "not listed: the most recent"
        );
        let mut one = RecentList::new();
        one.push(p("only"));
        assert_eq!(one.next_after(Some(&p("only"))), None);
        assert_eq!(one.next_after(None), Some(p("only").as_path()));
        assert_eq!(RecentList::new().next_after(None), None);
    }

    #[test]
    fn remove_clear_and_from_paths() {
        let list = RecentList::from_paths([p("a"), p("b"), p("a"), p("c")]);
        assert_eq!(list.paths(), &[p("a"), p("b"), p("c")]);
        let mut list = list;
        list.remove(&p("b"));
        assert_eq!(list.paths(), &[p("a"), p("c")]);
        list.clear();
        assert!(list.is_empty());
    }

    #[test]
    fn retain_existing_drops_missing_files() {
        let dir = std::env::temp_dir().join(format!("beatslopper-recent-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let real = dir.join("real.beatslopper.json");
        std::fs::write(&real, "{}").unwrap();
        let mut list = RecentList::new();
        list.push(dir.join("gone.beatslopper.json"));
        list.push(real.clone());
        list.retain_existing();
        assert_eq!(list.paths(), std::slice::from_ref(&real));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn round_trips_through_serde() {
        let list = RecentList::from_paths([p("a"), p("b")]);
        let json = serde_json::to_string(&list).unwrap();
        let back: RecentList = serde_json::from_str(&json).unwrap();
        assert_eq!(back, list);
    }
}
