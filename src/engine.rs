//! The C engine (engine/lxr_bridge.h) plus the resampler from the engine's fixed 44.14 kHz to the
//! host's sample rate.
//!
//! The firmware keeps all of its state in C globals. To give every plugin instance its own engine,
//! the engine is built as a dylib (build.rs) that is embedded here; each instance writes a copy to
//! a file of its own and loads that, so the copies' globals are separate.

use libloading::Library;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

pub const BLOCK_FRAMES: usize = 64;
/// The rate the firmware's tables and coefficients are tuned for (the hardware's real I2S rate).
pub const ENGINE_RATE: f64 = 44140.0;

static ENGINE_DYLIB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/lxr_engine.dylib"));
static NEXT_COPY: AtomicUsize = AtomicUsize::new(0);

macro_rules! engine_api {
    ($($name:ident: fn($($arg:ty),*) $(-> $ret:ty)?;)*) => {
        #[allow(dead_code)]
        struct Api {
            $($name: unsafe extern "C" fn($($arg),*) $(-> $ret)?,)*
        }
        impl Api {
            unsafe fn load(lib: &Library) -> Result<Self, libloading::Error> {
                Ok(Self { $($name: *lib.get(concat!(stringify!($name), "\0").as_bytes())?,)* })
            }
        }
    };
}

engine_api! {
    lxr_init: fn();
    lxr_set_param: fn(u16, u8);
    lxr_set_fader: fn(u8, f32);
    lxr_note_on: fn(u8, u8, u8);
    lxr_render: fn(*mut f32, *mut f32, *mut f32, *mut f32);
    lxr_take_triggered: fn() -> u8;
    lxr_set_bpm: fn(u16);
    lxr_seq_read_pattern: fn(u8, *const u8, usize) -> i32;
    lxr_seq_write_pattern: fn(u8, *mut u8);
    lxr_seq_start: fn();
    lxr_seq_stop: fn();
    lxr_seq_tick: fn();
    lxr_seq_skip_steps: fn(u32, u8);
    lxr_seq_set_pattern: fn(u8);
    lxr_seq_active_pattern: fn() -> u8;
    lxr_seq_pending_pattern: fn() -> u8;
    lxr_seq_step_index: fn(u8) -> u8;
    lxr_seq_shuffle: fn() -> f32;
    lxr_euklid: fn(u8, u8, u8) -> u64;
    lxr_seq_set_mutes: fn(u8);
    lxr_seq_set_record: fn(u8);
    lxr_seq_record: fn(u8, u8, u8);
    lxr_seq_set_song: fn(*const u8, u8, u8, u8);
    lxr_fx_set_mode: fn(u8);
    lxr_fx_set: fn(u16, f32);
    lxr_fx_clock: fn(f64, f32);
}

/// Value of a parameter in the firmware's init kit.
pub fn init_kit_value(idx: u16) -> u8 {
    crate::param_table::INIT_KIT.get(idx as usize).copied().unwrap_or(0)
}

pub struct Engine {
    api: Api,
    lib: Option<Library>,
    path: PathBuf,
}

/// Removes engine copies left behind by processes that ended without cleaning up (a crash).
fn remove_stale_copies(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mine = format!("engine-{}-", std::process::id());
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() > 24 * 3600);
        if name.starts_with("engine-") && !name.starts_with(&mine) && old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

impl Engine {
    /// Loads a private copy of the engine and initialises it with the init kit.
    pub fn new() -> Option<Self> {
        let dir = std::env::temp_dir().join("ELABS RLX");
        std::fs::create_dir_all(&dir).ok()?;
        remove_stale_copies(&dir);
        let copy = NEXT_COPY.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!("engine-{}-{copy}.dylib", std::process::id()));
        std::fs::write(&path, ENGINE_DYLIB).ok()?;
        // the file stays until the engine is dropped: as long as it exists no other copy can get its
        // inode, which the dynamic loader uses to tell libraries apart
        let loaded = unsafe { Library::new(&path) }.and_then(|lib| Ok((unsafe { Api::load(&lib) }?, lib)));
        match loaded {
            Ok((api, lib)) => {
                unsafe { (api.lxr_init)() };
                Some(Self { api, lib: Some(lib), path })
            }
            Err(e) => {
                nih_plug::nih_log!("loading the LXR engine failed: {e}");
                let _ = std::fs::remove_file(&path);
                None
            }
        }
    }

    pub fn set_param(&mut self, idx: u16, value: u8) {
        unsafe { (self.api.lxr_set_param)(idx, value) }
    }

    pub fn set_fader(&mut self, voice: u8, volume: f32) {
        unsafe { (self.api.lxr_set_fader)(voice, volume) }
    }

    pub fn note_on(&mut self, voice: u8, note: u8, velocity: u8) {
        unsafe { (self.api.lxr_note_on)(voice, note, velocity) }
    }

    /// Voices played since the last call, one bit per trigger (0-6), from MIDI or the sequencer.
    pub fn take_triggered(&mut self) -> u8 {
        unsafe { (self.api.lxr_take_triggered)() }
    }

    pub fn set_bpm(&mut self, bpm: u16) {
        unsafe { (self.api.lxr_set_bpm)(bpm) }
    }

    pub fn seq_read_pattern(&mut self, nr: u8, data: &[u8]) -> bool {
        unsafe { (self.api.lxr_seq_read_pattern)(nr, data.as_ptr(), data.len()) != 0 }
    }

    pub fn seq_write_pattern(&mut self, nr: u8) -> Vec<u8> {
        let mut out = vec![0u8; crate::seq::PATTERN_BYTES];
        unsafe { (self.api.lxr_seq_write_pattern)(nr, out.as_mut_ptr()) };
        out
    }

    #[cfg(test)]
    pub fn euklid(&mut self, length: u8, steps: u8, rotation: u8) -> u64 {
        unsafe { (self.api.lxr_euklid)(length, steps, rotation) }
    }

    pub fn seq_set_mutes(&mut self, mask: u8) {
        unsafe { (self.api.lxr_seq_set_mutes)(mask) }
    }

    pub fn seq_set_record(&mut self, on: bool) {
        unsafe { (self.api.lxr_seq_set_record)(on as u8) }
    }

    /// Records a hit into the playing pattern (when recording and running).
    pub fn seq_record(&mut self, track: u8, velocity: u8, note: u8) {
        unsafe { (self.api.lxr_seq_record)(track, velocity, note) }
    }

    pub fn seq_set_song(&mut self, patterns: &[u8], looped: bool, enabled: bool) {
        let count = patterns.len().min(63) as u8;
        unsafe { (self.api.lxr_seq_set_song)(patterns.as_ptr(), count, looped as u8, enabled as u8) }
    }

    pub fn seq_set_pattern(&mut self, nr: u8) {
        unsafe { (self.api.lxr_seq_set_pattern)(nr) }
    }

    pub fn seq_active_pattern(&self) -> u8 {
        unsafe { (self.api.lxr_seq_active_pattern)() }
    }

    pub fn seq_pending_pattern(&self) -> u8 {
        unsafe { (self.api.lxr_seq_pending_pattern)() }
    }

    pub fn seq_step_index(&self, track: u8) -> u8 {
        unsafe { (self.api.lxr_seq_step_index)(track) }
    }

    /// ENTROPIA MOD FX on or off (CLASSIC: the firmware's own FX bus).
    pub fn fx_set_mode(&mut self, entropia: bool) {
        unsafe { (self.api.lxr_fx_set_mode)(entropia as u8) }
    }

    /// An ENTROPIA FX setting, crate::entropia::Fx numbering (lxr_bridge.h LXR_FX_*).
    pub fn fx_set(&mut self, id: u16, value: f32) {
        unsafe { (self.api.lxr_fx_set)(id, value) }
    }

    /// The host's position in beats (None: stopped) and tempo, for the synced ring LFO.
    pub fn fx_clock(&mut self, beat: Option<f64>, bpm: f64) {
        unsafe { (self.api.lxr_fx_clock)(beat.unwrap_or(-1.0), bpm as f32) }
    }

    /// Renders one block; frames are [out1 L, out1 R, out2 L, out2 R].
    fn render(&mut self, out: &mut Vec<[f32; 4]>) {
        let mut ch = [[0.0f32; BLOCK_FRAMES]; 4];
        let [a, b, c, d] = &mut ch;
        unsafe { (self.api.lxr_render)(a.as_mut_ptr(), b.as_mut_ptr(), c.as_mut_ptr(), d.as_mut_ptr()) };
        out.extend((0..BLOCK_FRAMES).map(|i| [ch[0][i], ch[1][i], ch[2][i], ch[3][i]]));
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        drop(self.lib.take());
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Clocks the firmware sequencer from the host's musical position: one tick per 1/96 beat, a step
/// every 24 ticks. Every second step is delayed by the pattern's shuffle (up to 16 ticks), as the
/// firmware's own clock does.
pub struct SeqClock {
    running: bool,
    /// next tick to play, counted from the host's beat 0
    next_tick: i64,
    /// where the next block should start if the host keeps playing
    expected_beat: f64,
}

impl Default for SeqClock {
    fn default() -> Self {
        Self { running: false, next_tick: 0, expected_beat: 0.0 }
    }
}

impl SeqClock {
    const TICKS_PER_BEAT: f64 = 96.0;

    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Starts at `beat`, silently skipping the steps before it so patterns line up with the host.
    fn start(&mut self, engine: &mut Engine, beat: f64) {
        unsafe { (engine.api.lxr_seq_start)() };
        let t0 = (beat * Self::TICKS_PER_BEAT - 1e-6).ceil().max(0.0) as i64;
        let steps_done = (t0 + 23) / 24;
        unsafe { (engine.api.lxr_seq_skip_steps)(steps_done as u32, (t0 % 24) as u8) };
        self.next_tick = t0;
        self.running = true;
    }

    pub fn stop(&mut self, engine: &mut Engine) {
        if self.running {
            unsafe { (engine.api.lxr_seq_stop)() };
            self.running = false;
        }
    }

    /// Called at the start of a block the host is playing; restarts after a jump or loop.
    pub fn begin_block(&mut self, engine: &mut Engine, beat: f64) {
        if !self.running || (beat - self.expected_beat).abs() > 0.5 / Self::TICKS_PER_BEAT {
            self.stop(engine);
            self.start(engine, beat);
        }
    }

    pub fn end_block(&mut self, next_block_beat: f64) {
        self.expected_beat = next_block_beat;
    }

    fn tick_time(engine: &Engine, tick: i64) -> f64 {
        let grid = tick as f64 / Self::TICKS_PER_BEAT;
        let step = tick.div_euclid(24);
        if step % 2 == 1 {
            let delay = unsafe { (engine.api.lxr_seq_shuffle)() } as f64 * 16.0;
            grid.max((step * 24) as f64 / Self::TICKS_PER_BEAT + delay / Self::TICKS_PER_BEAT)
        } else {
            grid
        }
    }

    /// Plays every tick due at or before `beat`; `on_beat` runs right before the first tick of each
    /// host beat with the beat's number.
    pub fn advance_to(&mut self, engine: &mut Engine, beat: f64, mut on_beat: impl FnMut(&mut Engine, i64)) {
        while self.running && Self::tick_time(engine, self.next_tick) <= beat {
            if self.next_tick % Self::TICKS_PER_BEAT as i64 == 0 {
                on_beat(engine, self.next_tick / Self::TICKS_PER_BEAT as i64);
            }
            unsafe { (engine.api.lxr_seq_tick)() };
            self.next_tick += 1;
        }
    }
}

/// Pulls engine blocks on demand and interpolates them (4-point cubic Hermite) to the host rate.
pub struct Resampler {
    frames: Vec<[f32; 4]>,
    pos: f64,
    step: f64,
}

impl Resampler {
    pub fn new(host_rate: f32) -> Self {
        let mut r = Self {
            frames: Vec::with_capacity(4096),
            pos: 0.0,
            step: ENGINE_RATE / host_rate as f64,
        };
        r.reset();
        r
    }

    pub fn reset(&mut self) {
        self.frames.clear();
        // one frame of history in front of the first rendered frame
        self.frames.push([0.0; 4]);
        self.pos = 1.0;
    }

    pub fn next(&mut self, engine: &mut Engine) -> [f32; 4] {
        let i = self.pos as usize;
        while i + 2 >= self.frames.len() {
            engine.render(&mut self.frames);
        }
        let t = (self.pos - i as f64) as f32;
        let (y0, y1, y2, y3) = (self.frames[i - 1], self.frames[i], self.frames[i + 1], self.frames[i + 2]);
        let mut out = [0.0; 4];
        for c in 0..4 {
            let c1 = 0.5 * (y2[c] - y0[c]);
            let c2 = y0[c] - 2.5 * y1[c] + 2.0 * y2[c] - 0.5 * y3[c];
            let c3 = 0.5 * (y3[c] - y0[c]) + 1.5 * (y1[c] - y2[c]);
            out[c] = ((c3 * t + c2) * t + c1) * t + y1[c];
        }

        self.pos += self.step;
        // drop consumed frames, keeping one frame of history
        let consumed = self.pos as usize - 1;
        if consumed >= 1024 {
            self.frames.drain(..consumed);
            self.pos -= consumed as f64;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_crossings(x: &[f32]) -> usize {
        x.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count()
    }

    fn write_wav(path: &str, rate: u32, frames: &[[f32; 2]]) {
        let mut b = Vec::new();
        let data = (frames.len() * 4) as u32;
        b.extend(b"RIFF"); b.extend((36 + data).to_le_bytes()); b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes()); b.extend(1u16.to_le_bytes()); b.extend(2u16.to_le_bytes());
        b.extend(rate.to_le_bytes()); b.extend((rate * 4).to_le_bytes()); b.extend(4u16.to_le_bytes());
        b.extend(16u16.to_le_bytes()); b.extend(b"data"); b.extend(data.to_le_bytes());
        for f in frames {
            for s in f {
                b.extend(((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
            }
        }
        std::fs::write(path, b).unwrap();
    }

    // One test only: the engine is a process-wide singleton.
    #[test]
    fn renders_voices_at_host_rate_with_native_pitch() {
        let mut engine = Engine::new().expect("engine loads");

        // every instance has its own engine: a hit on one is silence on another
        {
            let mut other = Engine::new().expect("second engine loads");
            engine.note_on(0, 60, 127);
            let (mut a, mut b) = (Vec::new(), Vec::new());
            for _ in 0..20 {
                engine.render(&mut a);
                other.render(&mut b);
            }
            let peak = |v: &[[f32; 4]]| v.iter().flat_map(|f| f.iter()).fold(0.0f32, |m, s| m.max(s.abs()));
            assert!(peak(&a) > 0.1, "the triggered engine plays");
            assert_eq!(peak(&b), 0.0, "the other engine stays silent");
            // and a parameter set on one doesn't reach the other
            other.set_param(8, 100);
        }
        let mut fresh = Vec::new();
        engine.note_on(0, 60, 127);
        while fresh.len() < 11000 {
            engine.render(&mut fresh);
        }

        // Drum 1 at the native rate
        let mut native = Vec::new();
        engine.note_on(0, 60, 100);
        while native.len() < 11000 {
            engine.render(&mut native);
        }
        let native_l: Vec<f32> = native[..11000].iter().map(|f| f[0]).collect();

        // the same hit through the resampler at 48 kHz
        let mut rs = Resampler::new(48000.0);
        engine.note_on(0, 60, 100);
        let host_len = (11000.0 * 48000.0 / ENGINE_RATE) as usize;
        let host_l: Vec<f32> = (0..host_len).map(|_| rs.next(&mut engine)[0]).collect();

        let (zn, zh) = (zero_crossings(&native_l), zero_crossings(&host_l));
        assert!(zn > 20, "drum 1 should oscillate, got {zn} zero crossings");
        // a kick is only ~25 cycles long here, so allow a couple of crossings of slack
        assert!(zn.abs_diff(zh) <= 2, "pitch changed by resampling: {zn} vs {zh}");

        // a parameter change reaches the engine: coarse tune +12 semitones on Drum 1 doubles the pitch
        let coarse = init_kit_value(8);
        engine.set_param(8, coarse + 12);
        engine.note_on(0, 60, 100);
        let up: Vec<f32> = (0..host_len).map(|_| rs.next(&mut engine)[0]).collect();
        let zu = zero_crossings(&up);
        let octave = zu as f32 / zh as f32;
        assert!((1.8..2.2).contains(&octave), "coarse +12 should double the pitch: {zh} -> {zu}");
        engine.set_param(8, coarse);

        // demo file: every trigger once at 48 kHz
        let mut demo = Vec::new();
        for v in 0..7u8 {
            engine.note_on(v, 60, 110);
            for _ in 0..(48000 * 6 / 10) {
                let f = rs.next(&mut engine);
                demo.push([f[0], f[1]]);
            }
        }
        let peak = demo.iter().flat_map(|f| f.iter()).fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.1 && peak <= 1.0, "peak {peak}");
        write_wav(concat!(env!("CARGO_MANIFEST_DIR"), "/target/lxr_demo_48k.wav"), 48000, &demo);

        // a two bar beat on a few of the built-in kits, loaded like the panel does
        let kits = crate::kits::all_kits();
        let mut beat = Vec::new();
        for name in ["TEKKNO", "BOOM808", "ELECTRO", "INDUSTRL"] {
            let kit = kits.iter().find(|k| k.name == name).expect("kit");
            for info in crate::param_table::PARAMS {
                let v = crate::kits::kit_value(&kit.data, info.index, info.min, info.max, &info.display);
                engine.set_param(info.index, v as u8);
            }
            let sixteenth = 48000 * 60 / 124 / 4;
            for step in 0..32 {
                if step % 4 == 0 { engine.note_on(0, 60, 120); }
                if step % 8 == 4 { engine.note_on(3, 60, 110); }
                if step % 2 == 1 { engine.note_on(5, 60, 90); }
                if step % 16 == 14 { engine.note_on(6, 60, 100); }
                if step % 16 == 10 { engine.note_on(1, 60, 100); }
                for _ in 0..sixteenth {
                    let f = rs.next(&mut engine);
                    beat.push([f[0] + f[2], f[1] + f[3]]);
                }
            }
        }
        let beat_peak = beat.iter().flat_map(|f| f.iter()).fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(beat_peak > 0.1, "kits should make sound, peak {beat_peak}");
        assert!(beat.iter().flat_map(|f| f.iter()).all(|s| s.is_finite()));
        write_wav(concat!(env!("CARGO_MANIFEST_DIR"), "/target/lxr_kits_demo_48k.wav"), 48000, &beat);

        // ---- sequencer
        // the plugin's euclid generator gives exactly the firmware's steps, for every setting the
        // panel allows (rotation up to the length)
        let mut checked = 0;
        for length in 1..=16u8 {
            for steps in 1..=16u8 {
                for rotation in 0..=length.min(15) {
                    let e = crate::seq::Euclid { length, steps, rotation };
                    assert_eq!(
                        crate::seq::euclid_bits(e),
                        engine.euklid(length, steps, rotation),
                        "{e:?}"
                    );
                    checked += 1;
                }
            }
        }
        println!("euclid: {checked} settings match the firmware");
        // the plugin's empty pattern is exactly what the firmware's seq_clearPattern() makes
        assert_eq!(engine.seq_write_pattern(40), crate::seq::empty_pattern());
        // every built-in pattern survives a trip through the firmware unchanged
        for project in 0..crate::project_table::PROJECTS.len() {
            for nr in 0..crate::seq::NUM_PATTERNS {
                let p = crate::seq::project_pattern(project, nr);
                assert!(engine.seq_read_pattern(nr as u8, &p));
                assert_eq!(engine.seq_write_pattern(nr as u8), p, "project {project} pattern {nr}");
            }
        }

        // play ELABS pattern 1 with its kit, the host at 124 BPM from beat 0
        let pattern = crate::seq::project_pattern(0, 0);
        let (kit_name, kit) = crate::seq::project_kit(0, crate::seq::kit_number(&pattern)).expect("kit");
        for info in crate::param_table::PARAMS {
            let v = crate::kits::kit_value(kit, info.index, info.min, info.max, &info.display);
            engine.set_param(info.index, v as u8);
        }
        assert!(engine.seq_read_pattern(0, &pattern));
        engine.seq_set_pattern(0);
        let used: u8 = (0..7).filter(|&t| (0..64).any(|s| crate::seq::step_on(&pattern, t, s))).map(|t| 1 << t).sum();
        assert!(used != 0, "pattern 1 has steps");

        let mut clock = SeqClock::default();
        let (sr, bpm) = (48000.0, 124.0);
        let beats_per_sample = bpm / 60.0 / sr;
        let mut song = Vec::new();
        let mut played = 0u8;
        let mut first_hit = None;
        engine.take_triggered();
        for block in 0..(16.0 * 60.0 / bpm * sr / 512.0) as usize {
            let beat = block as f64 * 512.0 * beats_per_sample;
            clock.begin_block(&mut engine, beat);
            for i in 0..512 {
                clock.advance_to(&mut engine, beat + i as f64 * beats_per_sample, |_, _| {});
                let f = rs.next(&mut engine);
                song.push([f[0] + f[2], f[1] + f[3]]);
            }
            clock.end_block(beat + 512.0 * beats_per_sample);
            let t = engine.take_triggered();
            if t != 0 && first_hit.is_none() {
                first_hit = Some(block);
            }
            played |= t;
        }
        clock.stop(&mut engine);
        assert_eq!(played & used, used, "every track with steps plays (played {played:07b}, used {used:07b})");
        assert!(first_hit.is_some_and(|b| b < 4), "the pattern starts right away");
        assert!(song.iter().flat_map(|f| f.iter()).all(|s| s.is_finite()));
        write_wav(concat!(env!("CARGO_MANIFEST_DIR"), "/target/lxr_sequencer_demo_48k.wav"), 48000, &song);

        // shuffle: closed hihat on every step; the second step moves 16 of its 24 ticks late at
        // full shuffle (120 BPM, 48 kHz: a step is 6000 samples)
        let hit_times = |engine: &mut Engine, rs: &mut Resampler, shuffle: u8| -> Vec<usize> {
            let mut p = crate::seq::empty_pattern();
            for step in 0..16 {
                crate::seq::toggle_step(&mut p, 5, step);
            }
            crate::seq::set_shuffle(&mut p, shuffle);
            engine.seq_read_pattern(1, &p);
            engine.seq_set_pattern(1);
            let mut clock = SeqClock::default();
            let bps = 2.0 / 48000.0;
            let mut hits = Vec::new();
            engine.take_triggered();
            for block in 0..(3 * 6000 / 16) {
                let beat = (block * 16) as f64 * bps;
                clock.begin_block(engine, beat);
                for i in 0..16 {
                    clock.advance_to(engine, beat + i as f64 * bps, |_, _| {});
                    rs.next(engine);
                }
                clock.end_block(beat + 16.0 * bps);
                if engine.take_triggered() & (1 << 5) != 0 {
                    hits.push(block * 16);
                }
            }
            clock.stop(engine);
            hits
        };
        let straight = hit_times(&mut engine, &mut rs, 0);
        let swung = hit_times(&mut engine, &mut rs, 127);
        println!("shuffle: straight hits {straight:?}, full shuffle {swung:?}");
        assert!(straight.len() >= 3 && swung.len() >= 3);
        assert!((straight[1] as i64 - 6000).abs() < 200, "step 2 on the grid without shuffle");
        assert!((swung[1] as i64 - 10000).abs() < 200, "step 2 is 2/3 of a step late with full shuffle");
        assert!((swung[2] as i64 - 12000).abs() < 200, "step 3 back on the grid");

        // ---- step settings, mutes, recording, song mode (120 BPM, 48 kHz, a step = 6000 samples)
        use crate::seq::{self as sq, StepData};
        // plays `steps` sixteenths from beat 0; returns the sample of every hit per track
        let play = |engine: &mut Engine, rs: &mut Resampler, steps: usize, during: &mut dyn FnMut(&mut Engine, usize)| {
            let mut clock = SeqClock::default();
            let bps = 2.0 / 48000.0;
            let mut hits = vec![Vec::new(); 7];
            engine.take_triggered();
            for block in 0..(steps * 6000 / 16) {
                let beat = (block * 16) as f64 * bps;
                clock.begin_block(engine, beat);
                for i in 0..16 {
                    clock.advance_to(engine, beat + i as f64 * bps, |_, _| {});
                    rs.next(engine);
                }
                clock.end_block(beat + 16.0 * bps);
                during(engine, block * 16);
                let t = engine.take_triggered();
                for (track, h) in hits.iter_mut().enumerate() {
                    if t & (1 << track) != 0 {
                        h.push(block * 16);
                    }
                }
            }
            clock.stop(engine);
            hits
        };

        // flam x4 on step 1 of track 0; track 3 on step 1 too
        let mut p = sq::empty_pattern();
        sq::toggle_step(&mut p, 0, 0);
        sq::toggle_step(&mut p, 3, 0);
        let mut d = sq::step(&p, 0, 0);
        d.flam = 3; // x4
        sq::set_step(&mut p, 0, 0, d);
        engine.seq_read_pattern(2, &p);
        engine.seq_set_pattern(2);
        let hits = play(&mut engine, &mut rs, 4, &mut |_, _| {});
        println!("flam x4 hits: {:?}", hits[0]);
        assert_eq!(hits[0].len(), 4, "flam x4 plays 4 hits in the step");
        assert_eq!(hits[3].len(), 1);

        // mutes: track 3 muted, track 0 still plays
        engine.seq_set_mutes(1 << 3);
        let hits = play(&mut engine, &mut rs, 4, &mut |_, _| {});
        assert!(hits[3].is_empty(), "muted track is silent");
        assert!(!hits[0].is_empty());
        engine.seq_set_mutes(0);

        // recording: a snare hit near step 5 is written into the playing pattern on step 5
        engine.seq_set_record(true);
        play(&mut engine, &mut rs, 8, &mut |engine, sample| {
            if sample == 4 * 6000 + 1200 {
                engine.seq_record(3, 99, 60);
            }
        });
        engine.seq_set_record(false);
        let recorded = engine.seq_write_pattern(2);
        assert!(sq::step_on(&recorded, 3, 4), "recorded hit is on step 5");
        assert_eq!(sq::step(&recorded, 3, 4).velocity, 99);
        assert!(sq::step_on(&recorded, 3, 0), "the rest stays");

        // song mode: pattern 2 then 3 (a 16 step bar each), looping
        let mut p3 = sq::empty_pattern();
        sq::toggle_step(&mut p3, 5, 0);
        engine.seq_read_pattern(3, &p3);
        engine.seq_set_song(&[2, 3], true, true);
        let mut actives = Vec::new();
        play(&mut engine, &mut rs, 48, &mut |engine, sample| {
            if sample % 6000 == 2992 {
                let a = engine.seq_active_pattern();
                if actives.last() != Some(&a) {
                    actives.push(a);
                }
            }
        });
        println!("song order: {actives:?}");
        assert_eq!(actives, vec![2, 3, 2], "the song plays 2, 3 and loops");
        engine.seq_set_song(&[], true, false);

        // the Maestro: a snare roll in the first bar (fill now), the user's steps in the second,
        // on a 13 step snare track that wraps inside the bar (its steps 0-2 play again at the bar's end:
        // the fill only goes in once they have played)
        use std::sync::atomic::Ordering;
        let maestro_shared = crate::maestro::MaestroShared::default();
        maestro_shared.on.store(true, Ordering::Release);
        maestro_shared.variation.store(0, Ordering::Release);
        for m in &maestro_shared.moves {
            m.on.store(false, Ordering::Release);
        }
        maestro_shared.tracks.store(1 << 3, Ordering::Release);
        maestro_shared.fill_now.store(true, Ordering::Release);
        let seq_shared = crate::seq::SeqShared::default();
        let mut p4 = sq::empty_pattern();
        sq::set_track_length(&mut p4, 3, 13);
        sq::toggle_step(&mut p4, 3, 4);
        sq::toggle_step(&mut p4, 3, 12);
        seq_shared.patterns.lock().unwrap()[4] = p4.clone();
        engine.seq_read_pattern(4, &p4);
        engine.seq_set_pattern(4);
        let mut maestro = crate::maestro::Maestro::default();
        let sent = [-1; crate::param_table::NUM_ENGINE_PARAMS];
        let sent_fx = [0.5; crate::entropia::COUNT];
        let mut clock = SeqClock::default();
        let bps = 2.0 / 48000.0;
        let mut snare = Vec::new();
        engine.take_triggered();
        for block in 0..(32 * 6000 / 16) {
            let beat = (block * 16) as f64 * bps;
            clock.begin_block(&mut engine, beat);
            for i in 0..16 {
                clock.advance_to(&mut engine, beat + i as f64 * bps, |engine, beat| {
                    maestro.on_beat(beat, engine, &maestro_shared, &seq_shared, &sent, &sent_fx, false);
                });
                rs.next(&mut engine);
            }
            clock.end_block(beat + 16.0 * bps);
            if engine.take_triggered() & (1 << 3) != 0 {
                snare.push(block * 16 / 6000);
            }
        }
        clock.stop(&mut engine);
        println!("maestro snare hits by step: {snare:?}");
        let last_beat = snare.iter().filter(|&&s| (12..16).contains(&s)).count();
        assert!(last_beat >= 2 + 3 + 4 + 6, "the roll's ratchets x2 x3 x4 x6 play ({last_beat} hits)");
        assert_eq!(snare.iter().filter(|&&s| s < 8).copied().collect::<Vec<_>>(), vec![4], "the bar starts as the user wrote it (the roll may begin on step 11)");
        let second_bar: Vec<usize> = snare.iter().copied().filter(|&s| s >= 16).collect();
        assert_eq!(second_bar, vec![17, 25, 30], "the next bar plays the user's steps again");
        assert_eq!(seq_shared.patterns.lock().unwrap()[4], p4, "the user's pattern is never changed");
        maestro.release(&mut engine, &seq_shared, &sent, &sent_fx);
        assert_eq!(seq_shared.dirty.load(Ordering::Acquire), 1 << 4, "released: pattern 4 goes back");

        // its master filter move (ENTROPIA MOD), every fill: a sweep over the fill's two beats that
        // the next downbeat takes back
        maestro_shared.moves[crate::maestro::FILTER].on.store(true, Ordering::Release);
        maestro_shared.moves[crate::maestro::FILTER].chance.store(100, Ordering::Release);
        maestro_shared.moves[crate::maestro::FILTER].depth.store(100, Ordering::Release);
        let mut filter_at = Vec::new();
        for beat in 0..5 {
            if beat == 0 {
                maestro_shared.fill_now.store(true, Ordering::Release);
            }
            maestro.on_beat(beat, &mut engine, &maestro_shared, &seq_shared, &sent, &sent_fx, true);
            for half in [0.0, 0.5] {
                maestro.tick(beat as f64 + half, &mut engine, &sent_fx);
                filter_at.push(maestro.held_fx(crate::entropia::FILTER));
            }
        }
        println!("maestro filter by half beat: {filter_at:?}");
        assert!(filter_at[..4].iter().all(Option::is_none), "nothing before the fill");
        let swept = filter_at[7].expect("swept on the fill's last half beat");
        assert!((swept - 0.5).abs() > 0.2, "far from the user's 0.5: {swept}");
        assert!(filter_at[4].is_some_and(|v| (v - 0.5).abs() < 0.01), "it starts from the user's value");
        assert_eq!(filter_at[8], None, "the downbeat puts the user's filter back");
        maestro.release(&mut engine, &seq_shared, &sent, &sent_fx);
        assert!(!maestro.holding());

        // ---- ENTROPIA MOD FX, on the closed hihat (short: anything after half a second is the FX)
        use crate::entropia as ent;
        let render = |engine: &mut Engine, rs: &mut Resampler, hit: bool, secs: f32| -> Vec<[f32; 2]> {
            if hit {
                engine.note_on(5, 60, 120);
            }
            (0..(48000.0 * secs) as usize)
                .map(|_| {
                    let f = rs.next(engine);
                    [f[0] + f[2], f[1] + f[3]]
                })
                .collect()
        };
        let rms = |s: &[[f32; 2]]| (s.iter().map(|f| f[0] * f[0] + f[1] * f[1]).sum::<f32>() / (2 * s.len()) as f32).sqrt();
        let (head, tail) = (0..9600, 24000..96000);
        // the init kit's hat (a known sound, whatever kit played last), the kit's own FX off
        // (ENTROPIA replaces it), a short closed hat
        for info in crate::param_table::PARAMS {
            let v = crate::kits::kit_value(crate::param_table::INIT_KIT, info.index, info.min, info.max, &info.display);
            engine.set_param(info.index, v as u8);
        }
        engine.set_param(crate::FX_TYPE_PARAM, 0);
        engine.set_param(60, 20);
        // and nothing else: a voice of the last kit may still ring
        for v in 0..5 {
            engine.set_fader(v, 0.0);
        }
        render(&mut engine, &mut rs, false, 1.0);
        let classic = render(&mut engine, &mut rs, true, 2.0);
        engine.fx_set_mode(true);
        render(&mut engine, &mut rs, false, 0.5);
        let dry = render(&mut engine, &mut rs, true, 2.0);
        let (a, b) = (rms(&classic[head.clone()]), rms(&dry[head.clone()]));
        println!("entropia: classic {a:.4}, all sends off {b:.4}, dry tail {:.6}", rms(&dry[tail.clone()]));
        assert!((a - b).abs() < 0.15 * a, "nothing sent, nothing changes: {a} vs {b}");
        let mut entropia_demo = dry.clone();
        let mut fx_tail = |engine: &mut Engine, rs: &mut Resampler, bus: usize, name: &str| {
            engine.fx_set(ent::send_id(bus, 5), 1.0);
            let wet = render(engine, rs, true, 2.0);
            engine.fx_set(ent::send_id(bus, 5), 0.0);
            render(engine, rs, false, 4.0);
            assert!(wet.iter().flatten().all(|s| s.is_finite()), "{name}");
            entropia_demo.extend_from_slice(&wet);
            (rms(&wet[head.clone()]), rms(&wet[tail.clone()]))
        };
        // the hat is all highs: an open tone and some feedback, or its repeats are filtered away
        engine.fx_set(ent::DELAY_TONE, 1.0);
        engine.fx_set(ent::DELAY_FEEDBACK, 0.6);
        let (_, delay_tail) = fx_tail(&mut engine, &mut rs, 0, "delay");
        let (ring_head, _) = fx_tail(&mut engine, &mut rs, 1, "ring");
        let (_, verb_tail) = fx_tail(&mut engine, &mut rs, 2, "reverb");
        println!("entropia: delay tail {delay_tail:.5}, ring head {ring_head:.4}, reverb tail {verb_tail:.5}");
        let dry_tail = rms(&dry[tail.clone()]);
        assert!(delay_tail > 10.0 * dry_tail + 1e-4, "the delay repeats the hat");
        assert!(verb_tail > 10.0 * dry_tail + 1e-4, "the reverb rings on");
        assert!((ring_head - b).abs() > 0.02 * b, "the ring modulator changes the sound");
        // master filter: the hat through a closed low pass, then a high pass, loses most of itself
        for (setting, name) in [(0.05, "low pass"), (0.95, "high pass")] {
            engine.fx_set(ent::FILTER, setting);
            render(&mut engine, &mut rs, false, 0.2);
            let filtered = render(&mut engine, &mut rs, true, 0.5);
            let level = rms(&filtered[head.clone()]);
            println!("entropia: {name} {level:.4}");
            assert!(filtered.iter().flatten().all(|s| s.is_finite()));
            if name == "low pass" {
                assert!(level < 0.3 * b, "a closed low pass takes the hat away");
            }
            entropia_demo.extend_from_slice(&filtered);
        }
        engine.fx_set(ent::FILTER, 0.5);
        // drive and compressor on the master
        engine.fx_set(ent::DRIVE_MIX, 1.0);
        engine.fx_set(ent::DRIVE, 0.8);
        engine.fx_set(ent::COMP_MIX, 1.0);
        let driven = render(&mut engine, &mut rs, true, 0.5);
        assert!(driven.iter().flatten().all(|s| s.is_finite()) && rms(&driven) > 0.0);
        entropia_demo.extend_from_slice(&driven);
        engine.fx_set(ent::DRIVE_MIX, 0.0);
        engine.fx_set(ent::COMP_MIX, 0.0);
        engine.fx_set_mode(false);
        for v in 0..5 {
            engine.set_fader(v, 1.0);
        }
        write_wav(concat!(env!("CARGO_MANIFEST_DIR"), "/target/lxr_entropia_demo_48k.wav"), 48000, &entropia_demo);
        println!("sequencer: kit {kit_name}, tracks used {used:07b}, played {played:07b}");
        println!("zero crossings native {zn}, 48k {zh}, +12 {zu}; demo peak {peak:.3}");
    }
}
