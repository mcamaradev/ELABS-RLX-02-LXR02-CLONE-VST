//! Star ratings and tags for kits and patterns, shared by every session: kept in
//! ~/Music/ELABS RLX/favorites.tsv, one line per rated item ("key<TAB>stars<TAB>tag,tag").
//!
//! Items are keyed by name plus a hash of their contents, so two kits that share a name don't
//! share their rating, and a kit keeps its rating wherever it is loaded from.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

/// The tags the built-in kits come with (EntropiaLabs' own), until the user changes them. Keys hold
/// the kits' fingerprints: a test checks they still match kit_table.rs after the kits change.
const BUILT_IN_TAGS: &[(&str, &[&str])] = &[
    ("kit:TEKKNO:8529e1fb", &["808", "Experimental", "Techno"]),
    ("kit:BOOM808:4d98aaa2", &["808"]),
    ("kit:ELECTRO:b73d2940", &["Electro", "Experimental"]),
    ("kit:LOFIDUST:e3cf3cd1", &["Lo-fi"]),
    ("kit:INDUSTRL:4b6ed36d", &["Industrial"]),
    ("kit:MINIMAL:de8d49ec", &["Glitch"]),
    ("kit:NTRPBT1:68941381", &["909", "Industrial"]),
    ("kit:CHAPA:031178ea", &["808"]),
    ("kit:FERRUGEM:0c4e84b7", &["808", "Industrial"]),
    ("kit:CONCRETO:780debe4", &["909"]),
    ("kit:MADRUGA:7b1edc10", &["909"]),
];

fn built_in(key: &str) -> Entry {
    let tags = BUILT_IN_TAGS.iter().find(|(k, _)| *k == key).map_or(&[][..], |(_, t)| *t);
    Entry { stars: 0, tags: tags.iter().map(|t| t.to_string()).collect() }
}

/// Tags offered before the user makes their own.
pub const PRESET_TAGS: [&str; 12] =
    ["Techno", "House", "Electro", "Industrial", "Hip Hop", "808", "909", "Lo-fi", "Acoustic", "Glitch", "Ambient", "Experimental"];

#[derive(Clone, Default, PartialEq, Debug)]
pub struct Entry {
    /// 0 = not rated, 1-5
    pub stars: u8,
    pub tags: BTreeSet<String>,
}

pub struct Favorites {
    entries: HashMap<String, Entry>,
    path: Option<PathBuf>,
}

/// FNV-1a: short, stable fingerprint of a kit's or pattern's bytes.
fn fingerprint(data: &[u8]) -> u32 {
    data.iter().fold(0x811c9dc5u32, |h, &b| (h ^ b as u32).wrapping_mul(0x0100_0193))
}

pub fn kit_key(name: &str, data: &[u8]) -> String {
    format!("kit:{}:{:08x}", name.replace(['\t', '\n'], " "), fingerprint(data))
}

pub fn pattern_key(project: &str, nr: usize, data: &[u8]) -> String {
    format!("pat:{}:{:02}:{:08x}", project.replace(['\t', '\n'], " "), nr + 1, fingerprint(data))
}

/// Tags are single words or phrases without separators.
fn clean_tag(tag: &str) -> Option<String> {
    let t: String = tag.chars().filter(|c| !matches!(c, '\t' | '\n' | ',')).collect();
    let t = t.trim().to_string();
    (!t.is_empty()).then_some(t)
}

impl Favorites {
    pub fn load() -> Self {
        let path = crate::library::root().map(|r| r.join("favorites.tsv"));
        let mut entries = HashMap::new();
        if let Some(text) = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()) {
            for line in text.lines() {
                let mut fields = line.split('\t');
                let (Some(key), Some(stars)) = (fields.next(), fields.next()) else { continue };
                let tags = fields.next().unwrap_or("").split(',').filter_map(clean_tag).collect();
                let stars = stars.parse::<u8>().unwrap_or(0).min(5);
                entries.insert(key.to_string(), Entry { stars, tags });
            }
        }
        Self { entries, path }
    }

    #[cfg(test)]
    fn at(path: PathBuf) -> Self {
        Self { entries: HashMap::new(), path: Some(path) }
    }

    pub fn get(&self, key: &str) -> Entry {
        self.entries.get(key).cloned().unwrap_or_else(|| built_in(key))
    }

    pub fn set_stars(&mut self, key: &str, stars: u8) {
        self.entries.entry(key.to_string()).or_insert_with(|| built_in(key)).stars = stars.min(5);
        self.save();
    }

    pub fn toggle_tag(&mut self, key: &str, tag: &str) {
        let Some(tag) = clean_tag(tag) else { return };
        let tags = &mut self.entries.entry(key.to_string()).or_insert_with(|| built_in(key)).tags;
        if !tags.remove(&tag) {
            tags.insert(tag);
        }
        self.save();
    }

    /// Every tag in use plus the preset ones, sorted.
    pub fn all_tags(&self) -> Vec<String> {
        let mut tags: BTreeSet<String> = PRESET_TAGS.iter().map(|t| t.to_string()).collect();
        tags.extend(self.entries.values().flat_map(|e| e.tags.iter().cloned()));
        tags.extend(BUILT_IN_TAGS.iter().flat_map(|(_, t)| t.iter().map(|t| t.to_string())));
        tags.into_iter().collect()
    }

    fn save(&mut self) {
        // a built-in kit whose tags were all taken off keeps its (empty) line, or they'd come back
        self.entries.retain(|k, e| *e != built_in(k));
        let Some(path) = &self.path else { return };
        let mut keys: Vec<&String> = self.entries.keys().collect();
        keys.sort();
        let mut text = String::new();
        for key in keys {
            let e = &self.entries[key];
            let tags: Vec<&str> = e.tags.iter().map(String::as_str).collect();
            text.push_str(&format!("{key}\t{}\t{}\n", e.stars, tags.join(",")));
        }
        // write a temporary file and rename it over the old one, so a crash can't leave half a file
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = path.with_extension("tsv.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratings_and_tags_survive_a_reload() {
        let dir = std::env::temp_dir().join(format!("lxr-fav-test-{}", std::process::id()));
        let path = dir.join("favorites.tsv");
        let _ = std::fs::remove_dir_all(&dir);
        let kick = kit_key("Badonka", &[1, 2, 3]);
        let other = kit_key("Badonka", &[1, 2, 4]);
        assert_ne!(kick, other, "same name, different sound, different key");

        let mut f = Favorites::at(path.clone());
        f.set_stars(&kick, 4);
        f.toggle_tag(&kick, "Techno");
        f.toggle_tag(&kick, "My, own\ttag");
        f.toggle_tag(&other, "House");
        f.toggle_tag(&other, "House"); // off again: nothing left, so no line

        let mut g = Favorites::at(path.clone());
        let text = std::fs::read_to_string(&path).unwrap();
        for line in text.lines() {
            let mut fields = line.split('\t');
            let key = fields.next().unwrap().to_string();
            let stars = fields.next().unwrap().parse().unwrap();
            let tags = fields.next().unwrap_or("").split(',').filter_map(clean_tag).collect();
            g.entries.insert(key, Entry { stars, tags });
        }
        assert_eq!(g.get(&kick).stars, 4);
        assert_eq!(g.get(&kick).tags.iter().cloned().collect::<Vec<_>>(), vec!["My owntag".to_string(), "Techno".to_string()]);
        assert_eq!(g.get(&other), Entry::default());
        assert_eq!(text.lines().count(), 1);
        assert!(g.all_tags().contains(&"My owntag".to_string()) && g.all_tags().contains(&"909".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn built_in_tags_match_the_built_in_kits() {
        use crate::kit_table::{KITS, KIT_DATA, KIT_SIZE};
        let keys: Vec<String> =
            KITS.iter().enumerate().map(|(i, (_, name))| kit_key(name, &KIT_DATA[i * KIT_SIZE..(i + 1) * KIT_SIZE])).collect();
        for (key, _) in BUILT_IN_TAGS {
            assert!(keys.iter().any(|k| k == key), "{key} is no built-in kit any more: update BUILT_IN_TAGS");
        }

        let dir = std::env::temp_dir().join(format!("lxr-fav-built-in-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut f = Favorites::at(dir.join("favorites.tsv"));
        let tekkno = BUILT_IN_TAGS[0].0;
        assert!(f.get(tekkno).tags.contains("Techno"));
        for tag in ["808", "Experimental", "Techno"] {
            f.toggle_tag(tekkno, tag);
        }
        assert!(f.get(tekkno).tags.is_empty(), "taken off, they stay off");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
