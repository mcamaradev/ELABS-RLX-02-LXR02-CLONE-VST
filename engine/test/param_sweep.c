// Sweeps every plugin parameter through its whole range while triggering all voices.
// Built with AddressSanitizer by param_sweep.sh to catch out-of-bounds accesses in the firmware.
#include <stdio.h>
#include <math.h>
#include "../lxr_bridge.h"

static const struct { int idx, min, max; } params[] = {
#include "param_ranges.inc"
};

// Loads every kit of a kits/lxr_kits.bin file with its raw values and plays all voices.
static int kit_test(const char *path)
{
	FILE *f = fopen(path, "rb");
	if (!f) { printf("cannot open %s\n", path); return 1; }
	unsigned char kit[LXR_NUM_PARAMS];
	float l[64], r[64], l2[64], r2[64];
	int n = 0, silent = 0, nonfinite = 0;
	while (fread(kit, 1, sizeof kit, f) == sizeof kit) {
		for (int i = 0; i < LXR_NUM_PARAMS; i++)
			lxr_set_param(i, kit[i]);
		double peak = 0;
		for (int v = 0; v < 7; v++) {
			lxr_note_on(v, 60, 110);
			for (int b = 0; b < 200; b++) {
				lxr_render(l, r, l2, r2);
				for (int i = 0; i < 64; i++) {
					float s[4] = {l[i], r[i], l2[i], r2[i]};
					for (int c = 0; c < 4; c++) {
						if (!isfinite(s[c])) nonfinite++;
						if (fabsf(s[c]) > peak) peak = fabsf(s[c]);
					}
				}
			}
		}
		if (peak < 0.01) silent++;
		n++;
	}
	fclose(f);
	printf("loaded %d kits, %d silent, non-finite samples %d\n", n, silent, nonfinite);
	return nonfinite != 0;
}

// FX delay switched from normal to ping-pong while running (the crash seen in Live).
static void delay_mode_switch_test(void)
{
	float l[64], r[64], l2[64], r2[64];
	lxr_set_param(234, 4); // PAR_FX_TYPE: delay
	lxr_set_param(236, 50); // dry/wet
	// switch at different write positions; it only overflows while normal mode is writing in the
	// upper half of the buffer
	for (int round = 0; round < 6; round++) {
		lxr_set_param(245, 0); // PAR_FX_DELAY_TYPE: normal
		for (int b = 0; b < 150 + round * 90; b++) {
			if (b % 50 == 0) lxr_note_on(3, 60, 120);
			lxr_render(l, r, l2, r2);
		}
		lxr_set_param(245, 1); // ping-pong
		for (int b = 0; b < 600; b++) {
			if (b % 50 == 0) lxr_note_on(3, 60, 120);
			lxr_render(l, r, l2, r2);
		}
	}
	printf("delay normal/ping-pong switching ok\n");
}

// Plays every pattern of a kits/lxr_projects.bin (64 patterns + 64 kits per project) for 4 bars
// with the kit it names.
static int pattern_test(const char *path)
{
	enum { KIT = 247, PROJECT = 64 * (LXR_PATTERN_BYTES + KIT) };
	FILE *f = fopen(path, "rb");
	if (!f) { printf("cannot open %s\n", path); return 1; }
	static unsigned char project[PROJECT];
	float l[64], r[64], l2[64], r2[64];
	int projects = 0, played = 0, nonfinite = 0;
	while (fread(project, 1, PROJECT, f) == PROJECT) {
		for (int nr = 0; nr < 64; nr++)
			lxr_seq_read_pattern(nr, project + nr * LXR_PATTERN_BYTES, LXR_PATTERN_BYTES);
		for (int nr = 0; nr < 64; nr++) {
			const unsigned char *pat = project + nr * LXR_PATTERN_BYTES;
			const unsigned char *kit = project + 64 * LXR_PATTERN_BYTES + pat[LXR_PATTERN_BYTES - 1] * KIT;
			for (int i = 0; i < LXR_NUM_PARAMS; i++)
				lxr_set_param(i, kit[i]);
			lxr_seq_stop();
			lxr_seq_set_pattern(nr);
			lxr_seq_start();
			// 4 bars at 120 BPM: 64 steps of 24 ticks, 44140/48 samples per tick
			double due = 0;
			for (int tick = 0; tick < 64 * 24; tick++) {
				lxr_seq_tick();
				for (due += 44140.0 / 192.0; due >= 64; due -= 64) {
					lxr_render(l, r, l2, r2);
					for (int i = 0; i < 64; i++)
						if (!isfinite(l[i]) || !isfinite(r[i]) || !isfinite(l2[i]) || !isfinite(r2[i]))
							nonfinite++;
				}
			}
			played++;
		}
		projects++;
	}
	fclose(f);
	lxr_seq_stop();
	printf("played %d patterns of %d projects, non-finite samples %d\n", played, projects, nonfinite);
	return nonfinite != 0;
}

// ENTROPIA FX: every setting from 0 to 1 (choices through their range) with all sends open,
// voices playing, then back to CLASSIC
static int entropia_test(void)
{
	float l[64], r[64], l2[64], r2[64];
	int nonfinite = 0;
	lxr_fx_set_mode(1);
	for (int i = 0; i < 18; i++)
		lxr_fx_set(LXR_FX_SEND + i, 1.f);
	lxr_fx_set(LXR_FX_DRIVE_MIX, 1.f);
	lxr_fx_set(LXR_FX_COMP_MIX, 1.f);
	for (int id = LXR_FX_DELAY_TYPE; id < LXR_FX_COUNT; id++) {
		const int choice = id == LXR_FX_DELAY_TYPE || id == LXR_FX_RING_WAVE || id == LXR_FX_DRIVE_TYPE || id == LXR_FX_COMP_RATIO || id == LXR_FX_RING_LFO_WAVE || id == LXR_FX_RING_LFO_SYNC;
		const int steps = id == LXR_FX_RING_LFO_SYNC ? LXR_FX_LFO_SYNCS : choice ? 6 : 8;
		// the synced LFO with the host playing (and one setting past the last)
		if (id == LXR_FX_RING_LFO_SYNC)
			lxr_fx_clock(3.7, 174.f);
		if (id == LXR_FX_RING_LFO_DEPTH)
			lxr_fx_set(LXR_FX_RING_LFO_RATE, 1.f);
		for (int k = 0; k <= steps; k++) {
			lxr_fx_set(id, choice ? (float)k : k / (float)steps);
			lxr_note_on(k % 7, 60, 127);
			for (int b = 0; b < 8; b++) {
				lxr_render(l, r, l2, r2);
				for (int i = 0; i < 64; i++)
					if (!isfinite(l[i]) || !isfinite(r[i]) || l2[i] != 0.f || r2[i] != 0.f) nonfinite++;
			}
		}
	}
	lxr_fx_set_mode(0);
	printf("entropia: swept %d settings, bad samples %d\n", LXR_FX_COUNT - LXR_FX_DELAY_TYPE, nonfinite);
	return nonfinite;
}

int main(int argc, char **argv)
{
	float l[64], r[64], l2[64], r2[64];
	double peak = 0;
	int nonfinite = 0;
	lxr_init();
	delay_mode_switch_test();
	if (entropia_test())
		return 1;
	if (argc > 2)
		return pattern_test(argv[2]);
	if (argc > 1)
		return kit_test(argv[1]);
	for (size_t p = 0; p < sizeof params / sizeof params[0]; p++) {
		for (int v = params[p].min; v <= params[p].max; v++) {
			lxr_set_param(params[p].idx, v);
			if (v % 8 == 0 || v == params[p].max) {
				lxr_note_on((v / 8) % 7, 60, 100);
				for (int b = 0; b < 4; b++) {
					lxr_render(l, r, l2, r2);
					for (int i = 0; i < 64; i++) {
						float s[4] = {l[i], r[i], l2[i], r2[i]};
						for (int c = 0; c < 4; c++) {
							if (!isfinite(s[c])) nonfinite++;
							if (fabsf(s[c]) > peak) peak = fabsf(s[c]);
						}
					}
				}
			}
		}
		lxr_set_param(params[p].idx, lxr_init_kit_value(params[p].idx));
	}
	printf("swept %zu params, peak %.3f, non-finite samples %d\n", sizeof params / sizeof params[0], peak, nonfinite);
	return nonfinite != 0;
}
