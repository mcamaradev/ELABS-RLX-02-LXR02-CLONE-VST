#include "lxr_bridge.h"

#include <string.h>
#include "stm32f7xx_hal.h"
#include "Parameters.h"
#include "Preset/InitKit.h"
#include "Soundengine/config.h"
#include "Soundengine/DSPAudio/mixer.h"
#include "Soundengine/DSPAudio/DrumVoice.h"
#include "Soundengine/DSPAudio/Snare.h"
#include "Soundengine/DSPAudio/HiHat.h"
#include "Soundengine/DSPAudio/CymbalVoice.h"
#include "Soundengine/DSPAudio/modulationNode.h"
#include "Soundengine/MIDI/MidiParser.h"
#include "Soundengine/MIDI/MidiVoiceControl.h"
#include "Soundengine/MIDI/ParameterArray.h"
#include "menu/menu.h"
#include "menu/CcNr2Text.h"

_Static_assert(LXR_NUM_PARAMS == END_OF_SOUND_PARAMETERS, "parameter count changed in the firmware");
_Static_assert(LXR_BLOCK_FRAMES == OUTPUT_DMA_SIZE, "block size changed in the firmware");

extern uint8_t activeParameters[];
extern float lxr_faderVolume[6];
// lxr_entropia.c; the mixer writes the "Out 1" routing to its second buffer argument
void lxr_fx_init(void);
void lxr_fx_render(int16_t *dac1, int16_t *dac2);

// Parameters that are not sent to the engine as values:
// - PAR_BANK_CHANGE arrives at midiParser_ccHandler as CC 0, where paramNr = data1 - 1 wraps to
//   0xffff and midiParser_originalCcValues is written 64 KB out of bounds (crashes on macOS);
// - PAR_VOICE_LFO* only tells the menu which voice's targets to offer;
// - the modulation destinations are handled by send_mod_target().
static int is_ignored_param(uint16_t idx)
{
	return idx == PAR_BANK_CHANGE ||
	       (idx >= PAR_VOICE_LFO1 && idx <= PAR_VOICE_LFO6);
}

static int is_mod_target_param(uint16_t idx)
{
	return (idx >= PAR_VEL_DEST_1 && idx <= PAR_VEL_DEST_6) ||
	       (idx >= PAR_TARGET_LFO1 && idx <= PAR_TARGET_LFO6);
}

// Kits store LFO and velocity destinations as indices into the menu's modTargets table; the front
// panel translates them to parameter numbers (preset_sendDrumsetParameters) and CortexParser.c
// hands those to the modulation nodes.
static void send_mod_target(uint16_t idx, uint8_t value)
{
	if (value >= getNumModTargets())
		value = 0;
	const uint8_t dest = (uint8_t)modTargets[value].param;
	if (idx >= PAR_VEL_DEST_1 && idx <= PAR_VEL_DEST_6) {
		modNode_setDestination(&velocityModulators[idx - PAR_VEL_DEST_1], dest);
		return;
	}
	switch (idx - PAR_TARGET_LFO1) {
	case 0:
	case 1:
	case 2: modNode_setDestination(&voiceArray[idx - PAR_TARGET_LFO1].lfo.modTarget, dest); break;
	case 3: modNode_setDestination(&snareVoice.lfo.modTarget, dest); break;
	case 4: modNode_setDestination(&cymbalVoice.lfo.modTarget, dest); break;
	case 5: modNode_setDestination(&hatVoice.lfo.modTarget, dest); break;
	}
}

// Same path the front panel uses (frontPanel_sendData -> CortexParser -> midiParser_ccHandler).
static void send_param(uint16_t idx, uint8_t value)
{
	MidiMsg msg;
	memset(&msg, 0, sizeof msg);
	if (idx < 128) {
		msg.status = MIDI_CC;
		msg.data1 = (uint8_t)((idx + 1) & 0x7f); // front/cortex offset, see CortexParser.c
	} else {
		msg.status = MIDI_CC2;
		msg.data1 = (uint8_t)(idx - 128);
	}
	msg.data2 = value;
	midiParser_ccHandler(msg, 1, 0, 0.f);
}

void lxr_init(void)
{
	mixer_init();
	parameterArray_init();
	// On the STM32 address 0 is readable (ITCM), and the modulation code reads through the NULL
	// pointers of unused entries (e.g. destination 0 = "no target"). Give them a harmless target.
	static uint32_t null_param_sink;
	for (int i = 0; i < END_OF_SOUND_PARAMETERS; i++)
		if (!parameterArray[i].ptr)
			parameterArray[i].ptr = &null_param_sink;
	for (int i = 0; i < 6; i++)
		modNode_init(&velocityModulators[i]);
	memset(midi_MidiChannels, 0, 8);

	initDrumVoice();
	Snare_init();
	HiHat_init();
	Cymbal_init();
	lxr_seq_init();
	lxr_fx_init();

	for (uint16_t i = 0; i < END_OF_SOUND_PARAMETERS; i++) {
		activeParameters[i] = lxr_init_kit_value(i);
		if (is_mod_target_param(i))
			send_mod_target(i, activeParameters[i]);
		else if (!is_ignored_param(i))
			send_param(i, activeParameters[i]);
	}
}

void lxr_set_param(uint16_t idx, uint8_t value)
{
	if (idx >= END_OF_SOUND_PARAMETERS)
		return;
	activeParameters[idx] = value;
	if (is_mod_target_param(idx))
		send_mod_target(idx, value);
	else if (!is_ignored_param(idx))
		send_param(idx, value);
}

uint8_t lxr_get_param(uint16_t idx)
{
	return idx < END_OF_SOUND_PARAMETERS ? activeParameters[idx] : 0;
}

void lxr_set_fader(uint8_t voice, float volume)
{
	if (voice < 6)
		lxr_faderVolume[voice] = volume;
}

void lxr_note_on(uint8_t voice, uint8_t note, uint8_t velocity)
{
	if (voice < 7)
		voiceControl_noteOn(voice, note, velocity);
}

void lxr_render(float *out1_l, float *out1_r, float *out2_l, float *out2_r)
{
	int16_t dac1[OUTPUT_DMA_SIZE * 2], dac2[OUTPUT_DMA_SIZE * 2];
	lxr_fx_render(dac1, dac2);
	for (int i = 0; i < OUTPUT_DMA_SIZE; i++) {
		out1_l[i] = dac1[2 * i] * (1.f / 32768.f);
		out1_r[i] = dac1[2 * i + 1] * (1.f / 32768.f);
		out2_l[i] = dac2[2 * i] * (1.f / 32768.f);
		out2_r[i] = dac2[2 * i + 1] * (1.f / 32768.f);
	}
}

// The kit format is an 8 byte name followed by the parameter values.
uint8_t lxr_init_kit_value(uint16_t idx)
{
	return idx + 8 < INIT_KIT_SIZE && idx < END_OF_SOUND_PARAMETERS ? initKit[idx + 8] : 0;
}

// ---------------------------------------------------------------- sequencer
// The firmware's step sequencer (sequencer.c), clocked by the host instead of the STM32 timer.

#include <stdlib.h>
#include "Preset/SeqStep.h"
#include "Soundengine/Sequencer/sequencer.h"
#include "Soundengine/Sequencer/EuklidGenerator.h"

_Static_assert(sizeof(Step) == 7, "Step layout changed");
_Static_assert(sizeof(StepTiming) == 1, "StepTiming layout changed");
_Static_assert(LXR_PATTERN_BYTES == 8 + 6 + NUM_TRACKS * (NUM_STEPS * (sizeof(Step) + 1)) + NUM_TRACKS * NUM_BARS * 2 + 1 + NUM_TRACKS + 1, "pattern file size");

extern float seq_shuffle;

void lxr_seq_init(void)
{
	seq_init();
	euklid_init();
}

// preset_readPatternData() for a .pat file in memory (versions 1-3).
int lxr_seq_read_pattern(uint8_t nr, const uint8_t *d, size_t len)
{
	if (nr >= NUM_PATTERN || len < 8)
		return 0;
	size_t pos = 8; // name
	int version = 1;
	if (len >= 14 && memcmp(d + 8, "PatV", 4) == 0) {
		char v[3] = {(char)d[12], (char)d[13], 0};
		version = atoi(v);
		pos = 14;
	}
	int ok = 0;
#define NEED(n) if (pos + (n) > len) goto done
	for (int trk = 0; trk < NUM_TRACKS; trk++) {
		NEED(sizeof(Step) * NUM_STEPS);
		memcpy(&seq_patternSet.seq_subStepPattern[nr][trk][0], d + pos, sizeof(Step) * NUM_STEPS);
		pos += sizeof(Step) * NUM_STEPS;
		if (version > 2) {
			NEED(NUM_STEPS);
			memcpy(&seq_patternSet.seq_stepTiming[nr][trk][0], d + pos, NUM_STEPS);
			pos += NUM_STEPS;
		} else {
			for (int s = 0; s < NUM_STEPS; s++) {
				seq_patternSet.seq_stepTiming[nr][trk][s].flam = 0;
				seq_patternSet.seq_stepTiming[nr][trk][s].shift = 7; // no shift
			}
		}
	}
	for (int trk = 0; trk < NUM_TRACKS; trk++)
		for (int bar = 0; bar < NUM_BARS; bar++) {
			NEED(2);
			seq_patternSet.seq_mainSteps[bar][nr][trk] = (uint16_t)(d[pos] | (d[pos + 1] << 8));
			pos += 2;
		}
	NEED(1);
	seq_patternSet.shuffle[nr] = d[pos++];
	NEED(NUM_TRACKS);
	for (int trk = 0; trk < NUM_TRACKS; trk++)
		seq_patternSet.length[nr][trk] = d[pos++];
	if (version > 1) {
		NEED(1);
		seq_patternSet.kitNr[nr] = d[pos++];
	}
	ok = 1;
#undef NEED
done:
	// switching patterns restores the old one from the backup buffer (the unit's undo for unsaved
	// edits), so the active pattern's backup has to follow what was loaded
	if (nr == seq_activePattern) {
		seq_copyPatternToBackupBuffer(nr);
		seq_setShuffle(seq_patternSet.shuffle[nr] / 127.f);
	}
	return ok;
}

// A version 3 .pat file of pattern nr (LXR_PATTERN_BYTES, empty name).
void lxr_seq_write_pattern(uint8_t nr, uint8_t *out)
{
	if (nr >= NUM_PATTERN)
		return;
	size_t pos = 0;
	memset(out, 0, 8);
	pos = 8;
	memcpy(out + pos, "PatV03", 6);
	pos += 6;
	for (int trk = 0; trk < NUM_TRACKS; trk++) {
		memcpy(out + pos, &seq_patternSet.seq_subStepPattern[nr][trk][0], sizeof(Step) * NUM_STEPS);
		pos += sizeof(Step) * NUM_STEPS;
		memcpy(out + pos, &seq_patternSet.seq_stepTiming[nr][trk][0], NUM_STEPS);
		pos += NUM_STEPS;
	}
	for (int trk = 0; trk < NUM_TRACKS; trk++)
		for (int bar = 0; bar < NUM_BARS; bar++) {
			const uint16_t v = seq_patternSet.seq_mainSteps[bar][nr][trk];
			out[pos++] = (uint8_t)(v & 0xff);
			out[pos++] = (uint8_t)(v >> 8);
		}
	out[pos++] = seq_patternSet.shuffle[nr];
	for (int trk = 0; trk < NUM_TRACKS; trk++)
		out[pos++] = seq_patternSet.length[nr][trk];
	out[pos++] = seq_patternSet.kitNr[nr];
}

void lxr_seq_start(void)
{
	seq_resetPosition();
	seq_setRunning(1);
}

void lxr_seq_stop(void)
{
	seq_setRunning(0);
	seq_resetPosition();
}

// One tick of the 96 PPQ clock (seq_tick() with the internal clock): a step every 24 ticks, the
// flam/sub-step/shift processing on every tick.
void lxr_seq_tick(void)
{
	if (seq_prescaleCounter == 0)
		seq_nextStep();
	seq_processFlam(seq_prescaleCounter);
	if (++seq_prescaleCounter >= 24)
		seq_prescaleCounter = 0;
}

// Advance whole steps without playing them, to line up with a host position after a start or jump.
void lxr_seq_skip_steps(uint32_t steps, uint8_t tick_in_step)
{
	uint8_t muted = 0;
	for (uint8_t t = 0; t < NUM_TRACKS; t++)
		if (seq_isTrackMuted(t))
			muted |= 1 << t;
	for (uint8_t t = 0; t < NUM_TRACKS; t++)
		seq_setMute(t, 1);
	for (uint32_t i = 0; i < steps; i++)
		seq_nextStep();
	seq_setMute(7, 0); // 7 = unmute all
	for (uint8_t t = 0; t < NUM_TRACKS; t++)
		if (muted & (1 << t))
			seq_setMute(t, 1);
	seq_prescaleCounter = tick_in_step % 24;
}

void lxr_seq_set_pattern(uint8_t nr) { seq_setNextPattern(nr); }
uint8_t lxr_seq_active_pattern(void) { return seq_activePattern; }
uint8_t lxr_seq_pending_pattern(void) { return seq_pendingPattern; }
uint8_t lxr_seq_step_index(uint8_t track) { return track < NUM_TRACKS ? seq_getStepIndex(track) : 0; }
uint8_t lxr_seq_running(void) { return seq_isRunning(); }
float lxr_seq_shuffle(void) { return seq_shuffle; }
void lxr_set_bpm(uint16_t bpm) { seq_setBpm(bpm); }

// Voices triggered since the last call (bit per trigger 0-6), from voiceControl_noteOn's front
// panel message (see lxr_stubs.c).
extern volatile uint8_t lxr_triggered;
uint8_t lxr_take_triggered(void)
{
	const uint8_t t = lxr_triggered;
	lxr_triggered = 0;
	return t;
}

// For tests: the firmware's euclid generator for track 0 of pattern 63 (euklid_setSteps /
// euklid_setRotation as the menu calls them), as 64 step bits.
uint64_t lxr_euklid(uint8_t length, uint8_t steps, uint8_t rotation)
{
	euklid_setLength(0, length);
	euklid_setRotation(0, 0, 63);
	euklid_setSteps(0, steps, 63);
	euklid_setRotation(0, rotation, 63);
	uint64_t bits = 0;
	for (int bar = 0; bar < NUM_BARS; bar++)
		bits |= (uint64_t)seq_patternSet.seq_mainSteps[bar][63][0] << (16 * bar);
	return bits;
}

// Mutes: one bit per track (seq_setMute()).
void lxr_seq_set_mutes(uint8_t mask)
{
	for (uint8_t t = 0; t < NUM_TRACKS; t++)
		seq_setMute(t, (mask >> t) & 1);
}

// Live recording: while on and running, lxr_seq_record() writes notes into the playing pattern,
// quantised to the nearest step (seq_addNote()).
void lxr_seq_set_record(uint8_t on) { seq_setRecordingMode(on); }
void lxr_seq_record(uint8_t track, uint8_t velocity, uint8_t note) { seq_addNote(track, velocity, note); }

// Song mode: patterns played in order, then from the top again (SONG_LOOP) or stopping (SONG_END).
extern uint8_t *seq_activeSong;
void lxr_seq_set_song(const uint8_t *patterns, uint8_t count, uint8_t loop, uint8_t enabled)
{
	if (count > MAX_SONG_LENGTH - 1)
		count = MAX_SONG_LENGTH - 1;
	for (uint8_t i = 0; i < count; i++)
		seq_activeSong[i] = patterns[i] < MAX_PATTERN ? patterns[i] : 0;
	for (uint8_t i = count; i < MAX_SONG_LENGTH; i++)
		seq_activeSong[i] = loop ? SONG_LOOP : SONG_END;
	seq_setSongModeEnabled(enabled && count > 0);
}
