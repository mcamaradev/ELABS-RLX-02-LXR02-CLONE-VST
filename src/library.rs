//! The user's library folder, ~/Music/ELABS RLX, laid out like the LXR-02's SD card:
//! - kits: .SND files anywhere in it (8 byte name + the parameter values); each folder is a bank
//! - projects: folders with .PAT files (NN-NAME.PAT) and the kits they use (NN-NAME.SND)
//!
//! Unzipped LXR-02 downloads (factory SD content, sound packs) can be dropped in as they are, or
//! imported from their zip or folder (`import`), which copies their kits and patterns to
//! Imported/NAME. Kits named INIT/EMPTY and second copies of a kit are not listed.
//! Kits and projects saved from the plugin go to Kits/ and Projects/ in the same format, so they
//! also load on the hardware.

use std::path::{Path, PathBuf};

use crate::kit_table::KIT_SIZE;
use crate::seq::{self, Project, NUM_PATTERNS, PATTERN_BYTES};

pub fn root() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| Path::new(&h).join("Music/ELABS RLX"))
}

pub fn kits_dir() -> Option<PathBuf> {
    root().map(|r| r.join("Kits"))
}

pub fn projects_dir() -> Option<PathBuf> {
    root().map(|r| r.join("Projects"))
}

pub fn imported_dir() -> Option<PathBuf> {
    root().map(|r| r.join("Imported"))
}

/// What an import brought in.
pub struct Imported {
    pub name: String,
    pub kits: usize,
    pub patterns: usize,
}

/// The name an import is listed under: Erica Synths' downloads by their usual names, anything
/// else by its file or folder name.
fn import_name(source: &Path) -> String {
    let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let lower = stem.to_lowercase();
    let name = match () {
        _ if lower.contains("factory") => "FACTORY".to_string(),
        _ if lower.contains("untz") => "UNTZKITS".to_string(),
        _ if lower.contains("dlc") => "HRTL DLC".to_string(),
        _ => stem.replace('_', " ").trim().to_uppercase().chars().take(20).collect(),
    };
    if name.is_empty() { "IMPORT".to_string() } else { name }
}

/// Copies the kits (.SND) and patterns (.PAT) of an LXR-02 download, a zip or a folder, into
/// Imported/NAME, keeping its folders (so projects stay projects).
pub fn import(source: &Path) -> std::io::Result<Imported> {
    let parent = imported_dir().ok_or_else(|| std::io::Error::other("no home folder"))?;
    let name = import_name(source);
    let mut dest = parent.join(&name);
    let mut n = 2;
    while dest.exists() {
        dest = parent.join(format!("{name} {n}"));
        n += 1;
    }
    let mut found = Imported { name: dest.file_name().unwrap_or_default().to_string_lossy().into_owned(), kits: 0, patterns: 0 };
    let wanted = |rel: &Path| {
        let hidden = rel.components().any(|c| {
            let c = c.as_os_str().to_string_lossy();
            c.starts_with('.') || c == "__MACOSX"
        });
        (!hidden && (has_ext(rel, "snd") || has_ext(rel, "pat"))).then(|| has_ext(rel, "snd"))
    };
    let mut put = |rel: &Path, bytes: &[u8]| -> std::io::Result<()> {
        let Some(kit) = wanted(rel) else { return Ok(()) };
        let path = dest.join(rel);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, bytes)?;
        if kit {
            found.kits += 1;
        } else {
            found.patterns += 1;
        }
        Ok(())
    };
    let result = if source.is_dir() {
        copy_tree(source, Path::new(""), &mut put)
    } else {
        let file = std::fs::File::open(source)?;
        let mut zip = zip::ZipArchive::new(file).map_err(|e| std::io::Error::other(format!("not a zip file ({e})")))?;
        (0..zip.len()).try_for_each(|i| {
            let mut entry = zip.by_index(i).map_err(std::io::Error::other)?;
            // enclosed_name: no absolute paths or "..", nothing lands outside the import folder
            let Some(rel) = entry.enclosed_name().filter(|_| entry.is_file()) else { return Ok(()) };
            if wanted(&rel).is_none() || entry.size() > 1 << 20 {
                return Ok(());
            }
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut bytes)?;
            put(&rel, &bytes)
        })
    };
    if result.is_err() || found.kits + found.patterns == 0 {
        let _ = std::fs::remove_dir_all(&dest);
    }
    result?;
    if found.kits + found.patterns == 0 {
        return Err(std::io::Error::other("no LXR kits (.SND) or patterns (.PAT) in it"));
    }
    Ok(found)
}

fn copy_tree(dir: &Path, rel: &Path, put: &mut impl FnMut(&Path, &[u8]) -> std::io::Result<()>) -> std::io::Result<()> {
    for path in sorted_entries(dir) {
        let rel = rel.join(path.file_name().unwrap_or_default());
        if path.is_dir() {
            copy_tree(&path, &rel, put)?;
        } else if has_ext(&path, "snd") || has_ext(&path, "pat") {
            put(&rel, &std::fs::read(&path)?)?;
        }
    }
    Ok(())
}

/// The import a folder belongs to: Imported/NAME/... -> NAME.
fn imported_as(dir: &Path) -> Option<String> {
    let base = imported_dir()?;
    let rel = dir.strip_prefix(&base).ok()?;
    rel.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned())
}

/// A .snd file: 8 byte name, then the parameter values (firmware preset_loadDrumsetSD).
pub fn parse_snd(bytes: &[u8]) -> Option<(String, Vec<u8>)> {
    if bytes.len() < 8 + 16 {
        return None;
    }
    let name: String = bytes[..8]
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '?' })
        .collect();
    let mut data = bytes[8..].iter().copied().take(KIT_SIZE).collect::<Vec<_>>();
    data.resize(KIT_SIZE, 0);
    Some((name.trim().to_string(), data))
}

/// The .snd file of a kit, with its name space-padded to 8 characters as the LXR writes it.
pub fn snd_bytes(name: &str, data: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = name.bytes().filter(|b| b.is_ascii_graphic() || *b == b' ').take(8).collect();
    out.resize(8, b' ');
    out.extend(data.iter().copied().take(KIT_SIZE));
    out.resize(8 + KIT_SIZE, 0);
    out
}

/// Converts a .pat file of any version to version 3 (firmware preset_readPatternData()): version 2
/// has no step timing bytes (they become flam 0 / shift 7 = none), version 1 also no kit number.
pub fn pattern_to_v3(raw: &[u8]) -> Option<Vec<u8>> {
    const TRACKS: usize = 7;
    const STEP_DATA: usize = 64 * 7;
    let (mut pos, version) = if raw.len() >= 14 && &raw[8..12] == b"PatV" {
        (14, std::str::from_utf8(&raw[12..14]).ok()?.parse::<u8>().ok()?)
    } else {
        (8, 1)
    };
    let mut out = Vec::with_capacity(PATTERN_BYTES);
    out.extend_from_slice(&[0; 8]);
    out.extend_from_slice(b"PatV03");
    for _ in 0..TRACKS {
        out.extend_from_slice(raw.get(pos..pos + STEP_DATA)?);
        pos += STEP_DATA;
        if version > 2 {
            out.extend_from_slice(raw.get(pos..pos + 64)?);
            pos += 64;
        } else {
            out.extend(std::iter::repeat_n(0x70u8, 64));
        }
    }
    let rest = TRACKS * 4 * 2 + 1 + TRACKS;
    out.extend_from_slice(raw.get(pos..pos + rest)?);
    pos += rest;
    out.push(if version > 1 { *raw.get(pos)? } else { 0 });
    (out.len() == PATTERN_BYTES).then_some(out)
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    paths
}

fn has_ext(path: &Path, ext: &str) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// The slot number of an SD card file name ("07-BADON.SND" -> 7).
fn slot(path: &Path) -> Option<usize> {
    let name = path.file_name()?.to_str()?;
    let n: usize = name.get(..2)?.parse().ok()?;
    (name.as_bytes().get(2) == Some(&b'-') && n < NUM_PATTERNS).then_some(n)
}

fn folder_name(dir: &Path) -> String {
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    // an imported SD card: its projects are NAME 1, NAME 2 ... (PROJ00 = 1, as on the LXR)
    if let Some(import) = imported_as(dir) {
        let upper = name.to_ascii_uppercase();
        return match upper.strip_prefix("PROJ").and_then(|n| n.parse::<usize>().ok()) {
            Some(n) => format!("{import} {}", n + 1),
            None if import == name => import,
            None => format!("{import} {name}"),
        };
    }
    // SD card project folders are just PROJ00..PROJ63; say where they came from
    if name.to_ascii_uppercase().starts_with("PROJ") {
        if let Some(parent) = dir.parent().and_then(|p| p.file_name()) {
            let parent: String = parent.to_string_lossy().split_whitespace().next().unwrap_or("").chars().take(6).collect();
            return format!("{parent} {}", &name[4..]).trim().to_string();
        }
    }
    name
}

/// Every kit file in the library as (bank, name, values); the bank is the folder it is in (an
/// import's kits are all in its bank). INIT and EMPTY kits and second copies are left out.
pub fn kits() -> Vec<(String, String, Vec<u8>)> {
    let mut out = Vec::new();
    if let Some(root) = root() {
        let mut seen = std::collections::HashSet::new();
        collect_kits(&root, &root, &mut out, &mut seen);
    }
    out
}

fn collect_kits(root: &Path, dir: &Path, out: &mut Vec<(String, String, Vec<u8>)>, seen: &mut std::collections::HashSet<Vec<u8>>) {
    let mut entries = sorted_entries(dir);
    // the user's own kits first: a copy of one elsewhere is the one left out
    if dir == root {
        entries.sort_by_key(|p| p.file_name().is_none_or(|n| n != "Kits"));
    }
    for path in entries {
        if path.is_dir() {
            collect_kits(root, &path, out, seen);
        } else if has_ext(&path, "snd") {
            if let Some((name, data)) = std::fs::read(&path).ok().as_deref().and_then(parse_snd) {
                let upper = name.to_ascii_uppercase();
                if upper.starts_with("INIT") || upper.starts_with("EMPTY") || !seen.insert(data.clone()) {
                    continue;
                }
                let bank = if dir == root || dir.file_name().is_some_and(|n| n == "Kits") {
                    "USER".to_string()
                } else if let Some(import) = imported_as(dir) {
                    import
                } else {
                    folder_name(dir)
                };
                let name = if name.is_empty() { path.file_stem().unwrap_or_default().to_string_lossy().into_owned() } else { name };
                out.push((bank, name, data));
            }
        }
    }
}

/// Every folder in the library that holds patterns, as a project.
pub fn projects() -> Vec<Project> {
    let mut out = Vec::new();
    if let Some(root) = root() {
        let mut imports = Vec::new();
        collect_projects(&root, &mut out, &mut imports);
        // an import with a single project is just its name ("HRTL DLC", not the folder inside it)
        for i in 0..out.len() {
            if let Some(import) = &imports[i] {
                if imports.iter().filter(|j| j.as_ref() == Some(import)).count() == 1 {
                    out[i].name = import.clone();
                }
            }
        }
    }
    out
}

/// Projects with at least one pattern that has steps; `imports` gets the import each came from.
fn collect_projects(dir: &Path, out: &mut Vec<Project>, imports: &mut Vec<Option<String>>) {
    let entries = sorted_entries(dir);
    let mut project = Project::empty(folder_name(dir));
    let mut found = false;
    for path in &entries {
        if path.is_dir() {
            collect_projects(path, out, imports);
            continue;
        }
        let Some(n) = slot(path) else { continue };
        if has_ext(path, "pat") {
            if let Some(p) = std::fs::read(path).ok().as_deref().and_then(pattern_to_v3) {
                found |= !seq::is_empty(&p);
                project.patterns[n] = Some(p);
            }
        } else if has_ext(path, "snd") {
            if let Some(kit) = std::fs::read(path).ok().as_deref().and_then(parse_snd) {
                project.kits[n] = Some(kit);
            }
        }
    }
    if found {
        out.push(project);
        imports.push(imported_as(dir));
    }
}

/// A name that is safe as a file name on any disk.
fn clean_stem(stem: &str, fallback: &str) -> String {
    let clean: String = stem.trim().chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    if clean.is_empty() { fallback.to_string() } else { clean }
}

/// A file name that is safe on any disk, without overwriting `existing` files.
fn free_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let clean = clean_stem(stem, "KIT");
    let mut path = dir.join(format!("{clean}.{ext}"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{clean}-{n}.{ext}"));
        n += 1;
    }
    path
}

/// Saves a kit as ~/Music/ELABS RLX/Kits/NAME.SND.
pub fn save_kit(name: &str, data: &[u8]) -> std::io::Result<PathBuf> {
    let dir = kits_dir().ok_or_else(|| std::io::Error::other("no home folder"))?;
    std::fs::create_dir_all(&dir)?;
    let path = free_path(&dir, name, "SND");
    std::fs::write(&path, snd_bytes(name, data))?;
    Ok(path)
}

/// Saves the session's patterns and project kits as an SD card style project folder
/// (~/Music/ELABS RLX/Projects/NAME/NN-.PAT, NN-NAME.SND).
pub fn save_project(name: &str, patterns: &[Vec<u8>], kits: &[Option<(String, Vec<u8>)>]) -> std::io::Result<PathBuf> {
    let parent = projects_dir().ok_or_else(|| std::io::Error::other("no home folder"))?;
    std::fs::create_dir_all(&parent)?;
    let dir = free_path(&parent, name, "lxr");
    let dir = dir.with_extension("");
    std::fs::create_dir_all(&dir)?;
    for (nr, p) in patterns.iter().enumerate().filter(|(_, p)| !seq::is_empty(p)) {
        std::fs::write(dir.join(format!("{nr:02}-.PAT")), p)?;
    }
    for (nr, kit) in kits.iter().enumerate() {
        if let Some((kit_name, data)) = kit {
            let short: String = kit_name.chars().filter(|c| c.is_ascii_alphanumeric()).take(5).collect::<String>().to_uppercase();
            std::fs::write(dir.join(format!("{nr:02}-{short}.SND")), snd_bytes(kit_name, data))?;
        }
    }
    Ok(dir)
}

/// Adds one pattern, with the kit it plays, to the project folder ~/Music/ELABS RLX/Projects/PROJECT
/// (made if needed) in its first free slot; returns the folder and the slot.
pub fn save_pattern(project: &str, pattern: &[u8], kit: &(String, Vec<u8>)) -> std::io::Result<(PathBuf, usize)> {
    let parent = projects_dir().ok_or_else(|| std::io::Error::other("no home folder"))?;
    let dir = parent.join(clean_stem(project, "PATTERNS"));
    std::fs::create_dir_all(&dir)?;
    let used: Vec<usize> = sorted_entries(&dir).iter().filter(|p| has_ext(p, "pat")).filter_map(|p| slot(p)).collect();
    let nr = (0..NUM_PATTERNS).find(|n| !used.contains(n)).ok_or_else(|| std::io::Error::other("the project is full (64 patterns)"))?;
    let mut p = pattern.to_vec();
    seq::set_kit_number(&mut p, nr);
    std::fs::write(dir.join(format!("{nr:02}-.PAT")), &p)?;
    let short: String = kit.0.chars().filter(|c| c.is_ascii_alphanumeric()).take(5).collect::<String>().to_uppercase();
    std::fs::write(dir.join(format!("{nr:02}-{short}.SND")), snd_bytes(&kit.0, &kit.1))?;
    Ok((dir, nr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_conversion_matches_the_build_time_converter() {
        // every embedded pattern was converted to v3 by tools/gen_projects.py; converting a v3 file
        // again gives the same bytes, and a v2 copy of it converts back to it
        for project in 0..crate::project_table::PROJECTS.len() {
            for nr in 0..NUM_PATTERNS {
                // slots without a pattern hold the firmware's cleared pattern, never zeros
                let at = project * crate::project_table::PROJECT_BYTES + nr * PATTERN_BYTES;
                assert_eq!(&crate::project_table::PROJECT_DATA[at..at + PATTERN_BYTES], &seq::project_pattern(project, nr)[..]);
                let p = seq::project_pattern(project, nr);
                assert_eq!(pattern_to_v3(&p).as_deref(), Some(&p[..]));
                let mut v2 = p[..8].to_vec();
                v2.extend_from_slice(b"PatV02");
                for t in 0..7 {
                    v2.extend_from_slice(&p[14 + t * 512..14 + t * 512 + 448]);
                }
                v2.extend_from_slice(&p[14 + 7 * 512..]);
                let mut expected = p.clone();
                for t in 0..7 {
                    expected[14 + t * 512 + 448..14 + (t + 1) * 512].fill(0x70);
                }
                assert_eq!(pattern_to_v3(&v2), Some(expected));
            }
        }
    }

    #[test]
    fn saved_kits_and_projects_are_found_again() {
        let home = std::env::temp_dir().join(format!("lxr-library-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let old_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &home);

        let kit: Vec<u8> = (0..KIT_SIZE).map(|i| (i % 128) as u8).collect();
        let path = save_kit("My Kick", &kit).unwrap();
        assert_eq!(path.file_name().unwrap(), "My_Kick.SND");
        assert_eq!(save_kit("My Kick", &kit).unwrap().file_name().unwrap(), "My_Kick-2.SND", "no overwriting");

        let mut patterns = vec![seq::empty_pattern(); NUM_PATTERNS];
        seq::toggle_step(&mut patterns[3], 0, 0);
        seq::set_kit_number(&mut patterns[3], 5);
        let mut slots = vec![None; NUM_PATTERNS];
        slots[5] = Some(("Snappy".to_string(), kit.clone()));
        let dir = save_project("Set One", &patterns, &slots).unwrap();
        assert!(dir.join("03-.PAT").exists() && dir.join("05-SNAPP.SND").exists());
        // single patterns go into the first free slots of a folder, each with its kit
        let (pdir, nr) = save_pattern("Grooves", &patterns[3], &("Snappy".to_string(), kit.clone())).unwrap();
        assert_eq!(nr, 0);
        assert_eq!(save_pattern("Grooves", &patterns[3], &("Snappy".to_string(), kit.clone())).unwrap().1, 1);
        assert!(pdir.join("01-.PAT").exists() && pdir.join("01-SNAPP.SND").exists());
        assert!(!dir.join("00-.PAT").exists(), "empty patterns aren't written");

        // importing a download: its folders (and so its projects) are kept, other files skipped
        let download = home.join("LXR download");
        std::fs::create_dir_all(download.join("SD/PROJ00")).unwrap();
        std::fs::create_dir_all(download.join("__MACOSX/SD/PROJ00")).unwrap();
        std::fs::write(download.join("SD/PROJ00/00-.PAT"), &patterns[3]).unwrap();
        std::fs::write(download.join("SD/PROJ00/00-TEKK.SND"), snd_bytes("Tekk", &kit)).unwrap();
        std::fs::write(download.join("SD/PROJ00/01-INITK.SND"), snd_bytes("INITKIT", &[0; KIT_SIZE])).unwrap();
        std::fs::write(download.join("__MACOSX/SD/PROJ00/00-.PAT"), b"junk").unwrap();
        std::fs::write(download.join("SD/readme.txt"), b"hi").unwrap();
        let imported = import(&download).unwrap();
        assert_eq!((imported.name.as_str(), imported.kits, imported.patterns), ("LXR DOWNLOAD", 2, 1));
        assert!(projects().iter().any(|p| p.name == "LXR DOWNLOAD" && p.pattern(0) == patterns[3]), "one project: named after the import");
        assert_eq!(import(&download).unwrap().name, "LXR DOWNLOAD 2", "a second import goes beside it");
        assert!(import(&home.join("nothing")).is_err());

        let found = kits();
        assert!(!found.iter().any(|(_, name, _)| name.starts_with("INIT")), "init kits aren't listed");
        assert_eq!(found.iter().filter(|(_, name, _)| name == "Tekk").count(), 0, "same values as My Kick: listed once");
        assert!(found.iter().any(|(bank, name, data)| bank == "USER" && name == "My Kick" && *data == kit));
        let projects = projects();
        let p = projects.iter().find(|p| p.name == "Set_One").expect("project found");
        assert_eq!(p.pattern(3), patterns[3]);
        assert_eq!(p.kit(5), Some(&("Snappy".to_string(), kit)));

        match old_home {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn snd_round_trip() {
        let data: Vec<u8> = (0..KIT_SIZE as u8).collect();
        let bytes = snd_bytes("Kick", &data);
        assert_eq!(bytes.len(), 255);
        assert_eq!(&bytes[..8], b"Kick    ");
        assert_eq!(parse_snd(&bytes), Some(("Kick".to_string(), data)));
    }
}

/// Imports Erica Synths' downloads from LXR_IMPORT_DIR into a temporary library and lists what
/// came in: `LXR_IMPORT_DIR=... cargo test --release --lib import_real_downloads -- --ignored --nocapture`
#[cfg(test)]
#[test]
#[ignore]
fn import_real_downloads() {
    let source = std::env::var("LXR_IMPORT_DIR").expect("LXR_IMPORT_DIR");
    let home = std::env::temp_dir().join(format!("lxr-import-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("HOME", &home);
    for zip in sorted_entries(Path::new(&source)).into_iter().filter(|p| has_ext(p, "zip")) {
        let done = import(&zip).unwrap();
        println!("{}: {} kit files, {} patterns", done.name, done.kits, done.patterns);
    }
    let kits = kits();
    let mut banks: Vec<(String, usize)> = Vec::new();
    for (bank, _, _) in &kits {
        match banks.iter_mut().find(|b| &b.0 == bank) {
            Some(b) => b.1 += 1,
            None => banks.push((bank.clone(), 1)),
        }
    }
    println!("kits listed: {banks:?}");
    for p in projects() {
        let n = p.patterns.iter().filter(|x| x.as_ref().is_some_and(|x| !seq::is_empty(x))).count();
        println!("project {}: {n} patterns", p.name);
    }
    let _ = std::fs::remove_dir_all(&home);
}
