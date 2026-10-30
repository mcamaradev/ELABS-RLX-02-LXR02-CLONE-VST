// ENTROPIA MOD FX: the plugin's second FX mode (a modification of the LXR's FX). The LXR has one FX bus that runs one effect at a time;
// here every voice sends to a delay, a ring modulator and a reverb at once, and everything goes
// through a master filter, drive and compressor. The effects are the firmware's own (Delay.c,
// Ringmod.c, Reverb.c, Distortion.c, Compressor.c); only the filter is new.
//
// The mixer hands every voice's block to lxr_voice_block() (patch 0004). While ENTROPIA is on, the
// firmware's FX bus is switched off for the block, out 2 is mixed into out 1, and out 2 stays silent.

#include "lxr_bridge.h"

#include <math.h>
#include <string.h>
#include "stm32f7xx_hal.h"
#include "Soundengine/FxBus.h"
#include "Soundengine/DSPAudio/mixer.h"
#include "Soundengine/DSPAudio/DrumVoice.h"
#include "Soundengine/DSPAudio/Snare.h"
#include "Soundengine/DSPAudio/HiHat.h"
#include "Soundengine/DSPAudio/CymbalVoice.h"
#include "Soundengine/DSPAudio/FX/FxRamBuffer.h"

#define N OUTPUT_DMA_SIZE
#define BUSES 3
#define VOICES 6

static uint8_t ent_on;
static float ent_value[LXR_FX_COUNT];
static float send_bus[BUSES][N * 2];

static Delay delay;
static Ringmod ring;
static Reverb reverb;
static __fp16 reverb_buffer[FX_BUFFER_SIZE + 1];
static MasterDistortion drive;
static Compressor comp;

// the ring modulator's LFO: phase 0..1, the random wave's held value and generator
static float lfo_phase, lfo_hold;
static uint32_t lfo_seed = 0x2545f491;
// synced: the cycle in beats per LXR_FX_RING_LFO_SYNC setting, and the host's clock
static const float lfo_beats[LXR_FX_LFO_SYNCS] = {
	0.f, 16.f, 8.f, 4.f, 3.f, 2.f, 1.5f, 4.f / 3.f, 1.f, 0.75f, 2.f / 3.f, 0.5f, 1.f / 3.f, 0.25f, 1.f / 6.f, 0.125f,
};
static double clock_beat = -1.;
static float clock_bpm = 120.f;
static uint8_t clock_fresh;

// master filter: a stereo state variable filter (TPT), low pass left of centre, high pass right
static float svf_ic1[2], svf_ic2[2], svf_g;

static float sat16(float x)
{
	return x > 32767.f ? 32767.f : x < -32768.f ? -32768.f : x;
}

static void voice_pan(uint8_t voice, float *l, float *r)
{
	switch (voice) {
	case 0:
	case 1:
	case 2: *l = voiceArray[voice].triggeredPanL; *r = voiceArray[voice].triggeredPanR; break;
	case 3: *l = snareVoice.triggeredPanL; *r = snareVoice.triggeredPanR; break;
	case 4: *l = cymbalVoice.triggeredPanL; *r = cymbalVoice.triggeredPanR; break;
	default: *l = hatVoice.triggeredPanL; *r = hatVoice.triggeredPanR; break;
	}
}

void lxr_voice_block(uint8_t voice, const int16_t *data)
{
	if (!ent_on || voice >= VOICES)
		return;
	float pl, pr;
	voice_pan(voice, &pl, &pr);
	for (int b = 0; b < BUSES; b++) {
		const float send = ent_value[LXR_FX_SEND + b * VOICES + voice];
		if (send <= 0.f)
			continue;
		float *bus = send_bus[b];
		for (int i = 0; i < N; i++) {
			bus[2 * i] += data[i] * pl * send;
			bus[2 * i + 1] += data[i] * pr * send;
		}
	}
}

void lxr_fx_init(void)
{
	memset(ent_value, 0, sizeof ent_value);
	Delay_init(&delay);
	Ringmod_init(&ring);
	reverb.reverbBufferSram = reverb_buffer;
	Reverb_init(&reverb);
	Distortion_init(&drive);
	Compressor_init(&comp);
	Compressor_setRatio(&comp, 0);
	static const struct { uint16_t id; float v; } defaults[] = {
		{LXR_FX_DELAY_TIME, 0.4f}, {LXR_FX_DELAY_RANGE, 0.5f}, {LXR_FX_DELAY_TONE, 0.6f},
		{LXR_FX_DELAY_FEEDBACK, 0.4f}, {LXR_FX_DELAY_RETURN, 1.f},
		{LXR_FX_RING_FREQ, 0.3f}, {LXR_FX_RING_RETURN, 1.f}, {LXR_FX_RING_LFO_RATE, 0.4f},
		{LXR_FX_VERB_SIZE, 0.5f}, {LXR_FX_VERB_DECAY, 0.5f}, {LXR_FX_VERB_TONE, 0.5f},
		{LXR_FX_VERB_VARIATION, 0.5f}, {LXR_FX_VERB_RETURN, 1.f},
		{LXR_FX_FILTER, 0.5f},
		{LXR_FX_DRIVE, 0.3f}, {LXR_FX_DRIVE_COLOR, 1.f}, {LXR_FX_DRIVE_LEVEL, 0.5f},
		{LXR_FX_COMP_RATIO, 3.f}, {LXR_FX_COMP_ATTACK, 0.1f}, {LXR_FX_COMP_RELEASE, 0.3f},
		{LXR_FX_COMP_THRESH, 0.7f},
	};
	for (size_t i = 0; i < sizeof defaults / sizeof *defaults; i++)
		lxr_fx_set(defaults[i].id, defaults[i].v);
}

void lxr_fx_set_mode(uint8_t entropia)
{
	if (entropia == ent_on)
		return;
	// the firmware's delay and ENTROPIA's share the FX RAM: start each from silence
	memset(FX_BUFFER, 0, sizeof(__fp16) * (FX_BUFFER_SIZE + 1));
	ent_on = entropia;
}

void lxr_fx_set(uint16_t id, float v)
{
	if (id >= LXR_FX_COUNT)
		return;
	ent_value[id] = v;
	switch (id) {
	case LXR_FX_DELAY_TIME: Delay_setDelayTime(&delay, v); break;
	case LXR_FX_DELAY_RANGE: Delay_setRange(&delay, v); break;
	case LXR_FX_DELAY_TONE: Delay_setFilter(&delay, v); break;
	case LXR_FX_DELAY_FEEDBACK: Delay_setFeedback(&delay, v); break;
	case LXR_FX_DELAY_TYPE: {
		const uint8_t t = v > 0.5f ? DELAY_PINGPONG : DELAY_NORMAL;
		if (t != delay.delayType) {
			memset(FX_BUFFER, 0, sizeof(__fp16) * (FX_BUFFER_SIZE + 1));
			delay.delayType = t;
		}
		break;
	}
	case LXR_FX_RING_WAVE: Ringmod_setWave(&ring, (uint8_t)v > REC ? REC : (uint8_t)v); break;
	case LXR_FX_VERB_SIZE: Reverb_setDelayTime(&reverb, v); break;
	case LXR_FX_VERB_DECAY: Reverb_setFeedback(&reverb, v); break;
	case LXR_FX_VERB_TONE: Reverb_setComposition(&reverb, v); break;
	case LXR_FX_VERB_VARIATION: Reverb_setVariation(&reverb, v); break;
	case LXR_FX_DRIVE_TYPE: drive.distortionType = (uint8_t)v > DISTORTION_RAT ? DISTORTION_RAT : (uint8_t)v; break;
	case LXR_FX_DRIVE: Distortion_setDrive(&drive, v); break;
	case LXR_FX_DRIVE_COLOR: Distortion_setParam1(&drive, v); break;
	case LXR_FX_DRIVE_TONE: Distortion_setParam2(&drive, v); break;
	case LXR_FX_DRIVE_LEVEL: Distortion_setParam3(&drive, v); break;
	case LXR_FX_COMP_RATIO: Compressor_setRatio(&comp, (uint8_t)v); break;
	case LXR_FX_COMP_ATTACK: Compressor_setAttack(&comp, v); break;
	case LXR_FX_COMP_RELEASE: Compressor_setRelease(&comp, v); break;
	case LXR_FX_COMP_THRESH: Compressor_setTreshold(&comp, v); break;
	case LXR_FX_COMP_GAIN: Compressor_setGain(&comp, v); break;
	default: break;
	}
}

void lxr_fx_clock(double beat, float bpm)
{
	clock_beat = beat;
	clock_bpm = bpm > 1.f ? bpm : 120.f;
	clock_fresh = 1;
}

uint8_t lxr_fx_mode(void)
{
	return ent_on;
}

// The ring modulator's frequency for this block: the FREQ setting moved by the LFO, in octaves.
// Synced, the LFO runs at the host's tempo and, while the host plays, follows its position.
static void ring_lfo(void)
{
	int sync = (int)ent_value[LXR_FX_RING_LFO_SYNC];
	sync = sync < 0 ? 0 : sync >= LXR_FX_LFO_SYNCS ? LXR_FX_LFO_SYNCS - 1 : sync;
	float step;
	if (sync) {
		const float beats = lfo_beats[sync];
		if (clock_fresh && clock_beat >= 0.) {
			// jump to the host's phase after a start or locate, else only lean towards it
			const float target = (float)fmod(clock_beat / beats, 1.);
			float d = target - lfo_phase;
			d -= roundf(d);
			lfo_phase += fabsf(d) > 0.1f ? d : d * 0.5f;
			lfo_phase -= floorf(lfo_phase);
		}
		step = clock_bpm / 60.f / beats * N / LXR_ENGINE_RATE;
	} else {
		step = 0.05f * powf(400.f, ent_value[LXR_FX_RING_LFO_RATE]) * N / LXR_ENGINE_RATE;
	}
	clock_fresh = 0;
	lfo_phase += step;
	if (lfo_phase >= 1.f) {
		lfo_phase -= floorf(lfo_phase);
		lfo_seed = lfo_seed * 1664525u + 1013904223u;
		lfo_hold = (lfo_seed >> 8) / (float)(1 << 23) - 1.f;
	}
	float lfo;
	switch ((int)ent_value[LXR_FX_RING_LFO_WAVE]) {
	case 0: lfo = sinf(2.f * (float)M_PI * lfo_phase); break;
	case 1: lfo = 1.f - 4.f * fabsf(lfo_phase - 0.5f); break;
	case 2: lfo = 1.f - 2.f * lfo_phase; break;
	case 3: lfo = lfo_phase < 0.5f ? 1.f : -1.f; break;
	default: lfo = lfo_hold; break;
	}
	const float hz = ent_value[LXR_FX_RING_FREQ] * 880.f * powf(2.f, 3.f * ent_value[LXR_FX_RING_LFO_DEPTH] * lfo);
	ring.osc.freq = fminf(hz, 8000.f);
	osc_setFreq(&ring.osc);
}

// One effect on a send bus: returns its wet signal added into `master`.
static void add_return(int b, float *master)
{
	const float level = ent_value[b == 0 ? LXR_FX_DELAY_RETURN : b == 1 ? LXR_FX_RING_RETURN : LXR_FX_VERB_RETURN];
	int16_t in[N * 2], wet[N * 2];
	int any = 0;
	for (int i = 0; i < N * 2; i++) {
		in[i] = (int16_t)sat16(send_bus[b][i]);
		any |= in[i];
	}
	// the delay and reverb keep ringing after their input stops; the ring modulator doesn't
	if (b == 1 && !any)
		return;
	memset(wet, 0, sizeof wet);
	switch (b) {
	case 0:
		if (delay.delayType == DELAY_NORMAL)
			Delay_process(&delay, in, wet, wet + 1, N, 2, 1.f);
		else
			Delay_processPingPong(&delay, in, wet, wet + 1, N, 2, 1.f);
		break;
	case 1: Ringmod_process(&ring, in, wet, wet + 1, N, 2, 1.f); break;
	default: Reverb_process(&reverb, in, wet, wet + 1, N, 2, 1.f); break;
	}
	for (int i = 0; i < N * 2; i++)
		master[i] += wet[i] * level;
}

static void filter(float *master)
{
	const float x = ent_value[LXR_FX_FILTER];
	const float amount = fabsf(x - 0.5f) * 2.f;
	if (amount < 0.01f) {
		svf_ic1[0] = svf_ic1[1] = svf_ic2[0] = svf_ic2[1] = 0.f;
		return;
	}
	const int highpass = x > 0.5f;
	// low pass from 20 kHz down to 50 Hz, high pass from 20 Hz up to 8 kHz
	const float cutoff = highpass ? 20.f * powf(400.f, amount) : 20000.f * powf(50.f / 20000.f, amount);
	const float target = tanf((float)M_PI * fminf(cutoff, 0.45f * LXR_ENGINE_RATE) / LXR_ENGINE_RATE);
	svf_g += (target - svf_g) * 0.3f;
	const float k = 2.f - 1.85f * ent_value[LXR_FX_FILTER_RESO];
	const float g = svf_g;
	const float a1 = 1.f / (1.f + g * (g + k)), a2 = g * a1, a3 = g * a2;
	for (int i = 0; i < N; i++) {
		for (int c = 0; c < 2; c++) {
			const float v0 = master[2 * i + c] * (1.f / 32768.f);
			const float v3 = v0 - svf_ic2[c];
			const float v1 = a1 * svf_ic1[c] + a2 * v3;
			const float v2 = svf_ic2[c] + a2 * svf_ic1[c] + a3 * v3;
			svf_ic1[c] = 2.f * v1 - svf_ic1[c];
			svf_ic2[c] = 2.f * v2 - svf_ic2[c];
			const float out = highpass ? v0 - k * v1 - v2 : v2;
			master[2 * i + c] = out * 32768.f;
		}
	}
}

// An insert on the master: the firmware effect's dry/wet does the mix.
typedef void (*Insert)(void *fx, int16_t *in, int16_t *outL, int16_t *outR, uint16_t size, uint8_t channels, float dryWet);

static void insert(Insert process, void *fx, float mix, float *master)
{
	if (mix <= 0.f)
		return;
	int16_t in[N * 2], out[N * 2];
	for (int i = 0; i < N * 2; i++)
		in[i] = (int16_t)sat16(master[i]);
	memset(out, 0, sizeof out);
	process(fx, in, out, out + 1, N, 2, mix);
	for (int i = 0; i < N * 2; i++)
		master[i] = out[i];
}

void lxr_fx_render(int16_t *dac1, int16_t *dac2)
{
	if (!ent_on) {
		mixer_calcNextSampleBlock(dac2, dac1);
		return;
	}
	memset(send_bus, 0, sizeof send_bus);
	// the firmware's FX bus passes its voices through dry
	const uint8_t classic = fxBus.fxType;
	fxBus.fxType = FX_OFF;
	mixer_calcNextSampleBlock(dac2, dac1);
	fxBus.fxType = classic;

	float master[N * 2];
	for (int i = 0; i < N * 2; i++)
		master[i] = (float)dac1[i] + (float)dac2[i];
	ring_lfo();
	for (int b = 0; b < BUSES; b++)
		add_return(b, master);
	filter(master);
	insert((Insert)Distortion_process, &drive, ent_value[LXR_FX_DRIVE_MIX], master);
	insert((Insert)Compressor_process, &comp, ent_value[LXR_FX_COMP_MIX], master);
	for (int i = 0; i < N * 2; i++) {
		dac1[i] = (int16_t)sat16(master[i]);
		dac2[i] = 0;
	}
}
