//! The ELABS kits and their demo project, the plugin's own sounds (built in instead of the LXR-02
//! factory content, which users import themselves). Each kit is the firmware's init kit with the
//! settings below; drums with a pitch get tuned to a note by ear-free measurement: the generator
//! renders the voice, counts its cycles and moves Coarse by the difference.
//!
//! `cargo test --release --lib write_elabs_content -- --ignored --nocapture` writes
//! kits/elabs/ELABS (an SD card style project: NN-NAME.SND + NN-.PAT), prints what every voice
//! measures and renders target/elabs_kits_demo_48k.wav; tools/gen_kits.py and gen_projects.py then
//! pack the folder into the plugin.

use crate::engine::{Engine, Resampler};
use crate::kit_table::KIT_SIZE;
use crate::param_table::{INIT_KIT, PARAMS};
use crate::seq::{self, StepData};

pub struct KitDef {
    pub name: &'static str,
    pub settings: &'static [(&'static str, i32)],
    /// (voice 0-2 = Drum 1-3 / 3 = Snare, Hz its tone should land on)
    pub tune: &'static [(usize, f32)],
    /// one bar per track (DRUM1 DRUM2 DRUM3 SNARE CLP/CYM CL HH OP HH): X 127, x 100, o 55 (ghost)
    pub pattern: [&'static str; 7],
}

/// Settings every kit starts from: velocity sets the level, hats and cymbal high passed.
const BASE: &[(&str, i32)] = &[
    ("Drum 1 Velocity to Volume", 1),
    ("Drum 2 Velocity to Volume", 1),
    ("Drum 3 Velocity to Volume", 1),
    ("Snare Velocity to Volume", 1),
    ("Cymbal Velocity to Volume", 1),
    ("Hihat Velocity to Volume", 1),
    ("Drum 1 Drive", 30),
    ("Drum 2 Drive", 20),
    ("Drum 3 Drive", 20),
    ("Snare Distortion", 20),
    ("Cymbal Distortion", 0),
    ("Hihat Distortion", 0),
];

pub const KITS: [KitDef; 6] = [
    KitDef {
        name: "TEKKNO",
        settings: &[
            // a deep, round kick with a click on top
            ("Drum 1 Osc Wave", 0), ("Drum 1 Decay", 42), ("Drum 1 Amp Slope", 70), ("Drum 1 Pitch Decay", 28),
            ("Drum 1 Pitch Mod", 72), ("Drum 1 Pitch Slope", 8), ("Drum 1 Transient Vol", 60), ("Drum 1 Transient Wave", 5),
            ("Drum 1 Transient Freq", 12), ("Drum 1 Drive", 55), ("Drum 1 Filter Freq", 96),
            // rumbling low tom
            ("Drum 2 Osc Wave", 1), ("Drum 2 Decay", 34), ("Drum 2 Amp Slope", 30), ("Drum 2 Pitch Decay", 60), ("Drum 2 Pitch Mod", 18),
            ("Drum 2 FM Amount", 28), ("Drum 2 FM Freq", 70), ("Drum 2 Filter Freq", 84), ("Drum 2 Resonance", 40), ("Drum 2 Pan", 52),
            ("Drum 2 Volume", 105),
            // metallic perc
            ("Drum 3 Osc Wave", 3), ("Drum 3 Decay", 14), ("Drum 3 Amp Slope", 20), ("Drum 3 Pitch Mod", 0), ("Drum 3 FM Amount", 60),
            ("Drum 3 FM Freq", 92), ("Drum 3 Filter Type", 2), ("Drum 3 Filter Freq", 92), ("Drum 3 Resonance", 60), ("Drum 3 Pan", 80),
            ("Drum 3 Volume", 85),
            // snare: mostly noise, high passed
            ("Snare Osc Wave", 1), ("Snare Noise Mix", 92), ("Snare Noise Freq", 127), ("Snare Decay", 24), ("Snare Amp Slope", 28),
            ("Snare Pitch Decay", 30), ("Snare Pitch Mod", 30), ("Snare Filter Type", 1), ("Snare Filter Freq", 44), ("Snare Resonance", 30),
            ("Snare Distortion", 45), ("Snare Transient Vol", 50), ("Snare Transient Wave", 0),
            // ride
            ("Cymbal Osc Wave", 3), ("Cymbal Coarse", 100), ("Cymbal Mod Osc 1 Freq", 70), ("Cymbal Mod Osc 1 Gain", 80),
            ("Cymbal Mod Osc 2 Freq", 92), ("Cymbal Mod Osc 2 Gain", 70), ("Cymbal Filter Type", 1), ("Cymbal Filter Freq", 96),
            ("Cymbal Resonance", 30), ("Cymbal Decay", 58), ("Cymbal Amp Slope", 40), ("Cymbal Volume", 72), ("Cymbal Pan", 72),
            // hats
            ("Hihat Osc Wave", 3), ("Hihat Coarse", 78), ("Hihat Mod Osc 1 Freq", 78), ("Hihat Mod Osc 2 Freq", 100),
            ("Hihat Mod Osc 1 Gain", 90), ("Hihat Mod Osc 2 Gain", 90), ("Hihat Filter Freq", 110), ("Hihat Resonance", 20),
            ("Hihat Decay Closed", 8), ("Hihat Decay Open", 34), ("Hihat Volume", 92), ("Hihat Pan", 70),
        ],
        tune: &[(0, 49.0), (1, 98.0), (2, 330.0), (3, 196.0)],
        pattern: [
            "X...X...X...X...",
            "......x.......o.",
            "..o.......o...o.",
            "....X.......X...",
            "x.x.x.x.x.x.x.x.",
            "..x...x...x...x.",
            "..............x.",
        ],
    },
    KitDef {
        name: "BOOM808",
        settings: &[
            // long sine boom
            ("Drum 1 Osc Wave", 0), ("Drum 1 Decay", 84), ("Drum 1 Amp Slope", 90), ("Drum 1 Pitch Decay", 46), ("Drum 1 Pitch Mod", 44),
            ("Drum 1 Pitch Slope", 6), ("Drum 1 Drive", 25), ("Drum 1 Transient Vol", 20), ("Drum 1 Transient Wave", 2),
            // two toms
            ("Drum 2 Osc Wave", 0), ("Drum 2 Decay", 46), ("Drum 2 Amp Slope", 40), ("Drum 2 Pitch Decay", 50), ("Drum 2 Pitch Mod", 26),
            ("Drum 2 Pan", 44), ("Drum 2 Volume", 110),
            ("Drum 3 Osc Wave", 0), ("Drum 3 Decay", 42), ("Drum 3 Amp Slope", 40), ("Drum 3 Pitch Decay", 50), ("Drum 3 Pitch Mod", 26),
            ("Drum 3 Pan", 84), ("Drum 3 Volume", 105),
            // snare: tone and noise
            ("Snare Osc Wave", 0), ("Snare Noise Mix", 72), ("Snare Decay", 28), ("Snare Amp Slope", 22), ("Snare Pitch Decay", 22),
            ("Snare Pitch Mod", 40), ("Snare Filter Type", 0), ("Snare Filter Freq", 112), ("Snare Transient Vol", 40),
            ("Snare Transient Wave", 0),
            // cowbell
            ("Cymbal Osc Wave", 3), ("Cymbal Coarse", 76), ("Cymbal Mod Osc 1 Wave", 3), ("Cymbal Mod Osc 1 Freq", 84),
            ("Cymbal Mod Osc 1 Gain", 110), ("Cymbal Mod Osc 2 Gain", 0), ("Cymbal Filter Type", 2), ("Cymbal Filter Freq", 88),
            ("Cymbal Resonance", 50), ("Cymbal Decay", 26), ("Cymbal Amp Slope", 20), ("Cymbal Volume", 70), ("Cymbal Pan", 76),
            // hats: noise
            ("Hihat Osc Wave", 3), ("Hihat Coarse", 82), ("Hihat Mod Osc 1 Freq", 72), ("Hihat Mod Osc 2 Freq", 102),
            ("Hihat Mod Osc 1 Gain", 90), ("Hihat Mod Osc 2 Gain", 90), ("Hihat Filter Freq", 114), ("Hihat Resonance", 30),
            ("Hihat Decay Closed", 6), ("Hihat Decay Open", 42), ("Hihat Volume", 90), ("Hihat Pan", 58),
        ],
        tune: &[(0, 44.0), (1, 110.0), (2, 165.0), (3, 220.0)],
        pattern: [
            "X.........X.....",
            "...........x....",
            ".............x..",
            "....X.......X...",
            "......x.......x.",
            "x.x.x.x.x.x.x.xx",
            "................",
        ],
    },
    KitDef {
        name: "ELECTRO",
        settings: &[
            // punchy kick
            ("Drum 1 Osc Wave", 0), ("Drum 1 Decay", 24), ("Drum 1 Amp Slope", 50), ("Drum 1 Pitch Decay", 18), ("Drum 1 Pitch Mod", 92),
            ("Drum 1 Pitch Slope", 12), ("Drum 1 Transient Vol", 70), ("Drum 1 Transient Wave", 2), ("Drum 1 Drive", 70),
            // zap
            ("Drum 2 Osc Wave", 2), ("Drum 2 Decay", 26), ("Drum 2 Amp Slope", 24), ("Drum 2 Pitch Decay", 26), ("Drum 2 Pitch Mod", 127),
            ("Drum 2 Filter Freq", 92), ("Drum 2 Resonance", 70), ("Drum 2 Pan", 40), ("Drum 2 Volume", 85),
            // blip
            ("Drum 3 Osc Wave", 0), ("Drum 3 Decay", 6), ("Drum 3 Amp Slope", 10), ("Drum 3 Pitch Mod", 10), ("Drum 3 Pitch Decay", 10),
            ("Drum 3 Pan", 86), ("Drum 3 Volume", 90),
            // snare, a little clap in it
            ("Snare Osc Wave", 1), ("Snare Noise Mix", 105), ("Snare Decay", 20), ("Snare Amp Slope", 16), ("Snare Repeat", 3),
            ("Snare Filter Type", 2), ("Snare Filter Freq", 98), ("Snare Resonance", 40), ("Snare Distortion", 60),
            ("Snare Pitch Mod", 20), ("Snare Pitch Decay", 20),
            // short noise ride
            ("Cymbal Osc Wave", 4), ("Cymbal Filter Type", 2), ("Cymbal Filter Freq", 96), ("Cymbal Resonance", 40),
            ("Cymbal Repeat", 3), ("Cymbal Decay", 30), ("Cymbal Amp Slope", 30), ("Cymbal Volume", 80), ("Cymbal Pan", 60),
            // tight hats
            ("Hihat Osc Wave", 3), ("Hihat Coarse", 90), ("Hihat Mod Osc 1 Freq", 70), ("Hihat Mod Osc 2 Freq", 110),
            ("Hihat Mod Osc 1 Gain", 100), ("Hihat Mod Osc 2 Gain", 80), ("Hihat Filter Freq", 112), ("Hihat Decay Closed", 4),
            ("Hihat Decay Open", 26), ("Hihat Volume", 88), ("Hihat Pan", 66),
        ],
        tune: &[(0, 55.0), (1, 220.0), (2, 880.0), (3, 247.0)],
        pattern: [
            "X......X..X.....",
            "......x.......x.",
            ".x...x.......x..",
            "....X.......X...",
            "....x.......x...",
            "xx.xxx.xxx.xxx.x",
            "..x.............",
        ],
    },
    KitDef {
        name: "LOFIDUST",
        settings: &[
            // dusty kick
            ("Drum 1 Osc Wave", 0), ("Drum 1 Decay", 36), ("Drum 1 Amp Slope", 60), ("Drum 1 Pitch Decay", 30), ("Drum 1 Pitch Mod", 50),
            ("Drum 1 Sample Rate", 70), ("Drum 1 Filter Freq", 66), ("Drum 1 Transient Vol", 30), ("Drum 1 Transient Wave", 4),
            // rim
            ("Drum 2 Osc Wave", 1), ("Drum 2 Decay", 3), ("Drum 2 Transient Vol", 110), ("Drum 2 Transient Wave", 6),
            ("Drum 2 Transient Freq", 40), ("Drum 2 Sample Rate", 80), ("Drum 2 Pan", 50), ("Drum 2 Volume", 80),
            // shaker
            ("Drum 3 Osc Wave", 4), ("Drum 3 Attack", 6), ("Drum 3 Decay", 12), ("Drum 3 Amp Slope", 30), ("Drum 3 Pitch Mod", 0),
            ("Drum 3 Filter Type", 2), ("Drum 3 Filter Freq", 104), ("Drum 3 Resonance", 30), ("Drum 3 Pan", 84), ("Drum 3 Volume", 70),
            // crunchy snare
            ("Snare Osc Wave", 1), ("Snare Noise Mix", 80), ("Snare Decay", 32), ("Snare Amp Slope", 30), ("Snare Sample Rate", 72),
            ("Snare Filter Freq", 82), ("Snare Distortion", 40), ("Snare Pitch Mod", 24), ("Snare Pitch Decay", 30),
            // crackly cymbal
            ("Cymbal Osc Wave", 4), ("Cymbal Sample Rate", 60), ("Cymbal Filter Type", 2), ("Cymbal Filter Freq", 90),
            ("Cymbal Resonance", 30), ("Cymbal Repeat", 2), ("Cymbal Decay", 34), ("Cymbal Amp Slope", 30), ("Cymbal Volume", 115),
            ("Cymbal Pan", 40),
            // soft hats
            ("Hihat Osc Wave", 3), ("Hihat Coarse", 76), ("Hihat Mod Osc 1 Freq", 70), ("Hihat Mod Osc 2 Freq", 98),
            ("Hihat Mod Osc 1 Gain", 80), ("Hihat Mod Osc 2 Gain", 80), ("Hihat Sample Rate", 76), ("Hihat Filter Type", 1),
            ("Hihat Filter Freq", 104), ("Hihat Resonance", 20), ("Hihat Decay Closed", 7), ("Hihat Decay Open", 30), ("Hihat Volume", 78), ("Hihat Pan", 72),
            ("Master Sample Rate", 118),
        ],
        tune: &[(0, 52.0), (3, 185.0)],
        pattern: [
            "X.....X...X.....",
            "..........x.....",
            "x.xxx.xxx.xxx.xx",
            "....X..o....X...",
            "............x...",
            "x...x...x...x.o.",
            "..........x.....",
        ],
    },
    KitDef {
        name: "INDUSTRL",
        settings: &[
            // distorted kick
            ("Drum 1 Osc Wave", 0), ("Drum 1 Decay", 38), ("Drum 1 Amp Slope", 60), ("Drum 1 Pitch Decay", 22), ("Drum 1 Pitch Mod", 80),
            ("Drum 1 Drive", 110), ("Drum 1 Filter Drive", 70), ("Drum 1 Filter Freq", 88), ("Drum 1 Transient Vol", 80),
            ("Drum 1 Transient Wave", 5), ("Drum 1 Volume", 100),
            // clang
            ("Drum 2 Osc Wave", 3), ("Drum 2 Decay", 32), ("Drum 2 Amp Slope", 30), ("Drum 2 FM Amount", 100), ("Drum 2 FM Freq", 100),
            ("Drum 2 FM Wave", 0), ("Drum 2 Filter Type", 5), ("Drum 2 Filter Freq", 90), ("Drum 2 Resonance", 70), ("Drum 2 Pan", 36),
            ("Drum 2 Pitch Mod", 10), ("Drum 2 Volume", 80),
            // folded noise hit
            ("Drum 3 Osc Wave", 4), ("Drum 3 Decay", 18), ("Drum 3 Amp Slope", 20), ("Drum 3 Drive", 100), ("Drum 3 Filter Type", 2),
            ("Drum 3 Filter Freq", 70), ("Drum 3 Resonance", 90), ("Drum 3 Pan", 90), ("Drum 3 Volume", 80), ("Drum 3 Pitch Mod", 0),
            // metal snare
            ("Snare Osc Wave", 3), ("Snare Noise Mix", 100), ("Snare Decay", 26), ("Snare Amp Slope", 24), ("Snare Distortion", 110),
            ("Snare Filter Type", 2), ("Snare Filter Freq", 92), ("Snare Resonance", 90), ("Snare Pitch Mod", 50), ("Snare Pitch Decay", 26),
            ("Snare Volume", 90),
            // metallic cymbal
            ("Cymbal Osc Wave", 3), ("Cymbal Coarse", 110), ("Cymbal Mod Osc 1 Freq", 90), ("Cymbal Mod Osc 1 Gain", 120),
            ("Cymbal Mod Osc 2 Freq", 60), ("Cymbal Mod Osc 2 Gain", 100), ("Cymbal Filter Type", 1), ("Cymbal Filter Freq", 90),
            ("Cymbal Distortion", 60), ("Cymbal Decay", 50), ("Cymbal Amp Slope", 50), ("Cymbal Volume", 62), ("Cymbal Pan", 80),
            // gritty hats
            ("Hihat Osc Wave", 3), ("Hihat Coarse", 70), ("Hihat Mod Osc 1 Freq", 90), ("Hihat Mod Osc 2 Freq", 104),
            ("Hihat Mod Osc 1 Gain", 110), ("Hihat Mod Osc 2 Gain", 110), ("Hihat Distortion", 70), ("Hihat Filter Freq", 106),
            ("Hihat Decay Closed", 6), ("Hihat Decay Open", 38), ("Hihat Volume", 80), ("Hihat Pan", 58),
        ],
        tune: &[(0, 46.0), (3, 175.0)],
        pattern: [
            "X..XX...X..XX...",
            "...........x....",
            "......x.......x.",
            "....X.......X..o",
            "x...............",
            "x.xxx.xxx.xxx.xx",
            "..x.......x.....",
        ],
    },
    KitDef {
        name: "MINIMAL",
        settings: &[
            // small clicky kick
            ("Drum 1 Osc Wave", 0), ("Drum 1 Decay", 20), ("Drum 1 Amp Slope", 40), ("Drum 1 Pitch Decay", 16), ("Drum 1 Pitch Mod", 56),
            ("Drum 1 Transient Vol", 56), ("Drum 1 Transient Wave", 4), ("Drum 1 Drive", 20),
            // tick
            ("Drum 2 Osc Wave", 0), ("Drum 2 Decay", 4), ("Drum 2 Amp Slope", 6), ("Drum 2 Pitch Mod", 30), ("Drum 2 Pitch Decay", 8),
            ("Drum 2 Transient Vol", 60), ("Drum 2 Transient Wave", 3), ("Drum 2 Pan", 34), ("Drum 2 Volume", 80),
            // wood
            ("Drum 3 Osc Wave", 1), ("Drum 3 Decay", 9), ("Drum 3 Amp Slope", 10), ("Drum 3 Pitch Mod", 14), ("Drum 3 Pitch Decay", 10),
            ("Drum 3 Filter Type", 2), ("Drum 3 Filter Freq", 96), ("Drum 3 Resonance", 50), ("Drum 3 Pan", 92), ("Drum 3 Volume", 127),
            // rimshot
            ("Snare Osc Wave", 1), ("Snare Noise Mix", 30), ("Snare Decay", 9), ("Snare Amp Slope", 10), ("Snare Pitch Mod", 30),
            ("Snare Pitch Decay", 12), ("Snare Transient Vol", 90), ("Snare Transient Wave", 6), ("Snare Filter Freq", 110),
            ("Snare Volume", 64),
            // shaker on the cymbal
            ("Cymbal Osc Wave", 4), ("Cymbal Attack", 8), ("Cymbal Decay", 18), ("Cymbal Amp Slope", 24), ("Cymbal Filter Type", 1), ("Cymbal Filter Freq", 110),
            ("Cymbal Volume", 60), ("Cymbal Pan", 70),
            // quiet short hats
            ("Hihat Osc Wave", 3), ("Hihat Coarse", 84), ("Hihat Mod Osc 1 Freq", 80), ("Hihat Mod Osc 2 Freq", 96),
            ("Hihat Mod Osc 1 Gain", 80), ("Hihat Mod Osc 2 Gain", 80), ("Hihat Filter Freq", 114), ("Hihat Decay Closed", 3),
            ("Hihat Decay Open", 20), ("Hihat Volume", 76), ("Hihat Pan", 56),
        ],
        tune: &[(0, 58.0), (1, 1046.0), (2, 523.0), (3, 392.0)],
        pattern: [
            "X...X...X...X...",
            "...x......x.....",
            "......x.....x..x",
            "................",
            "..x...x...x...x.",
            "..o...o...o...o.",
            "................",
        ],
    },
];

fn param_index(name: &str) -> &'static crate::param_table::ParamInfo {
    PARAMS.iter().find(|p| p.name == name).unwrap_or_else(|| panic!("no parameter {name}"))
}

/// A kit's values: the init kit with the base and the kit's settings.
pub fn kit_data(def: &KitDef) -> Vec<u8> {
    let mut data = INIT_KIT.to_vec();
    data.resize(KIT_SIZE, 0);
    for &(name, value) in BASE.iter().chain(def.settings) {
        let p = param_index(name);
        assert!((p.min..=p.max).contains(&value), "{}: {name} = {value} is outside {}..{}", def.name, p.min, p.max);
        data[p.index as usize] = value as u8;
    }
    data
}

/// The kit's bar as a pattern, one bar long on every track.
pub fn pattern(def: &KitDef, kit_slot: usize) -> Vec<u8> {
    let mut p = seq::empty_pattern();
    for (t, row) in def.pattern.iter().enumerate() {
        assert_eq!(row.len(), 16, "{} track {t}", def.name);
        for (s, c) in row.chars().enumerate() {
            let velocity = match c {
                'X' => 127,
                'x' => 100,
                'o' => 55,
                _ => continue,
            };
            seq::toggle_step(&mut p, t, s);
            let d = seq::step(&p, t, s);
            seq::set_step(&mut p, t, s, StepData { velocity, ..d });
        }
        seq::set_track_length(&mut p, t, 16);
    }
    seq::set_kit_number(&mut p, kit_slot);
    p
}

fn load(engine: &mut Engine, data: &[u8]) {
    for info in PARAMS {
        let v = crate::kits::kit_value(data, info.index, info.min, info.max, &info.display);
        engine.set_param(info.index, v as u8);
    }
}

/// One hit of `voice`: (peak, time to fall 40 dB in ms, Hz of its tone between 60 and 160 ms).
fn measure(engine: &mut Engine, rs: &mut Resampler, voice: u8) -> (f32, f32, f32) {
    // let anything still ringing die away
    for _ in 0..48000 {
        rs.next(engine);
    }
    engine.note_on(voice, 60, 127);
    let x: Vec<f32> = (0..48000 * 2).map(|_| {
        let f = rs.next(engine);
        (f[0] + f[1] + f[2] + f[3]) * 0.5
    }).collect();
    let peak = x.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let last = x.iter().rposition(|s| s.abs() > peak * 0.01).unwrap_or(0);
    let window = &x[48 * 60..48 * 160];
    let crossings = window.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
    (peak, last as f32 / 48.0, crossings as f32 / 2.0 / 0.1)
}

const COARSE: [&str; 4] = ["Drum 1 Coarse", "Drum 2 Coarse", "Drum 3 Coarse", "Snare Coarse"];

#[test]
#[ignore]
fn write_elabs_content() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/kits/elabs/ELABS");
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    let mut engine = Engine::new().expect("engine");
    let mut rs = Resampler::new(48000.0);
    let mut demo: Vec<[f32; 2]> = Vec::new();
    for (slot, def) in KITS.iter().enumerate() {
        let mut data = kit_data(def);
        // tune: twice, the second pass corrects what the pitch envelope's tail threw off
        for _ in 0..2 {
            for &(voice, hz) in def.tune {
                load(&mut engine, &data);
                let (_, _, f) = measure(&mut engine, &mut rs, voice as u8);
                if f > 5.0 {
                    let at = param_index(COARSE[voice]).index as usize;
                    let moved = data[at] as f32 + 12.0 * (hz / f).log2();
                    data[at] = moved.round().clamp(0.0, 127.0) as u8;
                }
            }
        }
        load(&mut engine, &data);
        println!("{}:", def.name);
        for voice in 0..7u8 {
            let (peak, ms, hz) = measure(&mut engine, &mut rs, voice);
            println!("  {:7} peak {peak:5.2} ({:+5.1} dB)  {ms:6.0} ms  {hz:6.0} Hz", ["DRUM1", "DRUM2", "DRUM3", "SNARE", "CLP/CYM", "CL HH", "OP HH"][voice as usize], 20.0 * peak.max(1e-6).log10());
        }
        std::fs::write(format!("{dir}/{slot:02}-{}.SND", &def.name[..def.name.len().min(5)]), crate::library::snd_bytes(def.name, &data)).unwrap();
        let p = pattern(def, slot);
        std::fs::write(format!("{dir}/{slot:02}-.PAT"), &p).unwrap();

        // two bars of the kit's pattern at 124 BPM
        let sixteenth = 48000 * 60 / 124 / 4;
        for step in 0..32 {
            for t in 0..7 {
                if seq::step_on(&p, t, step % 16) {
                    engine.note_on(t as u8, 60, seq::step(&p, t, step % 16).velocity);
                }
            }
            for _ in 0..sixteenth {
                let f = rs.next(&mut engine);
                demo.push([f[0] + f[2], f[1] + f[3]]);
            }
        }
    }
    let peak = demo.iter().flat_map(|f| f.iter()).fold(0.0f32, |m, s| m.max(s.abs()));
    println!("demo peak {peak:.2}");
    let mut b = Vec::new();
    let data_len = (demo.len() * 4) as u32;
    b.extend(b"RIFF"); b.extend((36 + data_len).to_le_bytes()); b.extend(b"WAVEfmt ");
    b.extend(16u32.to_le_bytes()); b.extend(1u16.to_le_bytes()); b.extend(2u16.to_le_bytes());
    b.extend(48000u32.to_le_bytes()); b.extend((48000u32 * 4).to_le_bytes()); b.extend(4u16.to_le_bytes());
    b.extend(16u16.to_le_bytes()); b.extend(b"data"); b.extend(data_len.to_le_bytes());
    for f in &demo {
        for s in f {
            b.extend(((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
    }
    std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/target/elabs_kits_demo_48k.wav"), b).unwrap();
}
