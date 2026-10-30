//! ELABS RLX (formerly ELABS LXR): the Erica Synths LXR V2 drum engine, built from its GPL-3.0 firmware, as a VST3.
//!
//! MIDI: channel 1 notes 36-42 (C1-F#1) trigger Drum 1, Drum 2, Drum 3, Snare, Cymbal,
//! Hihat closed and Hihat open at their programmed pitch. Channels 2-8 play the same voices
//! chromatically (note 60 = programmed pitch).

use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

mod editor;
#[cfg(test)]
mod elabs_kits;
mod engine;
mod entropia;
mod favorites;
mod kit_table;
mod kits;
mod library;
mod maestro;
mod param_table;
mod project_table;
mod seq;
mod ui_pages;

use base64::Engine as _;
use engine::{Engine, Resampler, SeqClock};
use seq::SeqShared;
use param_table::{Display, ParamInfo, NUM_ENGINE_PARAMS, PARAMS};

const VOICES: [&str; 6] = ["Drum 1", "Drum 2", "Drum 3", "Snare", "Cymbal", "Hihat"];
const DRUM_MAP_FIRST_NOTE: u8 = 36;
const NUM_TRIGGERS: u8 = 7;
/// PAR_FX_TYPE: selects which FX sub page the panel shows
pub const FX_TYPE_PARAM: u16 = 234;

pub struct LxrParams {
    editor_state: Arc<EguiState>,
    sound: Vec<IntParam>,
    /// position in `sound` per firmware parameter index
    sound_by_index: [Option<u16>; NUM_ENGINE_PARAMS],
    faders: Vec<FloatParam>,
    volume: FloatParam,
    /// plays the firmware's step sequencer in time with the host transport
    seq_on: BoolParam,
    /// morph from the kit towards the morph kit (kit B), 0-255 like the LXR
    morph: IntParam,
    /// ENTROPIA MOD FX instead of the LXR's own FX bus (CLASSIC)
    fx_mode: BoolParam,
    /// ENTROPIA MOD FX settings, engine numbering (entropia.rs), with the ids the host saves them under
    fx: Vec<(String, entropia::FxParam)>,
    /// kit B of the morph, saved with the project
    morph_kit: Mutex<Option<(String, Vec<u8>)>>,
    /// bumped when the morph kit changes, so the audio thread picks it up
    morph_version: AtomicU32,
    /// voices (bits 0-5) and the FX (bit 6) that loading a kit leaves alone
    voice_locks: AtomicU8,
    /// name of the last loaded kit, saved with the project
    kit_name: Mutex<String>,
    /// sequencer patterns, saved with the project
    seq: SeqShared,
    /// the Maestro's settings (LANES, behind the robot), saved with the project
    maestro: maestro::MaestroShared,
}

/// Lock-free messages between the panel and the audio thread, one bit per trigger (0-6).
#[derive(Default)]
pub struct UiShared {
    /// pads pressed on the panel, to be played by the audio thread
    pending: AtomicU8,
    /// triggers played since the panel last looked, for the pad LEDs
    fired: AtomicU8,
}

impl LxrParams {
    fn new() -> Self {
        let sound = PARAMS.iter().map(sound_param).collect();
        let faders = VOICES
            .iter()
            .map(|v| {
                FloatParam::new(format!("{v} Fader"), 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                    .with_unit(" %")
                    .with_value_to_string(formatters::v2s_f32_percentage(0))
                    .with_string_to_value(formatters::s2v_f32_percentage())
            })
            .collect();
        let mut sound_by_index = [None; NUM_ENGINE_PARAMS];
        for (i, info) in PARAMS.iter().enumerate() {
            sound_by_index[info.index as usize] = Some(i as u16);
        }
        let volume = FloatParam::new("Volume", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage());
        Self {
            editor_state: editor::default_state(),
            sound,
            sound_by_index,
            faders,
            volume,
            seq_on: BoolParam::new("Sequencer", false),
            morph: IntParam::new("Morph", 0, IntRange::Linear { min: 0, max: 255 }),
            fx_mode: BoolParam::new("FX Mode", false)
                .with_value_to_string(Arc::new(|on| entropia::FX_MODES[on as usize].to_string()))
                .with_string_to_value(Arc::new(|s| entropia::FX_MODES.iter().position(|m| m.eq_ignore_ascii_case(s.trim())).map(|i| i == 1))),
            fx: entropia::params(),
            morph_kit: Mutex::new(None),
            morph_version: AtomicU32::new(0),
            voice_locks: AtomicU8::new(0),
            kit_name: Mutex::new("Init".to_string()),
            seq: SeqShared::default(),
            maestro: maestro::MaestroShared::default(),
        }
    }

    pub fn set_morph_kit(&self, kit: Option<(String, Vec<u8>)>) {
        if let Ok(mut k) = self.morph_kit.lock() {
            *k = kit;
        }
        self.morph_version.fetch_add(1, Ordering::AcqRel);
    }

    pub fn morph_kit_name(&self) -> Option<String> {
        self.morph_kit.lock().ok().and_then(|k| k.as_ref().map(|k| k.0.clone()))
    }

    /// The plugin parameter for a firmware parameter index, if the plugin exposes it.
    pub fn by_index(&self, index: u16) -> Option<&IntParam> {
        let i = (*self.sound_by_index.get(index as usize)?)?;
        self.sound.get(i as usize)
    }
}

fn sound_param(info: &'static ParamInfo) -> IntParam {
    let default = (engine::init_kit_value(info.index) as i32).clamp(info.min, info.max);
    let range = IntRange::Linear { min: info.min, max: info.max };
    let p = IntParam::new(info.name, default, range);
    match info.display {
        Display::Plain => p,
        Display::Offset(center) => p
            .with_value_to_string(Arc::new(move |v| format!("{:+}", v - center)))
            .with_string_to_value(Arc::new(move |s| s.trim().parse::<i32>().ok().map(|v| v + center))),
        Display::ModTarget => p
            .with_value_to_string(Arc::new(|v| mod_target_name(v as usize)))
            .with_string_to_value(Arc::new(|s| {
                let s = s.trim();
                (0..ui_pages::MOD_TARGETS.len())
                    .find(|&i| mod_target_name(i).eq_ignore_ascii_case(s))
                    .map(|i| i as i32)
                    .or_else(|| s.parse().ok())
            })),
        Display::Names(names) => p
            .with_value_to_string(Arc::new(move |v| names.get(v as usize).unwrap_or(&"?").to_string()))
            .with_string_to_value(Arc::new(move |s| {
                let s = s.trim();
                names
                    .iter()
                    .position(|n| n.eq_ignore_ascii_case(s))
                    .map(|i| i as i32)
                    .or_else(|| s.parse().ok())
            })),
    }
}

fn mod_target_name(index: usize) -> String {
    match ui_pages::MOD_TARGETS.get(index) {
        Some(t) if t.param == 0 => "Off".to_string(),
        Some(t) if t.voice == 0 => format!("All {}", t.long),
        Some(t) => format!("{} {}", VOICES[t.voice as usize - 1], t.long),
        None => "?".to_string(),
    }
}

unsafe impl Params for LxrParams {
    fn param_map(&self) -> Vec<(String, ParamPtr, String)> {
        let sound = PARAMS.iter().zip(&self.sound).map(|(info, p)| {
            (format!("p{}", info.index), p.as_ptr(), info.group.to_string())
        });
        let faders = VOICES
            .iter()
            .zip(&self.faders)
            .enumerate()
            .map(|(i, (v, p))| (format!("fader{i}"), p.as_ptr(), v.to_string()));
        sound
            .chain(faders)
            .chain(std::iter::once(("volume".to_string(), self.volume.as_ptr(), "Master".to_string())))
            .chain(std::iter::once(("seq_on".to_string(), self.seq_on.as_ptr(), "Sequencer".to_string())))
            .chain(std::iter::once(("morph".to_string(), self.morph.as_ptr(), "Master".to_string())))
            .chain(std::iter::once(("fx_mode".to_string(), self.fx_mode.as_ptr(), "Entropia Mod FX".to_string())))
            .chain(self.fx.iter().enumerate().map(|(i, (key, p))| (key.clone(), p.as_ptr(), entropia::group(i as u16).to_string())))
            .collect()
    }

    fn serialize_fields(&self) -> BTreeMap<String, String> {
        let kit = self.kit_name.lock().map(|n| n.clone()).unwrap_or_default();
        let mut fields = BTreeMap::from([("kit_name".to_string(), kit)]);
        fields.insert("voice_locks".to_string(), self.voice_locks.load(Ordering::Acquire).to_string());
        let (w, h) = self.editor_state.size();
        fields.insert("editor_size".to_string(), format!("{w},{h}"));
        if let Ok(Some((name, data))) = self.morph_kit.lock().as_deref() {
            let snd = library::snd_bytes(name, data);
            fields.insert("morph_kit".to_string(), base64::engine::general_purpose::STANDARD.encode(snd));
        }
        fields.insert("seq_project".to_string(), self.seq.project_name().unwrap_or_default());
        if let Ok(kits) = self.seq.project_kits.lock() {
            for (nr, (name, data)) in kits.iter().enumerate().filter_map(|(nr, k)| Some((nr, k.as_ref()?))) {
                let snd = library::snd_bytes(name, data);
                fields.insert(format!("seq_project_kit_{nr}"), base64::engine::general_purpose::STANDARD.encode(snd));
            }
        }
        let follows = self.seq.kit_follows.load(Ordering::Acquire);
        fields.insert("seq_kit_follows".to_string(), follows.to_string());
        fields.insert("maestro".to_string(), self.maestro.to_field());
        fields.insert("seq_mutes".to_string(), self.seq.mutes.load(Ordering::Acquire).to_string());
        if let Ok(song) = self.seq.song.lock() {
            fields.insert("seq_song".to_string(), song.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(","));
        }
        fields.insert("seq_song_on".to_string(), self.seq.song_on.load(Ordering::Acquire).to_string());
        fields.insert("seq_song_loop".to_string(), self.seq.song_loop.load(Ordering::Acquire).to_string());
        // euclid settings per track as "length,steps,rotation;..."
        if let Ok(euclid) = self.seq.euclid.lock() {
            let text: Vec<String> = euclid.iter().map(|e| format!("{},{},{}", e.length, e.steps, e.rotation)).collect();
            fields.insert("seq_euclid".to_string(), text.join(";"));
        }
        // only patterns with steps, as base64 .pat files
        if let Ok(patterns) = self.seq.patterns.lock() {
            for (nr, p) in patterns.iter().enumerate().filter(|(_, p)| !seq::is_empty(p)) {
                fields.insert(format!("seq_pattern_{nr}"), base64::engine::general_purpose::STANDARD.encode(p));
            }
        }
        fields
    }

    fn deserialize_fields(&self, serialized: &BTreeMap<String, String>) {
        if let (Some(name), Ok(mut kit)) = (serialized.get("kit_name"), self.kit_name.lock()) {
            *kit = name.clone();
        }
        self.voice_locks.store(serialized.get("voice_locks").and_then(|v| v.parse().ok()).unwrap_or(0), Ordering::Release);
        // the window opens at the size it had
        if let Some((w, h)) = serialized.get("editor_size").and_then(|s| s.split_once(',')) {
            if let (Ok(w), Ok(h)) = (w.parse::<u32>(), h.parse::<u32>()) {
                if (200..5000).contains(&w) && (100..5000).contains(&h) && (w, h) != self.editor_state.size() {
                    self.editor_state.set_requested_size((w, h));
                }
            }
        }
        self.set_morph_kit(
            serialized
                .get("morph_kit")
                .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok())
                .and_then(|b| library::parse_snd(&b)),
        );
        if let Ok(mut project) = self.seq.project.lock() {
            *project = serialized.get("seq_project").filter(|p| !p.is_empty()).cloned();
        }
        if let Ok(mut kits) = self.seq.project_kits.lock() {
            for (nr, kit) in kits.iter_mut().enumerate() {
                *kit = serialized
                    .get(&format!("seq_project_kit_{nr}"))
                    .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok())
                    .and_then(|b| library::parse_snd(&b));
            }
        }
        if let Some(follows) = serialized.get("seq_kit_follows") {
            self.seq.kit_follows.store(follows == "true", Ordering::Release);
        }
        let seq = &self.seq;
        if let Some(text) = serialized.get("maestro") {
            self.maestro.from_field(text);
        }
        seq.mutes.store(serialized.get("seq_mutes").and_then(|m| m.parse().ok()).unwrap_or(0), Ordering::Release);
        if let Ok(mut song) = seq.song.lock() {
            *song = serialized
                .get("seq_song")
                .map(|s| s.split(',').filter_map(|p| p.parse::<u8>().ok()).filter(|p| (*p as usize) < seq::NUM_PATTERNS).collect())
                .unwrap_or_default();
        }
        seq.song_on.store(serialized.get("seq_song_on").is_some_and(|v| v == "true"), Ordering::Release);
        seq.song_loop.store(serialized.get("seq_song_loop").is_none_or(|v| v == "true"), Ordering::Release);
        seq.song_dirty.store(true, Ordering::Release);
        if let (Some(text), Ok(mut euclid)) = (serialized.get("seq_euclid"), self.seq.euclid.lock()) {
            for (e, track) in euclid.iter_mut().zip(text.split(';')) {
                let v: Vec<u8> = track.split(',').filter_map(|n| n.parse().ok()).collect();
                if let [length, steps, rotation] = v[..] {
                    *e = seq::Euclid { length: length.clamp(1, 16), steps: steps.clamp(1, 16), rotation: rotation.min(15) };
                }
            }
        }
        if let Ok(mut patterns) = self.seq.patterns.lock() {
            for (nr, p) in patterns.iter_mut().enumerate() {
                *p = serialized
                    .get(&format!("seq_pattern_{nr}"))
                    .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok())
                    .filter(|b| b.len() == seq::PATTERN_BYTES)
                    .unwrap_or_else(seq::empty_pattern);
            }
        }
        self.seq.dirty.store(u64::MAX, Ordering::Release);
    }
}

/// The firmware's interpolate() (presetManager.c): a towards b by x/256, rounded up from 0x7f.
fn morph_value(a: u8, b: u8, x: u8) -> u8 {
    let fixed = (a as i32 * 256 + (b as i32 - a as i32) * x as i32) as u16;
    let result = (fixed / 256) as u8;
    if fixed & 0xff < 0x7f {
        result
    } else {
        result.wrapping_add(1)
    }
}

/// Parameters the morph moves: not the audio outputs (preset_morph_tick() keeps the source kit's)
/// and not the modulation destinations (preset_writeDrumsetData() doesn't morph them either).
fn morphs(info: &ParamInfo) -> bool {
    !matches!(info.display, Display::ModTarget) && !(161..=166).contains(&info.index) && !(215..=220).contains(&info.index)
}

pub struct Lxr {
    params: Arc<LxrParams>,
    shared: Arc<UiShared>,
    engine: Option<Engine>,
    resampler: Resampler,
    clock: SeqClock,
    sample_rate: f32,
    last_bpm: u16,
    /// Last value sent to the engine per firmware parameter index (-1 = never sent).
    sent: [i32; NUM_ENGINE_PARAMS],
    sent_faders: [f32; 6],
    /// ENTROPIA MOD FX state last sent to the engine (NaN / None = send again)
    sent_fx: [f32; entropia::COUNT],
    sent_fx_mode: Option<bool>,
    has_aux_out: bool,
    /// copy of the morph kit for the audio thread, and the version it was taken at
    morph_kit: Option<Vec<u8>>,
    morph_seen: u32,
    /// sequencer state last sent to the engine (None = send again)
    sent_mutes: Option<u8>,
    sent_record: Option<bool>,
    /// notes were recorded into the engine's patterns; copy them back to the plugin's
    recorded: bool,
    maestro: maestro::Maestro,
}

impl Default for Lxr {
    fn default() -> Self {
        Self {
            params: Arc::new(LxrParams::new()),
            shared: Arc::new(UiShared::default()),
            engine: None,
            resampler: Resampler::new(44100.0),
            clock: SeqClock::default(),
            sample_rate: 44100.0,
            last_bpm: 0,
            sent: [-1; NUM_ENGINE_PARAMS],
            sent_faders: [-1.0; 6],
            sent_fx: [f32::NAN; entropia::COUNT],
            sent_fx_mode: None,
            has_aux_out: false,
            morph_kit: None,
            morph_seen: u32::MAX,
            sent_mutes: None,
            sent_record: None,
            recorded: false,
            maestro: maestro::Maestro::default(),
        }
    }
}

impl Lxr {
    fn sync_params(&mut self) {
        let Some(engine) = self.engine.as_mut() else { return };
        // pick up a new morph kit (the panel only holds the lock for a moment; retry next block)
        let version = self.params.morph_version.load(Ordering::Acquire);
        if version != self.morph_seen {
            if let Ok(kit) = self.params.morph_kit.try_lock() {
                self.morph_kit = kit.as_ref().map(|k| k.1.clone());
                self.morph_seen = version;
            }
        }
        let morph = self.params.morph.value() as u8;
        for (info, p) in PARAMS.iter().zip(&self.params.sound) {
            let v = match &self.morph_kit {
                Some(b) if morph > 0 && morphs(info) => {
                    let target = kits::kit_value(b, info.index, info.min, info.max, &info.display);
                    morph_value(p.value() as u8, target as u8, morph) as i32
                }
                _ => p.value(),
            };
            let sent = &mut self.sent[info.index as usize];
            if *sent != v {
                *sent = v;
                engine.set_param(info.index, v as u8);
            }
        }
        for (i, p) in self.params.faders.iter().enumerate() {
            let v = p.value();
            if self.sent_faders[i] != v {
                self.sent_faders[i] = v;
                engine.set_fader(i as u8, v);
            }
        }
        for (i, (_, p)) in self.params.fx.iter().enumerate() {
            let v = p.engine_value();
            if self.sent_fx[i] != v {
                self.sent_fx[i] = v;
                engine.fx_set(i as u16, v);
            }
        }
        let entropia = self.params.fx_mode.value();
        if self.sent_fx_mode != Some(entropia) {
            self.sent_fx_mode = Some(entropia);
            engine.fx_set_mode(entropia);
        }
    }

    /// Host tempo, patterns changed on the panel and pattern switches go to the engine.
    fn sync_sequencer(&mut self, tempo: Option<f64>) {
        let Some(engine) = self.engine.as_mut() else { return };
        let bpm = tempo.unwrap_or(120.0).round().clamp(1.0, 999.0) as u16;
        if bpm != self.last_bpm {
            self.last_bpm = bpm;
            engine.set_bpm(bpm);
        }
        let seq = &self.params.seq;
        let dirty = seq.dirty.load(Ordering::Acquire);
        if dirty != 0 {
            // the panel only holds this lock for a moment; try again next block if it has it
            if let Ok(patterns) = seq.patterns.try_lock() {
                for nr in (0..seq::NUM_PATTERNS).filter(|nr| dirty & (1 << nr) != 0) {
                    engine.seq_read_pattern(nr as u8, &patterns[nr]);
                }
                seq.dirty.fetch_and(!dirty, Ordering::AcqRel);
            }
        }
        if let Some(nr) = seq.take_request() {
            engine.seq_set_pattern(nr);
        }
        let mutes = seq.mutes.load(Ordering::Acquire);
        if self.sent_mutes != Some(mutes) {
            self.sent_mutes = Some(mutes);
            engine.seq_set_mutes(mutes);
        }
        let record = seq.record.load(Ordering::Acquire);
        if self.sent_record != Some(record) {
            self.sent_record = Some(record);
            engine.seq_set_record(record);
        }
        if seq.song_dirty.load(Ordering::Acquire) {
            if let Ok(song) = seq.song.try_lock() {
                seq.song_dirty.store(false, Ordering::Release);
                engine.seq_set_song(&song, seq.song_loop.load(Ordering::Acquire), seq.song_on.load(Ordering::Acquire));
            }
        }
    }

    /// Plays a voice; while recording with the sequencer running, also writes it into the pattern.
    fn trigger(&mut self, voice: u8, note: u8, velocity: u8) {
        let Some(engine) = self.engine.as_mut() else { return };
        engine.note_on(voice, note, velocity);
        if self.clock.is_running() && self.params.seq.record.load(Ordering::Acquire) {
            engine.seq_record(voice, velocity, note);
            self.recorded = true;
        }
    }

    /// Sequencer position and played voices for the panel.
    fn publish_sequencer(&mut self) {
        let Some(engine) = self.engine.as_mut() else { return };
        let seq = &self.params.seq;
        seq.active.store(engine.seq_active_pattern(), Ordering::Release);
        seq.pending.store(engine.seq_pending_pattern(), Ordering::Release);
        seq.running.store(self.clock.is_running(), Ordering::Release);
        for (t, step) in seq.step.iter().enumerate() {
            step.store(engine.seq_step_index(t as u8), Ordering::Release);
        }
        self.shared.fired.fetch_or(engine.take_triggered(), Ordering::AcqRel);
        // recorded notes land in the playing pattern (or the next one, near its end): copy both back
        if self.recorded {
            if let Ok(mut patterns) = seq.patterns.try_lock() {
                for nr in [engine.seq_active_pattern(), engine.seq_pending_pattern()] {
                    patterns[nr as usize % seq::NUM_PATTERNS] = engine.seq_write_pattern(nr);
                }
                self.recorded = false;
            }
        }
    }

    fn handle_event(&mut self, event: NoteEvent<()>) {
        if let NoteEvent::NoteOn { channel, note, velocity, .. } = event {
            let vel = ((velocity * 127.0).round() as u8).clamp(1, 127);
            match channel {
                0 => {
                    let voice = note.wrapping_sub(DRUM_MAP_FIRST_NOTE);
                    if voice < NUM_TRIGGERS {
                        self.trigger(voice, 60, vel);
                    }
                }
                1..=7 => self.trigger(channel - 1, note, vel),
                _ => {}
            }
        }
    }
}

impl Plugin for Lxr {
    const NAME: &'static str = "ELABS RLX";
    const VENDOR: &'static str = "ELABS";
    const URL: &'static str = "https://github.com/erica-synths/lxr-desktop-app";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[],
            aux_output_ports: &[new_nonzero_u32(2)],
            names: PortNames {
                layout: Some("Out 1 + Out 2"),
                main_input: None,
                main_output: Some("Out 1"),
                aux_inputs: &[],
                aux_outputs: &["Out 2"],
            },
        },
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[],
            aux_output_ports: &[],
            names: PortNames::const_default(),
        },
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(self.params.clone(), self.shared.clone())
    }

    fn initialize(
        &mut self,
        layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.has_aux_out = !layout.aux_output_ports.is_empty();
        self.resampler = Resampler::new(buffer_config.sample_rate);
        self.sample_rate = buffer_config.sample_rate;
        if self.engine.is_none() {
            self.engine = Engine::new();
            self.sent = [-1; NUM_ENGINE_PARAMS];
            self.sent_faders = [-1.0; 6];
            self.sent_fx = [f32::NAN; entropia::COUNT];
            self.sent_fx_mode = None;
            // a fresh engine has empty patterns and no song, mutes or recording
            self.params.seq.dirty.store(u64::MAX, Ordering::Release);
            self.params.seq.song_dirty.store(true, Ordering::Release);
            self.sent_mutes = None;
            self.sent_record = None;
            self.last_bpm = 0;
        }
        true
    }

    fn reset(&mut self) {
        self.resampler.reset();
        if let Some(engine) = self.engine.as_mut() {
            self.clock.stop(engine);
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.sync_params();
        // the Maestro gives the user's patterns and settings back when it stops, or before REC
        // could copy a variation into a pattern
        let maestro_on = self.params.maestro.on.load(Ordering::Acquire) && !self.params.seq.record.load(Ordering::Acquire);
        if let (false, true, Some(engine)) = (maestro_on, self.maestro.holding(), self.engine.as_mut()) {
            self.maestro.release(engine, &self.params.seq, &self.sent, &self.sent_fx);
        }
        self.sync_sequencer(context.transport().tempo);
        let pads = self.shared.pending.swap(0, Ordering::AcqRel);
        for v in (0..NUM_TRIGGERS).filter(|v| pads & (1 << v) != 0) {
            self.trigger(v, 60, 100);
        }

        let num_samples = buffer.samples();
        let transport = context.transport();
        let beats_per_sample = transport.tempo.unwrap_or(120.0) / 60.0 / self.sample_rate as f64;
        let seq_beat = match transport.pos_beats() {
            Some(beat) if transport.playing && self.params.seq_on.value() && self.engine.is_some() => Some(beat),
            _ => None,
        };
        if let Some(engine) = self.engine.as_mut() {
            let host_beat = transport.pos_beats().filter(|_| transport.playing);
            engine.fx_clock(host_beat, transport.tempo.unwrap_or(120.0));
        }
        if let (Some(beat), Some(engine), true) = (seq_beat, self.engine.as_mut(), maestro_on) {
            self.maestro.tick(beat, engine, &self.sent_fx);
        }
        match (seq_beat, self.engine.as_mut()) {
            (Some(beat), Some(engine)) => self.clock.begin_block(engine, beat),
            (None, Some(engine)) => self.clock.stop(engine),
            _ => {}
        }
        let main = buffer.as_slice();
        let mut aux_slice = match aux.outputs.first_mut() {
            Some(b) if self.has_aux_out => Some(b.as_slice()),
            _ => None,
        };

        let mut next_event = context.next_event();
        for i in 0..num_samples {
            while let Some(event) = next_event {
                if event.timing() as usize > i {
                    break;
                }
                self.handle_event(event);
                next_event = context.next_event();
            }
            if let (Some(beat), Some(engine)) = (seq_beat, self.engine.as_mut()) {
                let (maestro, params, sent, sent_fx) = (&mut self.maestro, &self.params, &self.sent, &self.sent_fx);
                let entropia = params.fx_mode.value();
                self.clock.advance_to(engine, beat + i as f64 * beats_per_sample, |engine, beat| {
                    maestro.on_beat(beat, engine, &params.maestro, &params.seq, sent, sent_fx, entropia);
                });
            }

            let gain = self.params.volume.smoothed.next();
            let [l1, r1, l2, r2] = match self.engine.as_mut() {
                Some(engine) => self.resampler.next(engine).map(|s| s * gain),
                None => [0.0; 4],
            };
            match aux_slice.as_mut() {
                Some(a) => {
                    main[0][i] = l1;
                    main[1][i] = r1;
                    a[0][i] = l2;
                    a[1][i] = r2;
                }
                // without a second output, Out 2 is mixed into Out 1 so no voice goes missing
                None => {
                    main[0][i] = l1 + l2;
                    main[1][i] = r1 + r2;
                }
            }
        }
        while let Some(event) = next_event {
            self.handle_event(event);
            next_event = context.next_event();
        }
        if let Some(beat) = seq_beat {
            self.clock.end_block(beat + num_samples as f64 * beats_per_sample);
        }
        self.publish_sequencer();

        ProcessStatus::KeepAlive
    }
}

impl Vst3Plugin for Lxr {
    // kept from when the plugin was called ELABS LXR, so sets made with it still find it
    const VST3_CLASS_ID: [u8; 16] = *b"ElabsLxrV2Drums!";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Drum];
}

nih_export_vst3!(Lxr);

#[cfg(test)]
mod tests {
    use super::morph_value;

    #[test]
    fn morph_interpolates_like_the_firmware() {
        assert_eq!(morph_value(100, 20, 0), 100);
        assert_eq!(morph_value(100, 20, 128), 60);
        assert_eq!(morph_value(0, 127, 255), 127);
        // x/256 never quite reaches b: on the LXR, full morph down from 127 stops at 1
        assert_eq!(morph_value(127, 0, 255), 1);
        for a in 0..=127u8 {
            for x in 0..=255u8 {
                assert_eq!(morph_value(a, a, x), a, "same values stay put");
            }
        }
    }
}
