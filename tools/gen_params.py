#!/usr/bin/env python3
"""Builds src/param_table.rs from the firmware's parameter tables.

Inputs (written by gen_params.sh from small C programs compiled against the firmware headers):
  enum_names.txt   "<value>\t<PAR_NAME>" for every entry of ParamEnums (Parameters.h)
  params_dump.txt  "<idx>\t<dtype>\t<menuId>\t<value names...>" from menu.c's parameter_dtypes
"""
import re
import sys

enum_path, dump_path, pages_path, init_kit_path, out_path = sys.argv[1:6]
num_mod_targets = sum(1 for l in open(pages_path) if l.startswith("MODTARGET\t"))

names = {}
for line in open(enum_path):
    val, name = line.rstrip("\n").split("\t")
    val = int(val)
    # PAR_MOD_WHEEL is an alias of PAR_OSC_WAVE_DRUM1; prefer the sound name
    if val not in names or names[val] == "PAR_MOD_WHEEL":
        names[val] = name

rows, dtypes = [], {}
for line in open(dump_path):
    f = line.rstrip("\n").split("\t")
    if f[0] == "END_OF_SOUND_PARAMETERS":
        count = int(f[1])
    elif f[0] == "DTYPES":
        dtypes = {k: int(v) for k, v in (kv.split("=") for kv in f[1].split())}
    else:
        rows.append((int(f[0]), int(f[1]), int(f[2]), [s for s in f[3:] if s]))

D = dtypes
VOICES = ["Drum 1", "Drum 2", "Drum 3", "Snare", "Cymbal", "Hihat"]

# Parameters the plugin does not expose: MIDI/NRPN plumbing and unused slots.
SKIP = {"PAR_NONE", "PAR_NRPN_DATA_ENTRY_COARSE", "PAR_NRPN_FINE", "PAR_NRPN_COARSE",
        "PAR_BANK_CHANGE", "PAR_MORPH_KIT_DEST"}
SKIP_RE = re.compile(r"^PAR_UNUSED_\d$")
# modulation destinations: an index into the firmware's modTargets table
MOD_TARGET_RE = re.compile(r"^PAR_(VEL_DEST_\d|TARGET_LFO\d)$")

# Names that don't follow the "<thing><voice number>" pattern.
FIXED = {
    "PAR_NOISE_FREQ1": ("Snare", "Noise Freq"),
    "PAR_MIX1": ("Snare", "Noise Mix"),
    "PAR_MOD_OSC_F1_CYM": ("Cymbal", "Mod Osc 1 Freq"),
    "PAR_MOD_OSC_F2_CYM": ("Cymbal", "Mod Osc 2 Freq"),
    "PAR_MOD_OSC_GAIN1_CYM": ("Cymbal", "Mod Osc 1 Gain"),
    "PAR_MOD_OSC_GAIN2_CYM": ("Cymbal", "Mod Osc 2 Gain"),
    "PAR_MOD_OSC_F1": ("Hihat", "Mod Osc 1 Freq"),
    "PAR_MOD_OSC_F2": ("Hihat", "Mod Osc 2 Freq"),
    "PAR_MOD_OSC_GAIN1": ("Hihat", "Mod Osc 1 Gain"),
    "PAR_MOD_OSC_GAIN2": ("Hihat", "Mod Osc 2 Gain"),
    "PAR_OSC_WAVE_DRUM1": ("Drum 1", "Osc Wave"),
    "PAR_OSC_WAVE_DRUM2": ("Drum 2", "Osc Wave"),
    "PAR_OSC_WAVE_DRUM3": ("Drum 3", "Osc Wave"),
    "PAR_OSC_WAVE_SNARE": ("Snare", "Osc Wave"),
    "PAR_WAVE1_CYM": ("Cymbal", "Osc Wave"),
    "PAR_WAVE2_CYM": ("Cymbal", "Mod Osc 1 Wave"),
    "PAR_WAVE3_CYM": ("Cymbal", "Mod Osc 2 Wave"),
    "PAR_WAVE1_HH": ("Hihat", "Osc Wave"),
    "PAR_WAVE2_HH": ("Hihat", "Mod Osc 1 Wave"),
    "PAR_WAVE3_HH": ("Hihat", "Mod Osc 2 Wave"),
    "PAR_MOD_WAVE_DRUM1": ("Drum 1", "FM Wave"),
    "PAR_MOD_WAVE_DRUM2": ("Drum 2", "FM Wave"),
    "PAR_MOD_WAVE_DRUM3": ("Drum 3", "FM Wave"),
    "PAR_VELOD6_CLOSED": ("Hihat", "Decay Closed"),
    "PAR_VELOD6_OPEN": ("Hihat", "Decay Open"),
    "PAR_SNARE_DISTORTION": ("Snare", "Distortion"),
    "PAR_CYMBAL_DISTORTION": ("Cymbal", "Distortion"),
    "PAR_HAT_DISTORTION": ("Hihat", "Distortion"),
    "PAR_VOICE_DECIMATION_ALL": ("Master", "Sample Rate"),
}
THING = {
    "COARSE": "Coarse", "FINE": "Fine", "FILTER_FREQ": "Filter Freq", "RESO": "Resonance",
    "VELOA": "Attack", "VELOD": "Decay", "VOL_SLOPE": "Amp Slope", "REPEAT": "Repeat",
    "MOD_EG": "Pitch Decay", "MODAMNT": "Pitch Mod", "PITCH_SLOPE": "Pitch Slope",
    "FMAMNT": "FM Amount", "FM_FREQ": "FM Freq", "VOL": "Volume", "PAN": "Pan",
    "DRIVE": "Drive", "VOICE_DECIMATION": "Sample Rate", "FREQ_LFO": "LFO Freq",
    "AMOUNT_LFO": "LFO Amount", "FILTER_DRIVE": "Filter Drive", "MIX_MOD": "FM Mode",
    "VOLUME_MOD_ON_OFF": "Velocity to Volume", "VELO_MOD_AMT": "Velocity Mod Amount",
    "WAVE_LFO": "LFO Wave", "RETRIGGER_LFO": "LFO Retrigger", "SYNC_LFO": "LFO Sync",
    "OFFSET_LFO": "LFO Phase", "FILTER_TYPE": "Filter Type", "TRANS": "Transient",
    "PWM": "PWM", "AUDIO_OUT": "Output", "VEL_DEST": "Velocity Dest",
    "TARGET_LFO": "LFO Dest", "VOICE_LFO": "LFO Dest Voice",
}
FX = {"PAR_FX_TYPE": "Type", "PAR_FX_ROUTING": "Output", "PAR_FX_DRY_WET": "Dry/Wet",
      "PAR_FX_DIST_TYPE": "Dist Type", "PAR_FX_P1": "Param 1", "PAR_FX_P2": "Param 2",
      "PAR_FX_P3": "Param 3", "PAR_FX_P4": "Param 4", "PAR_FX_RATIO": "Comp Ratio",
      "PAR_FX_WAVE": "Mod Wave", "PAR_FX_REVERB_TYPE": "Reverb Type",
      "PAR_FX_DELAY_TYPE": "Delay Type"}


def pretty(name):
    if name in FIXED:
        return FIXED[name]
    if name in FX:
        return ("FX", FX[name])
    base = name[4:]
    m = re.match(r"^TRANS(\d)_(VOL|WAVE|FREQ)$", base)
    if m:
        return (VOICES[int(m.group(1)) - 1], "Transient " + m.group(2).title())
    m = re.match(r"^(.*?)(\d)$", base)
    if m and m.group(1).rstrip("_") in THING:
        return (VOICES[int(m.group(2)) - 1], THING[m.group(1).rstrip("_")])
    raise SystemExit(f"no name rule for {name}")


def value_range(name, dt, items):
    if MOD_TARGET_RE.match(name):
        return 0, num_mod_targets - 1
    if dt == D["VLFO"]:
        return 1, 6
    if dt == D["MENU"]:
        return 0, len(items) - 1
    return {
        D["B255"]: (0, 255), D["B127"]: (0, 127), D["PM63"]: (0, 127), D["PM100"]: (0, 100),
        D["0B100"]: (0, 100), D["ONOFF"]: (0, 1), D["0b1"]: (0, 1), D["1B16"]: (1, 16),
        D["1B64"]: (1, 64), D["0B15"]: (0, 15), D["NOTE"]: (0, 127),
    }[dt]


# Coarse/FM freq values are shown relative to 60 on the LXR, the other +/-63 values relative to 63.
CENTER60 = re.compile(r"^PAR_(COARSE\d|FM_FREQ\d|MOD_OSC_F\d(_CYM)?)$")

out = ["// Generated by tools/gen_params.py from the LXR firmware tables. Do not edit.",
       "",
       "pub enum Display {",
       "    Plain,",
       "    Offset(i32),",
       "    Names(&'static [&'static str]),",
       "    /// index into ui_pages::MOD_TARGETS",
       "    ModTarget,",
       "}",
       "",
       "pub struct ParamInfo {",
       "    pub index: u16,",
       "    pub group: &'static str,",
       "    pub name: &'static str,",
       "    pub min: i32,",
       "    pub max: i32,",
       "    pub display: Display,",
       "}",
       "",
       f"pub const NUM_ENGINE_PARAMS: usize = {count};",
       "",
       "pub static PARAMS: &[ParamInfo] = &["]
for idx, dt, menu_id, items in rows:
    name = names[idx]
    if name in SKIP or SKIP_RE.match(name):
        continue
    group, label = pretty(name)
    lo, hi = value_range(name, dt, items)
    if lo == hi:  # single-choice menus (e.g. the one reverb type) have nothing to set
        continue
    if MOD_TARGET_RE.match(name):
        disp = "Display::ModTarget"
    elif dt == D["MENU"]:
        disp = "Display::Names(&[" + ", ".join('"%s"' % s.strip() for s in items) + "])"
    elif dt == D["PM63"]:
        disp = "Display::Offset(%d)" % (60 if CENTER60.match(name) else 63)
    elif dt == D["0b1"] or dt == D["ONOFF"]:
        disp = 'Display::Names(&["Off", "On"])'
    else:
        disp = "Display::Plain"
    out.append(f'    ParamInfo {{ index: {idx}, group: "{group}", name: "{group} {label}", '
               f"min: {lo}, max: {hi}, display: {disp} }}, // {name}")
out.append("];")
# the firmware's init kit (initKit.c): an 8 byte name, then the parameter values
init_kit_src = open(init_kit_path).read()
init_kit = [int(b, 16) for b in re.findall(r"0x([0-9a-fA-F]{2})", init_kit_src.split("=", 1)[1])][8:]
out.append("")
out.append("/// Parameter values of the firmware's init kit (initKit.c without its name).")
out.append("pub static INIT_KIT: &[u8] = &[" + ", ".join(str(b) for b in init_kit) + "];")
open(out_path, "w").write("\n".join(out) + "\n")
print(f"{out_path}: {sum(1 for l in out if l.startswith('    ParamInfo'))} parameters")
