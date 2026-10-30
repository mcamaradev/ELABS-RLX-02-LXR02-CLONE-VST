// C API between the LXR V2 sound engine (firmware sources, unmodified) and the plugin.
// The engine lives in C globals, so there is exactly one engine per loaded binary.
#ifndef LXR_BRIDGE_H
#define LXR_BRIDGE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Frames rendered by one lxr_render() call (the firmware's DMA half-buffer size).
#define LXR_BLOCK_FRAMES 64
// Sample rate the engine's tables and coefficients are tuned for (the hardware's real I2S rate).
#define LXR_ENGINE_RATE 44140.0f
// Number of front-panel sound parameters (0 .. LXR_NUM_PARAMS-1).
#define LXR_NUM_PARAMS 247

// Initialise all voices and FX and load the firmware's built-in init kit.
void lxr_init(void);

// Set a front-panel sound parameter (same numbering and value range as the LXR's menu).
void lxr_set_param(uint16_t idx, uint8_t value);
// Current value of a front-panel parameter (initialised from the init kit).
uint8_t lxr_get_param(uint16_t idx);
// Value of a parameter in the firmware's init kit (usable before lxr_init()).
uint8_t lxr_init_kit_value(uint16_t idx);

// Volume fader of a voice (0-5), 0..1. On the hardware these are the physical faders.
void lxr_set_fader(uint8_t voice, float volume);

// Trigger a voice: 0-2 Drum 1-3, 3 Snare, 4 Cymbal, 5 Hihat closed, 6 Hihat open.
// note 60 plays the voice at its programmed pitch.
void lxr_note_on(uint8_t voice, uint8_t note, uint8_t velocity);

// Render LXR_BLOCK_FRAMES frames of both stereo outputs (DAC 1 = out1, DAC 2 = out2), -1..1.
void lxr_render(float *out1_l, float *out1_r, float *out2_l, float *out2_r);

// ---- ENTROPIA MOD FX (lxr_entropia.c): per-voice sends to delay, ring and reverb, then a master
// filter, drive and compressor, all at once. Off = the LXR's own single FX bus (CLASSIC).
enum {
	// send level 0..1 per bus (delay, ring, reverb) and voice (0-5): LXR_FX_SEND + bus * 6 + voice
	LXR_FX_SEND = 0,
	LXR_FX_DELAY_TYPE = 18, // 0 normal, 1 ping-pong
	LXR_FX_DELAY_TIME,
	LXR_FX_DELAY_RANGE,
	LXR_FX_DELAY_TONE,
	LXR_FX_DELAY_FEEDBACK,
	LXR_FX_DELAY_RETURN,
	LXR_FX_RING_WAVE, // 0 sine, 1 tri, 2 saw, 3 rect
	LXR_FX_RING_FREQ,
	LXR_FX_RING_RETURN,
	LXR_FX_VERB_SIZE,
	LXR_FX_VERB_DECAY,
	LXR_FX_VERB_TONE,
	LXR_FX_VERB_VARIATION,
	LXR_FX_VERB_RETURN,
	LXR_FX_FILTER, // 0 low pass closed .. 0.5 open .. 1 high pass closed
	LXR_FX_FILTER_RESO,
	LXR_FX_DRIVE_TYPE, // 0 tube, 1 fold, 2 rat
	LXR_FX_DRIVE,
	LXR_FX_DRIVE_COLOR,
	LXR_FX_DRIVE_TONE,
	LXR_FX_DRIVE_LEVEL,
	LXR_FX_DRIVE_MIX,
	LXR_FX_COMP_RATIO, // 0-5: 1, 2, 3, 4, 6, 8 : 1
	LXR_FX_COMP_ATTACK,
	LXR_FX_COMP_RELEASE,
	LXR_FX_COMP_THRESH,
	LXR_FX_COMP_GAIN,
	LXR_FX_COMP_MIX,
	// an LFO on the ring modulator's frequency
	LXR_FX_RING_LFO_WAVE, // 0 sine, 1 tri, 2 saw, 3 square, 4 random
	LXR_FX_RING_LFO_RATE, // 0.05-20 Hz
	LXR_FX_RING_LFO_DEPTH, // up to 3 octaves either way
	LXR_FX_RING_LFO_SYNC, // 0 free (RATE), else one of the LXR_FX_LFO_SYNCS note lengths
	LXR_FX_COUNT
};
// free + 4 bars ... 1/32 (lxr_entropia.c lfo_beats, src/entropia.rs LFO_SYNCS)
#define LXR_FX_LFO_SYNCS 16
void lxr_fx_set_mode(uint8_t entropia);
uint8_t lxr_fx_mode(void);
void lxr_fx_set(uint16_t id, float value);
// the host's position in beats (negative: stopped) and tempo, once per host block, for the synced LFO
void lxr_fx_clock(double beat, float bpm);

// ---- sequencer (the firmware's, clocked by the host)
// A version 3 .pat file: name, "PatV03", 7 tracks x 64 steps x (7 byte step + 1 byte timing),
// 7 x 4 bars of 16 step bits, shuffle, 7 track lengths, kit number.
#define LXR_PATTERN_BYTES 3663
#define LXR_NUM_PATTERNS 64

void lxr_seq_init(void);
int lxr_seq_read_pattern(uint8_t nr, const uint8_t *data, size_t len);
void lxr_seq_write_pattern(uint8_t nr, uint8_t *out);
void lxr_seq_start(void);
void lxr_seq_stop(void);
// one tick of the 96 PPQ clock
void lxr_seq_tick(void);
void lxr_seq_skip_steps(uint32_t steps, uint8_t tick_in_step);
// switches at the end of the pattern while running, right away when stopped
void lxr_seq_set_pattern(uint8_t nr);
uint8_t lxr_seq_active_pattern(void);
uint8_t lxr_seq_pending_pattern(void);
uint8_t lxr_seq_step_index(uint8_t track);
uint8_t lxr_seq_running(void);
// 0..1, delays every second step by up to 16 ticks
float lxr_seq_shuffle(void);
void lxr_set_bpm(uint16_t bpm);
uint8_t lxr_take_triggered(void);
// mute bit per track
void lxr_seq_set_mutes(uint8_t mask);
// live recording into the playing pattern
void lxr_seq_set_record(uint8_t on);
void lxr_seq_record(uint8_t track, uint8_t velocity, uint8_t note);
// song mode: up to 63 patterns in order, then loop or stop
void lxr_seq_set_song(const uint8_t *patterns, uint8_t count, uint8_t loop, uint8_t enabled);
// for tests: the firmware euclid generator's 64 step bits
uint64_t lxr_euklid(uint8_t length, uint8_t steps, uint8_t rotation);

#ifdef __cplusplus
}
#endif

#endif
