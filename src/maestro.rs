//! MAESTRO (behind the robot in LANES): plays the pattern a little differently every bar, with a
//! fill at the end of each phrase, after the Maestro of the ELABS Liveset Player.
//!
//! Each bar the audio thread takes the user's pattern, makes a varied copy of the 16 steps every
//! track is about to play (velocities, ghost notes, a hat ratchet, a moved perc; on the third beat of
//! the phrase's last bar a fill) and gives the copy to the firmware sequencer. The user's pattern is never changed: the
//! panel shows it, and it is uploaded again when the Maestro stops (or REC is switched on, so a
//! variation can't be recorded into it). It also moves settings ("moves", each with its own switch,
//! CHANCE per bar and DEPTH): hat and snare decays, the colour of hats, snare, cymbal and percs (never a
//! pitch, never the kick's sound), and in ENTROPIA MOD the delay/reverb sends (throws), the ring LFO
//! and the master filter (sweeps, mostly into a fill). A setting the user turns becomes the new
//! centre the move goes around, and everything goes back to the user's values when the Maestro stops.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use crate::engine::Engine;
use crate::entropia as ent;
use crate::seq::{self, SeqShared, StepData, NUM_PATTERNS, NUM_TRACKS, PATTERN_BYTES};

pub const FILL_NAMES: [&str; 5] = ["snare roll", "clap stutter", "tom run", "hat build", "kick drop"];
/// fill every N bars; 0 = no fills
pub const PHRASES: [u8; 4] = [0, 4, 8, 16];

/// What the Maestro may move besides the steps, in panel order.
pub const MOVES: [&str; 5] = ["DECAY", "COLOUR", "SENDS", "RING LFO", "FILTER"];
pub const MOVE_HELP: [&str; 5] = [
    "hat (closed and open) and snare decays",
    "hat, snare and cymbal filters, snare noise, cymbal and perc decays",
    "ENTROPIA MOD: throws a voice into the delay or reverb for a bar, the snare or clap at fills",
    "ENTROPIA MOD: the ring LFO's rate (or note length when synced), depth and now and then its wave",
    "ENTROPIA MOD: master filter sweeps, mostly into a fill (high pass rising or low pass closing), back on the downbeat",
];
pub const DECAY: usize = 0;
pub const COLOUR: usize = 1;
pub const SENDS: usize = 2;
pub const LFO: usize = 3;
pub const FILTER: usize = 4;
/// (on, chance, depth) of each move when nothing was saved
const MOVE_DEFAULTS: [(bool, u8, u8); 5] = [(true, 60, 40), (true, 60, 40), (true, 30, 50), (true, 30, 50), (true, 50, 60)];

/// One move: on/off, how often (0-100 % of bars, or of fills for the filter) and how far (0-100).
pub struct Move {
    pub on: AtomicBool,
    pub chance: AtomicU8,
    pub depth: AtomicU8,
}

impl Move {
    fn new((on, chance, depth): (bool, u8, u8)) -> Self {
        Self { on: AtomicBool::new(on), chance: AtomicU8::new(chance), depth: AtomicU8::new(depth) }
    }

    /// (chance, depth) as 0-1, or None when off or at depth 0.
    fn get(&self) -> Option<(f32, f32)> {
        let depth = self.depth.load(Ordering::Acquire);
        (self.on.load(Ordering::Acquire) && depth > 0).then(|| (self.chance.load(Ordering::Acquire) as f32 / 100.0, depth as f32 / 100.0))
    }
}

const KICK: usize = 0;
const DRUM2: usize = 1;
const DRUM3: usize = 2;
const SNARE: usize = 3;
const CLAP: usize = 4;
const CLOSED_HAT: usize = 5;
const OPEN_HAT: usize = 6;

/// Settings from the panel and what the audio thread reports back.
pub struct MaestroShared {
    pub on: AtomicBool,
    /// 0-100: how much each bar differs
    pub variation: AtomicU8,
    /// DECAY, COLOUR, SENDS, LFO, FILTER
    pub moves: [Move; 5],
    /// bars per phrase; a fill ends each phrase (0 = no fills)
    pub phrase: AtomicU8,
    /// tracks the Maestro may play with, bit per track
    pub tracks: AtomicU8,
    /// fill in the next bar
    pub fill_now: AtomicBool,
    /// bar of the phrase now playing (0-based)
    pub bar: AtomicU8,
    /// the last fill, FILL_NAMES index + 1 (0 = none yet)
    pub last_fill: AtomicU8,
}

impl Default for MaestroShared {
    fn default() -> Self {
        Self {
            on: AtomicBool::new(false),
            variation: AtomicU8::new(50),
            moves: MOVE_DEFAULTS.map(Move::new),
            phrase: AtomicU8::new(8),
            tracks: AtomicU8::new(0x7f),
            fill_now: AtomicBool::new(false),
            bar: AtomicU8::new(0),
            last_fill: AtomicU8::new(0),
        }
    }
}

impl MaestroShared {
    /// "on,variation,gestures,phrase,tracks" (gestures: the old single amount, kept so older
    /// plugin versions read it) and then on,chance,depth of each move, for the saved session.
    pub fn to_field(&self) -> String {
        let mut out = format!(
            "{},{},{},{},{}",
            self.on.load(Ordering::Acquire) as u8,
            self.variation.load(Ordering::Acquire),
            self.moves[COLOUR].depth.load(Ordering::Acquire),
            self.phrase.load(Ordering::Acquire),
            self.tracks.load(Ordering::Acquire)
        );
        for m in &self.moves {
            out += &format!(",{},{},{}", m.on.load(Ordering::Acquire) as u8, m.chance.load(Ordering::Acquire), m.depth.load(Ordering::Acquire));
        }
        out
    }

    pub fn from_field(&self, text: &str) {
        let v: Vec<u8> = text.split(',').filter_map(|n| n.parse().ok()).collect();
        if v.len() < 5 {
            return;
        }
        let [on, variation, gestures, phrase, tracks] = [v[0], v[1], v[2], v[3], v[4]];
        self.on.store(on != 0, Ordering::Release);
        self.variation.store(variation.min(100), Ordering::Release);
        self.phrase.store(if PHRASES.contains(&phrase) { phrase } else { 8 }, Ordering::Release);
        self.tracks.store(tracks & 0x7f, Ordering::Release);
        if v.len() == 5 + 3 * MOVES.len() {
            for (m, s) in self.moves.iter().zip(v[5..].chunks(3)) {
                m.on.store(s[0] != 0, Ordering::Release);
                m.chance.store(s[1].min(100), Ordering::Release);
                m.depth.store(s[2].min(100), Ordering::Release);
            }
        } else {
            // a session from before the moves: its one gestures amount moved decays and colour
            for m in [DECAY, COLOUR] {
                self.moves[m].depth.store(gestures.min(100), Ordering::Release);
            }
        }
    }
}

/// xorshift: small, fast, and no allocation on the audio thread
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// 0..1
    fn f(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn pick(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn chance(&mut self, p: f32) -> bool {
        self.f() < p
    }

    fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + self.pick((hi - lo + 1) as usize) as i32
    }
}

/// The pattern steps track `t` plays in the 16 steps of the coming bar.
pub type Positions = [[usize; 16]; NUM_TRACKS];

/// Where each track is about to be, the next step being the bar's step `next`. The firmware counts
/// from -1 before the first step. (The bar's steps before `next` come out as the ones already played.)
fn positions(engine: &Engine, p: &[u8], next: usize) -> Positions {
    let mut pos: Positions = [[0; 16]; NUM_TRACKS];
    for (t, row) in pos.iter_mut().enumerate() {
        let current = engine.seq_step_index(t as u8) as i8 as i32;
        let length = seq::track_length(p, t) as i32;
        for (k, s) in row.iter_mut().enumerate() {
            *s = (current + 1 + k as i32 - next as i32).rem_euclid(length) as usize;
        }
    }
    pos
}

fn has_hits(p: &[u8], t: usize) -> bool {
    (0..seq::track_length(p, t) as usize).any(|s| seq::step_on(p, t, s))
}

/// A step of the track to copy new hits from, so they keep its note and parameter locks.
fn template(p: &[u8], t: usize) -> StepData {
    let first = (0..seq::track_length(p, t) as usize).find(|&s| seq::step_on(p, t, s));
    let mut d = seq::step(p, t, first.unwrap_or(0));
    if first.is_none() {
        d.note = 60;
        d.dest = [None; 2];
    }
    d.prob = 100;
    d.flam = 0;
    d.shift = 0;
    d
}

fn hit(p: &mut [u8], t: usize, step: usize, velocity: i32, flam: u8, tpl: StepData) {
    if !seq::step_on(p, t, step) {
        seq::toggle_step(p, t, step);
    }
    let d = StepData { velocity: velocity.clamp(1, 127) as u8, flam, ..tpl };
    seq::set_step(p, t, step, d);
}

fn clear(p: &mut [u8], t: usize, step: usize) {
    if seq::step_on(p, t, step) {
        seq::toggle_step(p, t, step);
    }
}

fn on_at(p: &[u8], pos: &Positions, t: usize, k: usize) -> bool {
    seq::step_on(p, t, pos[t][k])
}

/// Varies the coming bar of `p` in place: `amount` 0-1, only `tracks` (bit per track), with fill
/// `fill` (FILL_NAMES index) over its last steps.
pub fn vary(p: &mut [u8], pos: &Positions, amount: f32, tracks: u8, fill: Option<usize>, rng: &mut Rng) {
    let allowed = |t: usize| tracks & (1 << t) != 0;
    let a = amount.clamp(0.0, 1.0);

    // velocities breathe a little (the kick least); each pattern step once, however often it plays
    for t in (0..NUM_TRACKS).filter(|&t| allowed(t)) {
        let spread = if t == KICK { 6.0 } else { 14.0 } * a;
        let mut seen = 0u64;
        for k in 0..16 {
            let s = pos[t][k];
            if seen & (1 << s) != 0 || !seq::step_on(p, t, s) {
                continue;
            }
            seen |= 1 << s;
            let mut d = seq::step(p, t, s);
            let v = d.velocity as f32 + (rng.f() * 2.0 - 1.0) * spread;
            d.velocity = v.round().clamp(20.0, 127.0) as u8;
            seq::set_step(p, t, s, d);
        }
    }

    // closed hats: ghost notes on empty sixteenths, now and then a ratchet
    if allowed(CLOSED_HAT) && has_hits(p, CLOSED_HAT) {
        let tpl = template(p, CLOSED_HAT);
        if rng.chance(a * 0.6) {
            for _ in 0..1 + rng.pick(2) {
                let k = 1 + 2 * rng.pick(8);
                if !on_at(p, pos, CLOSED_HAT, k) {
                    hit(p, CLOSED_HAT, pos[CLOSED_HAT][k], rng.range(30, 55), 0, tpl);
                }
            }
        }
        if rng.chance(a * 0.3) {
            let k = 8 + rng.pick(8);
            if on_at(p, pos, CLOSED_HAT, k) {
                let mut d = seq::step(p, CLOSED_HAT, pos[CLOSED_HAT][k]);
                d.flam = 1 + rng.pick(2) as u8;
                seq::set_step(p, CLOSED_HAT, pos[CLOSED_HAT][k], d);
            }
        }
    }
    // open hat on an off-beat
    if allowed(OPEN_HAT) && has_hits(p, OPEN_HAT) && rng.chance(a * 0.35) {
        let k = [2, 6, 10, 14][rng.pick(4)];
        if !on_at(p, pos, OPEN_HAT, k) {
            let tpl = template(p, OPEN_HAT);
            hit(p, OPEN_HAT, pos[OPEN_HAT][k], rng.range(70, 100), 0, tpl);
        }
    }
    // snare and clap ghost notes around the backbeat
    for (t, chance, choices, lo, hi) in [(SNARE, 0.5, [3, 7, 9, 15], 28, 48), (CLAP, 0.3, [11, 15, 7, 3], 40, 60)] {
        if allowed(t) && has_hits(p, t) && rng.chance(a * chance) {
            let k = choices[rng.pick(4)];
            if !on_at(p, pos, t, k) {
                let tpl = template(p, t);
                hit(p, t, pos[t][k], rng.range(lo, hi), 0, tpl);
            }
        }
    }
    // percs: a hit moves by a sixteenth, or a ghost joins
    for t in [DRUM2, DRUM3] {
        if !allowed(t) || !has_hits(p, t) {
            continue;
        }
        if rng.chance(a * 0.4) {
            let k = rng.pick(16);
            let to = if rng.chance(0.5) { k + 1 } else { k.wrapping_sub(1) };
            if k % 4 != 0 && to < 16 && on_at(p, pos, t, k) && !on_at(p, pos, t, to) {
                let d = seq::step(p, t, pos[t][k]);
                clear(p, t, pos[t][k]);
                hit(p, t, pos[t][to], d.velocity as i32, d.flam, d);
            }
        }
        if rng.chance(a * 0.3) {
            let k = rng.pick(16);
            if !on_at(p, pos, t, k) {
                let tpl = template(p, t);
                hit(p, t, pos[t][k], rng.range(40, 70), 0, tpl);
            }
        }
    }

    if let Some(f) = fill {
        play_fill(p, pos, f, rng);
    }
}

/// Fills the Maestro can play with these tracks.
pub fn fills_for(tracks: u8) -> Vec<usize> {
    let allowed = |t: usize| tracks & (1 << t) != 0;
    (0..FILL_NAMES.len())
        .filter(|&f| match f {
            0 => allowed(SNARE),
            1 => allowed(CLAP),
            2 => allowed(DRUM2) || allowed(DRUM3),
            3 => allowed(CLOSED_HAT),
            _ => allowed(KICK) && allowed(SNARE),
        })
        .collect()
}

fn play_fill(p: &mut [u8], pos: &Positions, fill: usize, rng: &mut Rng) {
    match fill {
        // snare roll: ratchets getting faster and louder into the next bar
        0 => {
            let tpl = template(p, SNARE);
            if rng.chance(0.5) {
                hit(p, SNARE, pos[SNARE][10], 50, 0, tpl);
                hit(p, SNARE, pos[SNARE][11], 60, 0, tpl);
            }
            for (k, flam, v) in [(12, 1, 72), (13, 2, 88), (14, 3, 104), (15, 4, 122)] {
                hit(p, SNARE, pos[SNARE][k], v, flam, tpl);
            }
        }
        // clap stutters, after the Liveset Player's clap fills
        1 => {
            let tpl = template(p, CLAP);
            let shapes: [&[(usize, i32)]; 4] = [
                &[(12, 80), (13, 92), (14, 104), (15, 120)],
                &[(10, 96), (13, 104), (15, 116)],
                &[(12, 112), (14, 100), (15, 118)],
                &[(11, 62), (12, 118), (15, 112)],
            ];
            for &(k, v) in shapes[rng.pick(shapes.len())] {
                hit(p, CLAP, pos[CLAP][k], v, 0, tpl);
            }
        }
        // tom run down DRUM2 and DRUM3 (notes are the fill's only pitch change)
        2 => {
            let run: [(usize, usize, i32); 4] = [(12, DRUM2, 5), (13, DRUM2, 2), (14, DRUM3, 0), (15, DRUM3, -3)];
            for (k, t, semitones) in run {
                let mut tpl = template(p, t);
                tpl.note = (tpl.note as i32 + semitones).clamp(0, 127) as u8;
                hit(p, t, pos[t][k], 100 + k as i32, 0, tpl);
            }
        }
        // hat build: every sixteenth of the second half, rising, ratchets at the end, open hat last
        3 => {
            let tpl = template(p, CLOSED_HAT);
            for k in 8..16 {
                let flam = match k {
                    14 => 1,
                    15 => 2,
                    _ => 0,
                };
                hit(p, CLOSED_HAT, pos[CLOSED_HAT][k], 50 + (k as i32 - 8) * 10, flam, tpl);
            }
            let open = template(p, OPEN_HAT);
            hit(p, OPEN_HAT, pos[OPEN_HAT][15], 110, 0, open);
        }
        // kick drop: the last beat without kick, a short snare burst instead
        _ => {
            for k in 12..16 {
                clear(p, KICK, pos[KICK][k]);
            }
            let tpl = template(p, SNARE);
            hit(p, SNARE, pos[SNARE][14], 90, 1, tpl);
            hit(p, SNARE, pos[SNARE][15], 120, 3, tpl);
        }
    }
}

/// Sound settings the Maestro moves: (move, track, firmware parameter, how far at most). Decays and
/// colour only: no pitch, no FM, nothing of the kick.
const GESTURES: [(usize, usize, u16, f32); 10] = [
    (DECAY, CLOSED_HAT, 60, 12.0), // hihat decay closed
    (DECAY, OPEN_HAT, 61, 18.0),   // hihat decay open
    (DECAY, SNARE, 56, 12.0),      // snare decay
    (COLOUR, CLOSED_HAT, 42, 8.0), // hihat filter
    (COLOUR, SNARE, 28, 10.0),     // snare noise mix
    (COLOUR, SNARE, 40, 6.0),      // snare filter
    (COLOUR, CLAP, 58, 10.0),      // cymbal decay
    (COLOUR, CLAP, 41, 6.0),       // cymbal filter
    (COLOUR, DRUM2, 52, 6.0),      // drum 2 decay
    (COLOUR, DRUM3, 54, 6.0),      // drum 3 decay
];

/// The tracks an ENTROPIA MOD voice plays (the hihat voice plays both hat tracks).
fn voice_tracks(voice: usize) -> u8 {
    if voice == 5 {
        (1 << CLOSED_HAT) | (1 << OPEN_HAT)
    } else {
        1 << voice
    }
}

/// A master filter sweep: from the user's setting to `to` between two host beats, eased in.
#[derive(Clone, Copy)]
struct Sweep {
    to: f32,
    start: f64,
    end: f64,
}

#[derive(Clone, Copy)]
struct Gesture {
    /// the value the user set (what `sent` held when the Maestro last looked)
    base: i32,
    offset: f32,
}

/// The audio thread's side.
pub struct Maestro {
    /// the bar as it plays: the user's pattern with this bar's variation (and fill)
    work: Vec<u8>,
    /// the pattern `work` was made from this bar
    bar_pattern: Option<usize>,
    /// this bar ends the phrase
    fill_due: bool,
    /// patterns in the engine that hold a varied copy
    varied: u64,
    gestures: [Option<Gesture>; GESTURES.len()],
    /// ENTROPIA MOD settings (bit per entropia id) the Maestro has changed, and their values
    fx_held: u64,
    fx: [f32; ent::COUNT],
    sweep: Option<Sweep>,
    rng: Rng,
    last_fill: Option<usize>,
}

impl Default for Maestro {
    fn default() -> Self {
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
        Self {
            work: vec![0; PATTERN_BYTES],
            bar_pattern: None,
            fill_due: false,
            varied: 0,
            gestures: [None; GESTURES.len()],
            fx_held: 0,
            fx: [0.0; ent::COUNT],
            sweep: None,
            rng: Rng::new(seed),
            last_fill: None,
        }
    }
}

impl Maestro {
    /// Whether anything in the engine is the Maestro's rather than the user's.
    pub fn holding(&self) -> bool {
        self.varied != 0 || self.gestures.iter().any(Option::is_some) || self.fx_held != 0
    }

    /// Gives everything back: the user's patterns are uploaded again, the settings sent again.
    pub fn release(&mut self, engine: &mut Engine, seq: &SeqShared, sent: &[i32], sent_fx: &[f32]) {
        seq.dirty.fetch_or(self.varied, Ordering::AcqRel);
        self.varied = 0;
        for (g, &(_, _, param, _)) in self.gestures.iter_mut().zip(GESTURES.iter()) {
            if g.take().is_some() && sent[param as usize] >= 0 {
                engine.set_param(param, sent[param as usize] as u8);
            }
        }
        self.restore_fx(engine, sent_fx, u64::MAX);
    }

    /// Called right before the first tick of each host beat: the bar's variation on its first beat,
    /// a fill on its third (late enough that a track shorter than the bar has played the steps the
    /// fill reuses). `sent_fx` holds the user's ENTROPIA MOD settings, `entropia` whether it is on.
    #[allow(clippy::too_many_arguments)]
    pub fn on_beat(&mut self, beat: i64, engine: &mut Engine, shared: &MaestroShared, seq: &SeqShared, sent: &[i32], sent_fx: &[f32], entropia: bool) {
        if !shared.on.load(Ordering::Acquire) || seq.record.load(Ordering::Acquire) {
            return;
        }
        match beat.rem_euclid(4) {
            0 => {
                self.new_bar(beat.div_euclid(4), engine, shared, seq, sent);
                self.move_fx(beat, engine, shared, sent_fx, entropia, false);
            }
            2 => {
                let fill_now = shared.fill_now.swap(false, Ordering::AcqRel);
                if std::mem::take(&mut self.fill_due) || fill_now {
                    self.fill(engine, shared, seq, sent);
                    self.move_fx(beat, engine, shared, sent_fx, entropia, true);
                }
            }
            _ => {}
        }
    }

    /// Once per host block while the sequencer runs: the master filter sweep.
    pub fn tick(&mut self, beat: f64, engine: &mut Engine, sent_fx: &[f32]) {
        let Some(sweep) = self.sweep else { return };
        if beat < sweep.start {
            return;
        }
        let t = ((beat - sweep.start) / (sweep.end - sweep.start)).clamp(0.0, 1.0) as f32;
        let from = sent_fx[ent::FILTER as usize];
        self.set_fx(engine, ent::FILTER, from + (sweep.to - from) * t * t);
        if t >= 1.0 {
            // it stays there until the next bar's downbeat puts the user's setting back
            self.sweep = None;
        }
    }

    /// What the Maestro has an ENTROPIA MOD setting at, if it holds it.
    pub fn held_fx(&self, id: u16) -> Option<f32> {
        (self.fx_held & (1 << id) != 0).then(|| self.fx[id as usize])
    }

    fn set_fx(&mut self, engine: &mut Engine, id: u16, value: f32) {
        self.fx_held |= 1 << id;
        self.fx[id as usize] = value;
        engine.fx_set(id, value);
    }

    /// The user's values back for the held settings in `mask`.
    fn restore_fx(&mut self, engine: &mut Engine, sent_fx: &[f32], mask: u64) {
        let back = self.fx_held & mask;
        for id in (0..ent::COUNT).filter(|&id| back & (1 << id) != 0) {
            if sent_fx[id].is_finite() {
                engine.fx_set(id as u16, sent_fx[id]);
            }
        }
        self.fx_held &= !back;
        if mask & (1 << ent::FILTER) != 0 {
            self.sweep = None;
        }
    }

    /// ENTROPIA MOD moves at the start of a bar (`fill` false) or of a fill.
    fn move_fx(&mut self, beat: i64, engine: &mut Engine, shared: &MaestroShared, sent_fx: &[f32], entropia: bool, fill: bool) {
        if !entropia {
            self.restore_fx(engine, sent_fx, u64::MAX);
            return;
        }
        let tracks = shared.tracks.load(Ordering::Acquire);
        let value = |id: u16| sent_fx[id as usize];
        // a new bar starts from the user's settings
        if !fill {
            self.restore_fx(engine, sent_fx, u64::MAX);
        }

        // sends: one voice thrown into the delay or the reverb; at a fill the snare or clap
        if let Some((chance, depth)) = shared.moves[SENDS].get() {
            if self.rng.chance(chance) {
                let voices: Vec<usize> = if fill { vec![3, 4] } else { (1..ent::VOICES.len()).collect() };
                let voices: Vec<usize> = voices.into_iter().filter(|&v| tracks & voice_tracks(v) != 0).collect();
                if !voices.is_empty() {
                    let voice = voices[self.rng.pick(voices.len())];
                    let bus = if fill || self.rng.chance(0.5) { 0 } else { 2 };
                    let id = ent::send_id(bus, voice);
                    let base = value(id);
                    let throw = base + (1.0 - base) * depth * (0.5 + 0.5 * self.rng.f());
                    self.set_fx(engine, id, throw);
                }
            }
        }

        // the ring LFO
        if let Some((chance, depth)) = shared.moves[LFO].get() {
            if self.rng.chance(chance) {
                let synced = value(ent::RING_LFO_SYNC) >= 1.0;
                let last = ent::LFO_SYNCS.len() as i32 - 1;
                if synced {
                    let reach = 1 + (depth * 3.0) as i32;
                    let to = if fill { last - self.rng.pick(2) as i32 } else { value(ent::RING_LFO_SYNC) as i32 + self.rng.range(-reach, reach) };
                    self.set_fx(engine, ent::RING_LFO_SYNC, to.clamp(1, last) as f32);
                } else {
                    let rate = value(ent::RING_LFO_RATE);
                    let to = if fill { rate + 0.35 * depth } else { rate + (self.rng.f() * 2.0 - 1.0) * 0.3 * depth };
                    self.set_fx(engine, ent::RING_LFO_RATE, to.clamp(0.0, 1.0));
                }
                let d = value(ent::RING_LFO_DEPTH);
                let to = d + (1.0 - d) * depth * if fill { 0.8 } else { 0.2 + 0.5 * self.rng.f() };
                self.set_fx(engine, ent::RING_LFO_DEPTH, to.min(1.0));
                if self.rng.chance(0.3 * depth) {
                    let wave = self.rng.pick(ent::LFO_WAVES.len()) as f32;
                    self.set_fx(engine, ent::RING_LFO_WAVE, wave);
                }
            }
        }

        // the master filter: a sweep over the fill's two beats, now and then a short one into a bar's last beat
        if let Some((chance, depth)) = shared.moves[FILTER].get() {
            // a bar that ends in a fill leaves its sweep to the fill
            let fill_coming = self.fill_due || shared.fill_now.load(Ordering::Acquire);
            let p = match (fill, fill_coming) {
                (true, _) => chance,
                (false, true) => 0.0,
                (false, false) => chance * 0.15,
            };
            if self.sweep.is_none() && self.rng.chance(p) {
                let base = value(ent::FILTER);
                let to = if self.rng.chance(0.6) { base.max(0.5) + 0.42 * depth } else { base.min(0.5) - 0.42 * depth };
                let start = if fill { beat as f64 } else { beat as f64 + 3.0 };
                self.sweep = Some(Sweep { to: to.clamp(0.0, 1.0), start, end: beat.div_euclid(4) as f64 * 4.0 + 4.0 });
            }
        }
    }

    /// The user's pattern now playing, into `work`; false if the panel holds the lock right now.
    fn take_pattern(&mut self, engine: &Engine, seq: &SeqShared) -> Option<usize> {
        let active = engine.seq_active_pattern() as usize % NUM_PATTERNS;
        let patterns = seq.patterns.try_lock().ok()?;
        self.work.copy_from_slice(&patterns[active]);
        drop(patterns);
        // a pattern varied earlier that is no longer playing gets its own steps back
        let stale = self.varied & !(1 << active);
        if stale != 0 {
            seq.dirty.fetch_or(stale, Ordering::AcqRel);
            self.varied &= !stale;
        }
        Some(active)
    }

    fn new_bar(&mut self, bar: i64, engine: &mut Engine, shared: &MaestroShared, seq: &SeqShared, sent: &[i32]) {
        let phrase = shared.phrase.load(Ordering::Acquire) as i64;
        let bar_in_phrase = if phrase > 0 { bar.rem_euclid(phrase) } else { 0 };
        shared.bar.store(bar_in_phrase as u8, Ordering::Release);
        self.fill_due = phrase > 0 && bar_in_phrase == phrase - 1;
        self.bar_pattern = None;
        let tracks = shared.tracks.load(Ordering::Acquire);
        let Some(active) = self.take_pattern(engine, seq) else { return };
        let pos = positions(engine, &self.work, 0);
        let amount = shared.variation.load(Ordering::Acquire) as f32 / 100.0;
        vary(&mut self.work, &pos, amount, tracks, None, &mut self.rng);
        engine.seq_read_pattern(active as u8, &self.work);
        self.varied |= 1 << active;
        self.bar_pattern = Some(active);
        self.move_settings(engine, shared, sent, tracks, false);
    }

    fn fill(&mut self, engine: &mut Engine, shared: &MaestroShared, seq: &SeqShared, sent: &[i32]) {
        let tracks = shared.tracks.load(Ordering::Acquire);
        let choices = fills_for(tracks);
        let fresh: Vec<usize> = choices.iter().copied().filter(|f| Some(*f) != self.last_fill).collect();
        let pool = if fresh.is_empty() { &choices } else { &fresh };
        if pool.is_empty() {
            return;
        }
        let fill = pool[self.rng.pick(pool.len())];
        // on top of this bar's variation, unless the pattern changed since the bar began
        let active = engine.seq_active_pattern() as usize % NUM_PATTERNS;
        if self.bar_pattern != Some(active) && self.take_pattern(engine, seq).is_none() {
            return;
        }
        let pos = positions(engine, &self.work, 8);
        play_fill(&mut self.work, &pos, fill, &mut self.rng);
        engine.seq_read_pattern(active as u8, &self.work);
        self.varied |= 1 << active;
        self.bar_pattern = Some(active);
        self.last_fill = Some(fill);
        shared.last_fill.store(fill as u8 + 1, Ordering::Release);
        self.move_settings(engine, shared, sent, tracks, true);
    }

    /// DECAY and COLOUR: when a bar's roll says so, each setting of the move goes somewhere around the
    /// user's value (in the bars between it is the user's own); a fill opens them up.
    fn move_settings(&mut self, engine: &mut Engine, shared: &MaestroShared, sent: &[i32], tracks: u8, fill: bool) {
        let rolls: [Option<(bool, f32)>; 2] = [DECAY, COLOUR].map(|m| shared.moves[m].get().map(|(chance, depth)| (self.rng.chance(chance), depth)));
        for (g, &(m, track, param, reach)) in self.gestures.iter_mut().zip(GESTURES.iter()) {
            let user = sent[param as usize];
            let roll = rolls[if m == DECAY { 0 } else { 1 }];
            let Some((moves, depth)) = roll.filter(|_| tracks & (1 << track) != 0 && user >= 0) else {
                if g.take().is_some() && user >= 0 {
                    engine.set_param(param, user as u8);
                }
                continue;
            };
            let gesture = g.get_or_insert(Gesture { base: user, offset: 0.0 });
            // the user turned it: their value is the new centre
            gesture.base = user;
            let limit = reach * depth;
            gesture.offset = match (moves, fill) {
                (true, true) => limit,
                (true, false) => (self.rng.f() * 2.0 - 1.0) * limit,
                // a fill the roll passed over keeps the bar's value
                (false, true) => gesture.offset,
                (false, false) => 0.0,
            };
            let value = (gesture.base as f32 + gesture.offset).round().clamp(0.0, 127.0) as u8;
            engine.set_param(param, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight() -> Positions {
        let mut pos = [[0; 16]; NUM_TRACKS];
        for row in pos.iter_mut() {
            for (k, s) in row.iter_mut().enumerate() {
                *s = k;
            }
        }
        pos
    }

    fn groove() -> Vec<u8> {
        let mut p = seq::empty_pattern();
        for k in [0, 4, 8, 12] {
            seq::toggle_step(&mut p, KICK, k);
        }
        for k in [4, 12] {
            seq::toggle_step(&mut p, SNARE, k);
        }
        for k in (2..16).step_by(4) {
            seq::toggle_step(&mut p, CLOSED_HAT, k);
        }
        for k in [3, 10] {
            seq::toggle_step(&mut p, DRUM2, k);
        }
        p
    }

    #[test]
    fn variations_leave_the_kick_and_the_pitch_alone() {
        let base = groove();
        for seed in 0..200 {
            let mut p = base.clone();
            let mut rng = Rng::new(seed);
            vary(&mut p, &straight(), 1.0, 0x7f, None, &mut rng);
            for s in 0..64 {
                assert_eq!(seq::step_on(&p, KICK, s), seq::step_on(&base, KICK, s), "kick step {s}, seed {seed}");
                for t in 0..NUM_TRACKS {
                    assert_eq!(seq::step(&p, t, s).note, seq::step(&base, t, s).note, "note of track {t} step {s}");
                }
            }
            // velocities move, but not far
            for k in [0, 4, 8, 12] {
                let (a, b) = (seq::step(&p, KICK, k).velocity as i32, seq::step(&base, KICK, k).velocity as i32);
                assert!((a - b).abs() <= 6, "kick velocity {a} vs {b}");
            }
        }
    }

    #[test]
    fn tracks_left_out_are_untouched() {
        let base = groove();
        let mut p = base.clone();
        let mut rng = Rng::new(7);
        // only the hats
        vary(&mut p, &straight(), 1.0, 1 << CLOSED_HAT, Some(3), &mut rng);
        for t in (0..NUM_TRACKS).filter(|&t| t != CLOSED_HAT && t != OPEN_HAT) {
            for s in 0..64 {
                assert_eq!(seq::step(&p, t, s), seq::step(&base, t, s));
                assert_eq!(seq::step_on(&p, t, s), seq::step_on(&base, t, s));
            }
        }
        assert!(fills_for(1 << CLOSED_HAT) == vec![3]);
    }

    #[test]
    fn fills_land_at_the_end_of_the_bar() {
        let base = groove();
        // snare roll: ratchets rising over the last beat
        let mut p = base.clone();
        play_fill(&mut p, &straight(), 0, &mut Rng::new(1));
        let flams: Vec<u8> = (12..16).map(|k| seq::step(&p, SNARE, k).flam).collect();
        assert_eq!(flams, vec![1, 2, 3, 4]);
        assert!((12..16).all(|k| seq::step_on(&p, SNARE, k)));
        // kick drop: no kick on the last beat
        let mut p = base.clone();
        play_fill(&mut p, &straight(), 4, &mut Rng::new(1));
        assert!(!seq::step_on(&p, KICK, 12) && seq::step_on(&p, KICK, 8));
        // a track shorter than the bar wraps: a 13 step snare plays its steps 0-2 at the end
        let mut pos = straight();
        for (k, s) in pos[SNARE].iter_mut().enumerate() {
            *s = k % 13;
        }
        let mut p = base.clone();
        play_fill(&mut p, &pos, 0, &mut Rng::new(1));
        assert_eq!(seq::step(&p, SNARE, 2).flam, 4);
    }

    #[test]
    fn settings_round_trip() {
        let a = MaestroShared::default();
        a.on.store(true, Ordering::Release);
        a.variation.store(73, Ordering::Release);
        a.phrase.store(16, Ordering::Release);
        a.tracks.store(0b0101010, Ordering::Release);
        a.moves[FILTER].chance.store(12, Ordering::Release);
        a.moves[SENDS].on.store(false, Ordering::Release);
        let b = MaestroShared::default();
        b.from_field(&a.to_field());
        assert_eq!(b.to_field(), a.to_field());
        // a session saved before the moves: the gestures amount becomes DECAY and COLOUR depth
        let c = MaestroShared::default();
        c.from_field("1,250,3,5,255");
        assert!(c.to_field().starts_with("1,100,3,8,127,1,60,3,1,60,3,"), "{}", c.to_field());
    }
}
