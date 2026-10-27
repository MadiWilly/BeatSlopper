//! The preset list the SYNTH panel's browser offers (M9), built from the engine's data.
//!
//! The engine keeps two lists (`engine::presets`): `ALL`, the original eight M2 presets
//! (Bass, Sub, Lead, Pluck, Pad, Keys, Acid, Bell, the "classics"), and `LIBRARY`, the M9
//! library: 91 hand-made categorised presets, then the 1 000 of the Sound Pack. The app
//! addresses presets by one index into the *unified* list: the classics first (so preset 0
//! is still "Bass", the sound of a fresh track, and every index the pre-M9 code and project
//! files used keeps its meaning), then the library in its order. Presets are plain
//! `(ParamId, value)` data: the UI applies them through `Message::SetParam` /
//! `SetTrackParam` (see `AppState::apply_preset`), never by touching the engine directly.
//!
//! The browser filters by [`Category`] (`None` = every category) and by a search query
//! ([`matches`]: case-insensitive substring of the name or any tag, like the engine's
//! `search`); [`filtered`] combines the two and [`step`] walks a filter with wrap-around for
//! the `<` / `>` buttons and the Ctrl+PageUp / Ctrl+PageDown shortcuts.

pub use engine::Preset;
pub use engine::presets::Category;

/// The original eight presets (`engine::presets::ALL`), the first entries of the unified list.
pub use engine::presets::ALL as CLASSIC;
/// The M9 library (`engine::presets::LIBRARY`), the entries after the classics.
pub use engine::presets::LIBRARY;

/// Number of presets in the unified list.
#[must_use]
pub fn count() -> usize {
    CLASSIC.len() + LIBRARY.len()
}

/// Every preset in unified order (the classics, then the library).
pub fn all() -> impl Iterator<Item = &'static Preset> {
    CLASSIC.iter().chain(LIBRARY.iter())
}

/// Preset `index` of the unified list.
#[must_use]
pub fn get(index: usize) -> Option<&'static Preset> {
    if index < CLASSIC.len() {
        CLASSIC.get(index)
    } else {
        LIBRARY.get(index - CLASSIC.len())
    }
}

/// Name of preset `index` (`""` out of range).
#[must_use]
pub fn name(index: usize) -> &'static str {
    get(index).map_or("", |preset| preset.name)
}

/// Index of the preset named `name` (ASCII case-insensitive) in the unified list.
#[must_use]
pub fn index_of(name: &str) -> Option<usize> {
    all().position(|preset| preset.name.eq_ignore_ascii_case(name))
}

/// Whether `preset` matches the search `query`: an ASCII case-insensitive substring of its
/// name or of one of its tags (surrounding whitespace ignored). An empty query matches all.
#[must_use]
pub fn matches(preset: &Preset, query: &str) -> bool {
    let q = query.trim().to_ascii_lowercase();
    q.is_empty()
        || preset.name.to_ascii_lowercase().contains(&q)
        || preset
            .tags
            .iter()
            .any(|tag| tag.to_ascii_lowercase().contains(&q))
}

/// Indices of the presets of `category` (`None` = all of them), in unified order.
pub fn by_category(category: Option<Category>) -> impl Iterator<Item = usize> {
    all()
        .enumerate()
        .filter(move |(_, preset)| category.is_none_or(|c| preset.category == c))
        .map(|(index, _)| index)
}

/// Indices of the presets of `category` that match `query`, in unified order: the
/// browser's list.
#[must_use]
pub fn filtered(category: Option<Category>, query: &str) -> Vec<usize> {
    by_category(category)
        .filter(|&index| get(index).is_some_and(|preset| matches(preset, query)))
        .collect()
}

/// The preset `delta` steps (`-1` previous, `1` next) from `current` within `filter`
/// (unified indices), wrapping at both ends. A `current` outside the filter steps to the
/// filter's first (next) or last (previous) entry. `None` for an empty filter.
#[must_use]
pub fn step(filter: &[usize], current: usize, delta: i32) -> Option<usize> {
    if filter.is_empty() {
        return None;
    }
    let len = filter.len() as i32;
    let position = match filter.iter().position(|&index| index == current) {
        Some(position) => (position as i32 + delta).rem_euclid(len),
        None if delta < 0 => len - 1,
        None => 0,
    };
    Some(filter[position as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_list_is_classics_then_library() {
        assert_eq!(CLASSIC.len(), 8);
        assert_eq!(count(), 8 + engine::presets::LIBRARY_COUNT);
        assert_eq!(name(0), "Bass");
        assert_eq!(name(7), "Bell");
        assert_eq!(name(8), LIBRARY[0].name);
        assert_eq!(name(count() - 1), LIBRARY[LIBRARY.len() - 1].name);
        assert_eq!(name(count()), "");
        assert!(get(count()).is_none());
        assert_eq!(all().count(), count());
        for (i, preset) in all().enumerate() {
            assert_eq!(index_of(preset.name), Some(i), "{}", preset.name);
            assert_eq!(index_of(&preset.name.to_ascii_uppercase()), Some(i));
            assert!(std::ptr::eq(get(i).unwrap(), preset));
        }
        assert_eq!(index_of("No Such Preset"), None);
        assert_eq!(
            index_of("lofi piano"),
            Some(8 + engine::presets::index_of("Lofi Piano").unwrap())
        );
    }

    #[test]
    fn category_counts_match_the_engine_plus_the_classics() {
        let mut total = 0;
        for category in Category::ALL {
            let ours = by_category(Some(category)).count();
            let library = engine::presets::by_category(category).count();
            let classics = CLASSIC.iter().filter(|p| p.category == category).count();
            assert_eq!(ours, library + classics, "{}", category.label());
            assert!(ours >= library);
            total += ours;
        }
        assert_eq!(total, count());
        assert_eq!(by_category(None).count(), count());
        // Unified order inside a category: the classics come first.
        let bass: Vec<usize> = by_category(Some(Category::EdmBass)).collect();
        assert_eq!(bass[0], 0, "Bass");
        assert!(bass.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn search_matches_names_and_tags_like_the_engine() {
        let pianos = filtered(None, "piano");
        let names: Vec<&str> = pianos.iter().map(|&i| name(i)).collect();
        assert!(names.contains(&"Lofi Piano"), "{names:?}");
        assert!(names.contains(&"Toy Piano"), "{names:?}");
        assert_eq!(
            pianos.len(),
            engine::presets::search("piano").len(),
            "no classic is a piano"
        );
        assert_eq!(filtered(None, "  PIANO ").len(), pianos.len());
        assert_eq!(filtered(None, "").len(), count());
        assert_eq!(filtered(None, "no such thing").len(), 0);
        // A tag search inside a category.
        let classic_bass = filtered(Some(Category::EdmBass), "classic");
        assert_eq!(
            classic_bass,
            vec![
                index_of("Bass").unwrap(),
                index_of("Sub").unwrap(),
                index_of("Acid").unwrap()
            ]
        );
        assert!(matches(get(0).unwrap(), "BASS"));
        assert!(!matches(get(0).unwrap(), "piano"));
    }

    #[test]
    fn step_wraps_within_the_filter() {
        let filter = vec![3, 7, 12];
        assert_eq!(step(&filter, 3, 1), Some(7));
        assert_eq!(step(&filter, 12, 1), Some(3), "wraps forward");
        assert_eq!(step(&filter, 3, -1), Some(12), "wraps back");
        assert_eq!(step(&filter, 7, -1), Some(3));
        assert_eq!(step(&filter, 99, 1), Some(3), "outside: the first");
        assert_eq!(step(&filter, 99, -1), Some(12), "outside: the last");
        assert_eq!(step(&[], 0, 1), None);
        assert_eq!(step(&[5], 5, 1), Some(5));
        let everything = filtered(None, "");
        assert_eq!(step(&everything, count() - 1, 1), Some(0));
        assert_eq!(step(&everything, 0, -1), Some(count() - 1));
    }
}
