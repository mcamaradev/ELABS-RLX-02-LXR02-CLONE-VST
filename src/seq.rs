//! Sequencer patterns, shared between the panel, the audio thread and the saved project.
//!
//! The plugin keeps every pattern as a version 3 .pat file (see engine/lxr_bridge.h); the panel
//! edits these bytes and the audio thread copies changed patterns into the firmware sequencer.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Mutex;

use crate::project_table::{ProjectInfo, KIT_BYTES, PROJECTS, PROJECT_BYTES, PROJECT_DATA, SLOTS};

pub use crate::project_table::PATTERN_BYTES;
pub const NUM_PATTERNS: usize = 64;
pub const NUM_TRACKS: usize = 7;
const STEPS: usize = 64;
const MAIN_STEPS_OFFSET: usize = 8 + 6 + NUM_TRACKS * STEPS * 8;
const SHUFFLE_OFFSET: usize = MAIN_STEPS_OFFSET + NUM_TRACKS * 4 * 2;
const LENGTH_OFFSET: usize = SHUFFLE_OFFSET + 1;
const KIT_OFFSET: usize = PATTERN_BYTES - 1;
/// no pattern switch requested
const NO_SELECT: u8 = 0xff;

/// A cleared pattern, as the firmware's seq_clearTrack() leaves it: every step at full volume, note
/// 60, probability 100, no automation, no flam or shift, no main steps, length 16.
pub fn empty_pattern() -> Vec<u8> {
    let mut p = Vec::with_capacity(PATTERN_BYTES);
    p.extend_from_slice(&[0; 8]);
    p.extend_from_slice(b"PatV03");
    for _ in 0..NUM_TRACKS {
        for k in 0..STEPS {
            let volume = if k % 8 == 0 { 0x80 | 127 } else { 127 };
            p.extend_from_slice(&[volume, 100, 60, 0xff, 0, 0xff, 0]);
        }
        p.extend(std::iter::repeat_n(0x70u8, STEPS));
    }
    p.extend(std::iter::repeat_n(0u8, NUM_TRACKS * 4 * 2));
    p.push(0); // shuffle
    p.extend(std::iter::repeat_n(16u8, NUM_TRACKS));
    p.push(0); // kit
    p
}

pub fn step_on(pattern: &[u8], track: usize, step: usize) -> bool {
    let at = MAIN_STEPS_OFFSET + (track * 4 + step / 16) * 2;
    let bits = u16::from_le_bytes([pattern[at], pattern[at + 1]]);
    bits & (1 << (step % 16)) != 0
}

pub fn toggle_step(pattern: &mut [u8], track: usize, step: usize) {
    let at = MAIN_STEPS_OFFSET + (track * 4 + step / 16) * 2;
    let bits = u16::from_le_bytes([pattern[at], pattern[at + 1]]) ^ (1 << (step % 16));
    pattern[at..at + 2].copy_from_slice(&bits.to_le_bytes());
}

/// The pattern's shuffle, 0-127.
pub fn shuffle(pattern: &[u8]) -> u8 {
    pattern[SHUFFLE_OFFSET]
}

pub fn set_shuffle(pattern: &mut [u8], value: u8) {
    pattern[SHUFFLE_OFFSET] = value.min(127);
}

// ---------------------------------------------------------------- euclid

/// Euclid generator settings of one track (the LXR's euclid page): length 1-16, steps 1-length,
/// rotation 0-15.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Euclid {
    pub length: u8,
    pub steps: u8,
    pub rotation: u8,
}

impl Default for Euclid {
    /// euklid_init()
    fn default() -> Self {
        Self { length: 16, steps: 4, rotation: 0 }
    }
}

/// The firmware's euclid generator (EuklidGenerator.c, euklid_generate()), step for step: bit n
/// of the result is step n.
pub fn euclid_bits(e: Euclid) -> u64 {
    let length = e.length.max(1);
    let steps = e.steps.clamp(1, length);
    let mut gen = EuclidGen { next_cnt1: 0, original_length: length as u16, buffer: 0 };
    gen.calc(length, steps, 1, 0);
    let rotation = e.rotation;
    // euklid_rotatePattern()
    if rotation != 0 && length != 1 && rotation % length != 0 {
        let amount = (rotation % length) as u32;
        let b = gen.buffer;
        gen.buffer = (b << amount) | (b >> (length as u32 - amount));
        gen.buffer &= !(u64::MAX.checked_shl(length as u32).unwrap_or(0));
    }
    gen.buffer
}

struct EuclidGen {
    next_cnt1: i32,
    original_length: u16,
    buffer: u64,
}

impl EuclidGen {
    /// getObjSize(): bits up to the highest set one, at least 1
    fn obj_size(group: u16) -> i32 {
        let size = 32 - (group as u32).leading_zeros() as i32;
        size.max(1)
    }

    /// euklid_calcRecursive(), with the firmware's 16 bit groups
    fn calc(&mut self, length: u8, steps: u8, group1: u16, group2: u16) {
        let remainder = if steps != 0 { length % steps } else { 0 };
        if self.next_cnt1 == 0 {
            self.next_cnt1 = steps as i32;
        }
        let object_cnt1 = self.next_cnt1;
        let obj_size1 = Self::obj_size(group1);
        let obj_size2 = Self::obj_size(group2);
        let obj_cnt2 = (self.original_length as i32 - object_cnt1 * obj_size1) / obj_size2;
        self.next_cnt1 = object_cnt1.min(obj_cnt2);

        if obj_cnt2 <= 1 {
            let mut pos = 0u32;
            for (count, size, group) in [(object_cnt1, obj_size1, group1), (obj_cnt2, obj_size2, group2)] {
                for _ in 0..count.max(0) {
                    for j in (0..size).rev() {
                        if group & (1 << j) != 0 && pos < 64 {
                            self.buffer |= 1 << pos;
                        }
                        pos += 1;
                    }
                }
            }
            return;
        }

        let original_group1 = group1;
        let mut group1 = group1.wrapping_shl(obj_size2 as u32);
        for i in 0..obj_size2 {
            if group2 & (1 << i) != 0 {
                group1 |= 1 << i;
            }
        }
        let group2 = if obj_cnt2 - object_cnt1 > 0 { group2 } else { original_group1 };
        self.calc(steps, remainder as u8, group1, group2);
    }
}

/// euklid_transferPattern(): the generated steps replace the track's steps in all 4 bars, and the
/// euclid length becomes the track length.
pub fn apply_euclid(pattern: &mut [u8], track: usize, e: Euclid) {
    let bits = euclid_bits(e);
    for bar in 0..4 {
        let at = MAIN_STEPS_OFFSET + (track * 4 + bar) * 2;
        pattern[at..at + 2].copy_from_slice(&((bits >> (16 * bar)) as u16).to_le_bytes());
    }
    // 16 is stored as 0, the default length
    pattern[LENGTH_OFFSET + track] = if e.length == 16 { 0 } else { e.length };
}

// ---------------------------------------------------------------- step data

const TRACK_BYTES: usize = STEPS * 8;
/// Step (SeqStep.h): volume (bit 7 = sub step active), probability, note, automation 1 number and
/// value, automation 2 number and value
const STEP_BYTES: usize = 7;
/// automation slot not in use (MidiMessages.h NO_AUTOMATION)
pub const NO_AUTOMATION: u8 = 0xff;
pub const FLAM_NAMES: [&str; 6] = ["off", "x2", "x3", "x4", "x6", "x8"];

/// One step's settings, as the LXR's step edit page shows them.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct StepData {
    pub velocity: u8,
    pub note: u8,
    /// 0-100 %
    pub prob: u8,
    /// index into FLAM_NAMES
    pub flam: u8,
    /// -7..7, stored as 0..14
    pub shift: i8,
    /// automation (parameter lock) destinations, firmware parameter numbers, None = off
    pub dest: [Option<u16>; 2],
    pub value: [u8; 2],
}

fn step_at(track: usize, step: usize) -> usize {
    14 + track * TRACK_BYTES + step * STEP_BYTES
}

fn timing_at(track: usize, step: usize) -> usize {
    14 + track * TRACK_BYTES + STEPS * STEP_BYTES + step
}

/// The sequencer stores automation targets in its own numbering: the front panel's parameter
/// numbers below 128 are one higher there (CortexParser.c FRONT_SET_P1_DEST).
fn dest_from_seq(n: u8) -> Option<u16> {
    match n {
        NO_AUTOMATION | 0 => None,
        n if n <= 128 => Some(n as u16 - 1),
        n => Some(n as u16),
    }
}

fn dest_to_seq(p: Option<u16>) -> u8 {
    match p {
        None | Some(0) => NO_AUTOMATION,
        Some(p) if p < 128 => p as u8 + 1,
        Some(p) => p.min(254) as u8,
    }
}

pub fn step(pattern: &[u8], track: usize, step: usize) -> StepData {
    let at = step_at(track, step);
    let timing = pattern[timing_at(track, step)];
    StepData {
        velocity: pattern[at] & 0x7f,
        prob: pattern[at + 1],
        note: pattern[at + 2] & 0x7f,
        dest: [dest_from_seq(pattern[at + 3]), dest_from_seq(pattern[at + 5])],
        value: [pattern[at + 4], pattern[at + 6]],
        flam: (timing & 0x0f).min(5),
        shift: ((timing >> 4) as i8 - 7).clamp(-7, 7),
    }
}

pub fn set_step(pattern: &mut [u8], track: usize, step: usize, d: StepData) {
    let at = step_at(track, step);
    // keep the sub step active bit
    pattern[at] = (pattern[at] & 0x80) | d.velocity.min(127);
    pattern[at + 1] = d.prob.min(100);
    pattern[at + 2] = d.note.min(127);
    pattern[at + 3] = dest_to_seq(d.dest[0]);
    pattern[at + 4] = d.value[0].min(127);
    pattern[at + 5] = dest_to_seq(d.dest[1]);
    pattern[at + 6] = d.value[1].min(127);
    pattern[timing_at(track, step)] = d.flam.min(5) | (((d.shift.clamp(-7, 7) + 7) as u8) << 4);
}

/// Steps the track plays before it starts over, 1-64 (stored 0 means 16).
pub fn track_length(pattern: &[u8], track: usize) -> u8 {
    match pattern[LENGTH_OFFSET + track] {
        0 => 16,
        n => n.min(64),
    }
}

pub fn set_track_length(pattern: &mut [u8], track: usize, length: u8) {
    pattern[LENGTH_OFFSET + track] = length.clamp(1, 64);
}

/// seq_clearTrack(): the track's steps, their settings and its length back to a cleared pattern's.
pub fn clear_track(pattern: &mut [u8], track: usize) {
    let empty = empty_pattern();
    let at = 14 + track * TRACK_BYTES;
    pattern[at..at + TRACK_BYTES].copy_from_slice(&empty[at..at + TRACK_BYTES]);
    let main = MAIN_STEPS_OFFSET + track * 8;
    pattern[main..main + 8].fill(0);
    pattern[LENGTH_OFFSET + track] = 16;
}

/// seq_copyTrack(): a track's steps, their settings and its length onto another track.
pub fn copy_track(pattern: &mut [u8], from: usize, to: usize) {
    if from == to {
        return;
    }
    let (src, dst) = (14 + from * TRACK_BYTES, 14 + to * TRACK_BYTES);
    pattern.copy_within(src..src + TRACK_BYTES, dst);
    let (src, dst) = (MAIN_STEPS_OFFSET + from * 8, MAIN_STEPS_OFFSET + to * 8);
    pattern.copy_within(src..src + 8, dst);
    pattern[LENGTH_OFFSET + to] = pattern[LENGTH_OFFSET + from];
}

fn copy_step(pattern: &mut [u8], track: usize, from: usize, to: usize) {
    let src = step_at(track, from);
    pattern.copy_within(src..src + STEP_BYTES, step_at(track, to));
    pattern[timing_at(track, to)] = pattern[timing_at(track, from)];
    if step_on(pattern, track, from) != step_on(pattern, track, to) {
        toggle_step(pattern, track, to);
    }
}

/// Every track twice as long, playing its steps twice (like a DAW's "duplicate loop"). A track
/// longer than 32 steps has no room and stays as it is. Returns how many tracks changed.
pub fn double_pattern(pattern: &mut [u8]) -> usize {
    let mut changed = 0;
    for track in 0..NUM_TRACKS {
        let length = track_length(pattern, track) as usize;
        if length * 2 > STEPS {
            continue;
        }
        for step in 0..length {
            copy_step(pattern, track, step, step + length);
        }
        set_track_length(pattern, track, (length * 2) as u8);
        changed += 1;
    }
    changed
}

/// Every track half as long, playing only its first half. The steps after the end stay in the
/// pattern (the LXR keeps them too), so lengthening the track again brings them back. Returns how
/// many tracks changed.
pub fn halve_pattern(pattern: &mut [u8]) -> usize {
    let mut changed = 0;
    for track in 0..NUM_TRACKS {
        let length = track_length(pattern, track);
        if length > 1 {
            set_track_length(pattern, track, length / 2);
            changed += 1;
        }
    }
    changed
}

/// seq_clearPattern(): all tracks cleared; shuffle and kit stay.
pub fn clear_pattern(pattern: &mut [u8]) {
    for track in 0..NUM_TRACKS {
        clear_track(pattern, track);
    }
}

pub fn kit_number(pattern: &[u8]) -> usize {
    pattern[KIT_OFFSET] as usize
}

pub fn set_kit_number(pattern: &mut [u8], kit: usize) {
    pattern[KIT_OFFSET] = kit.min(NUM_PATTERNS - 1) as u8;
}

pub fn is_empty(pattern: &[u8]) -> bool {
    (0..NUM_TRACKS * 4).all(|i| pattern[MAIN_STEPS_OFFSET + i * 2] == 0 && pattern[MAIN_STEPS_OFFSET + i * 2 + 1] == 0)
}

/// A project as on the LXR-02's SD card: 64 pattern slots and 64 kit slots.
pub struct Project {
    pub name: String,
    pub patterns: Vec<Option<Vec<u8>>>,
    pub kits: Vec<Option<(String, Vec<u8>)>>,
}

impl Project {
    pub fn empty(name: String) -> Self {
        Self { name, patterns: vec![None; NUM_PATTERNS], kits: vec![None; NUM_PATTERNS] }
    }

    pub fn pattern(&self, nr: usize) -> Vec<u8> {
        self.patterns.get(nr).cloned().flatten().unwrap_or_else(empty_pattern)
    }

    pub fn kit(&self, nr: usize) -> Option<&(String, Vec<u8>)> {
        self.kits.get(nr)?.as_ref()
    }
}

/// The projects built into the plugin, then the ones in the user's library folder.
pub fn all_projects() -> Vec<Project> {
    let mut out: Vec<Project> = (0..PROJECTS.len())
        .map(|p| Project {
            name: PROJECTS[p].name.to_string(),
            patterns: (0..NUM_PATTERNS).map(|nr| PROJECTS[p].has_pattern[nr].then(|| project_pattern(p, nr))).collect(),
            kits: (0..NUM_PATTERNS).map(|nr| project_kit(p, nr).map(|(n, d)| (n.to_string(), d.to_vec()))).collect(),
        })
        .collect();
    out.extend(crate::library::projects());
    out
}

/// Pattern `nr` of an embedded project.
pub fn project_pattern(project: usize, nr: usize) -> Vec<u8> {
    if !PROJECTS[project].has_pattern[nr] {
        return empty_pattern();
    }
    let at = project * PROJECT_BYTES + nr * PATTERN_BYTES;
    PROJECT_DATA[at..at + PATTERN_BYTES].to_vec()
}

/// Kit slot `nr` of an embedded project as (name, values), if the project has one there.
pub fn project_kit(project: usize, nr: usize) -> Option<(&'static str, &'static [u8])> {
    let info: &ProjectInfo = PROJECTS.get(project)?;
    let name = *info.kit_names.get(nr)?;
    if name.is_empty() {
        return None;
    }
    let at = project * PROJECT_BYTES + SLOTS * PATTERN_BYTES + nr * KIT_BYTES;
    Some((name, &PROJECT_DATA[at..at + KIT_BYTES]))
}

pub struct SeqShared {
    pub patterns: Mutex<Vec<Vec<u8>>>,
    /// name of the project the patterns came from
    pub project: Mutex<Option<String>>,
    /// that project's kits, by slot: the kit each pattern names (saved with the session, so a set
    /// doesn't depend on the library folder)
    pub project_kits: Mutex<Vec<Option<(String, Vec<u8>)>>>,
    /// load the kit that goes with a pattern when the sequencer switches to it
    pub kit_follows: AtomicBool,
    /// euclid generator settings per track
    pub euclid: Mutex<[Euclid; NUM_TRACKS]>,
    /// muted tracks, one bit each
    pub mutes: AtomicU8,
    /// live recording into the playing pattern
    pub record: AtomicBool,
    /// song mode: patterns played in order
    pub song: Mutex<Vec<u8>>,
    pub song_on: AtomicBool,
    /// start over at the end of the song (or stop)
    pub song_loop: AtomicBool,
    /// the song changed and has to go to the engine
    pub song_dirty: AtomicBool,
    /// patterns the audio thread still has to copy into the engine, one bit per pattern
    pub dirty: AtomicU64,
    /// pattern switch requested by the panel
    pub select: AtomicU8,
    // published by the audio thread
    pub active: AtomicU8,
    pub pending: AtomicU8,
    pub running: AtomicBool,
    pub step: [AtomicU8; NUM_TRACKS],
}

impl Default for SeqShared {
    fn default() -> Self {
        Self {
            patterns: Mutex::new(vec![empty_pattern(); NUM_PATTERNS]),
            project: Mutex::new(None),
            project_kits: Mutex::new(vec![None; NUM_PATTERNS]),
            kit_follows: AtomicBool::new(true),
            euclid: Mutex::new([Euclid::default(); NUM_TRACKS]),
            mutes: AtomicU8::new(0),
            record: AtomicBool::new(false),
            song: Mutex::new(Vec::new()),
            song_on: AtomicBool::new(false),
            song_loop: AtomicBool::new(true),
            song_dirty: AtomicBool::new(true),
            dirty: AtomicU64::new(0),
            select: AtomicU8::new(NO_SELECT),
            active: AtomicU8::new(0),
            pending: AtomicU8::new(0),
            running: AtomicBool::new(false),
            step: Default::default(),
        }
    }
}

impl SeqShared {
    pub fn mark_dirty(&self, nr: usize) {
        self.dirty.fetch_or(1 << nr, Ordering::AcqRel);
    }

    /// Replaces all patterns and project kits with a project's.
    pub fn load_project(&self, project: &Project) {
        if let Ok(mut patterns) = self.patterns.lock() {
            for (nr, p) in patterns.iter_mut().enumerate() {
                *p = project.pattern(nr);
            }
        }
        if let Ok(mut kits) = self.project_kits.lock() {
            *kits = project.kits.clone();
        }
        if let Ok(mut p) = self.project.lock() {
            *p = Some(project.name.clone());
        }
        self.dirty.store(u64::MAX, Ordering::Release);
    }

    pub fn project_name(&self) -> Option<String> {
        self.project.lock().ok().and_then(|p| p.clone())
    }

    pub fn request_pattern(&self, nr: usize) {
        self.select.store(nr as u8, Ordering::Release);
    }

    pub fn take_request(&self) -> Option<u8> {
        match self.select.swap(NO_SELECT, Ordering::AcqRel) {
            NO_SELECT => None,
            nr => Some(nr),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn euclid_matches_known_rhythms() {
        let bits = |l, s, r| euclid_bits(Euclid { length: l, steps: s, rotation: r });
        assert_eq!(bits(16, 4, 0), 0b0001_0001_0001_0001); // four on the floor
        assert_eq!(bits(8, 3, 0).count_ones(), 3); // tresillo has 3 hits in 8
        assert_eq!(bits(16, 16, 0), 0xffff);
        assert_eq!(bits(5, 2, 0).count_ones(), 2);
        // applied to a pattern: the track's steps and length, other tracks untouched
        let mut p = empty_pattern();
        toggle_step(&mut p, 2, 40);
        apply_euclid(&mut p, 3, Euclid { length: 12, steps: 5, rotation: 2 });
        let expected = bits(12, 5, 2);
        for step in 0..64 {
            assert_eq!(step_on(&p, 3, step), expected & (1 << step) != 0, "step {step}");
        }
        assert!(step_on(&p, 2, 40), "other tracks keep their steps");
        assert_eq!(p[LENGTH_OFFSET + 3], 12);
        apply_euclid(&mut p, 3, Euclid::default());
        assert_eq!(p[LENGTH_OFFSET + 3], 0, "16 is stored as the default length 0");
        set_shuffle(&mut p, 200);
        assert_eq!(shuffle(&p), 127);

        // step settings read back as written, without touching the step's on/off or neighbours
        let mut q = empty_pattern();
        let before = q.clone();
        let d = StepData { velocity: 90, note: 64, prob: 50, flam: 3, shift: -4, dest: [Some(8), Some(200)], value: [100, 3] };
        set_step(&mut q, 4, 17, d);
        assert_eq!(step(&q, 4, 17), d);
        assert_eq!(step(&q, 4, 18), step(&before, 4, 18));
        assert_eq!(q[step_at(4, 17)] & 0x80, before[step_at(4, 17)] & 0x80, "sub step bit kept");
        assert_eq!(q[step_at(4, 17) + 3], 9, "front parameter 8 is 9 in the sequencer's numbering");
        assert_eq!(step(&empty_pattern(), 0, 0).dest, [None, None]);
        assert_eq!(step(&empty_pattern(), 0, 0).shift, 0);
        // track length and clearing
        set_track_length(&mut q, 2, 13);
        assert_eq!(track_length(&q, 2), 13);
        toggle_step(&mut q, 4, 3);
        clear_track(&mut q, 4);
        assert!(!step_on(&q, 4, 3));
        assert_eq!(step(&q, 4, 17), step(&before, 4, 17));
        assert_eq!(track_length(&q, 2), 13, "other tracks untouched");
        clear_pattern(&mut q);
        assert_eq!(q, before);
        // copying a track brings its steps, settings and length
        set_step(&mut q, 1, 9, d);
        toggle_step(&mut q, 1, 9);
        set_track_length(&mut q, 1, 12);
        copy_track(&mut q, 1, 6);
        assert!(step_on(&q, 6, 9));
        assert_eq!(step(&q, 6, 9), d);
        assert_eq!(track_length(&q, 6), 12);
        assert!(step_on(&q, 1, 9), "the source stays");
        // doubling repeats each track over twice its length; halving shortens it back
        let mut r = empty_pattern();
        set_track_length(&mut r, 6, 40);
        set_track_length(&mut r, 2, 3);
        toggle_step(&mut r, 2, 1);
        set_step(&mut r, 2, 1, d);
        toggle_step(&mut r, 0, 20); // past the end of a 16 step track: overwritten by step 4
        assert_eq!(double_pattern(&mut r), 6, "the 40 step track has no room");
        assert_eq!((track_length(&r, 0), track_length(&r, 2), track_length(&r, 6)), (32, 6, 40));
        assert!(step_on(&r, 2, 1) && step_on(&r, 2, 4) && !step_on(&r, 2, 3) && !step_on(&r, 2, 5));
        assert_eq!(step(&r, 2, 4), d);
        assert!(!step_on(&r, 0, 20));
        assert_eq!(halve_pattern(&mut r), 7);
        assert_eq!((track_length(&r, 0), track_length(&r, 2), track_length(&r, 6)), (16, 3, 20));
        assert!(step_on(&r, 2, 4), "steps past the end are kept");
        // rotation keeps the hits and the length
        for r in 0..16 {
            assert_eq!(bits(16, 5, r).count_ones(), 5);
            assert_eq!(bits(13, 5, r) >> 13, 0);
        }
    }
}
