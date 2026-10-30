// Renders each LXR voice once with the init kit into a WAV file and prints per-voice peaks.
#include <stdio.h>
#include <math.h>
#include <stdint.h>
#include "../lxr_bridge.h"

int main(int argc, char **argv)
{
	const char *path = argc > 1 ? argv[1] : "lxr_test.wav";
	const int blocks_per_hit = (int)(LXR_ENGINE_RATE * 0.6f) / LXR_BLOCK_FRAMES;
	const int nblocks = blocks_per_hit * 7;
	FILE *f = fopen(path, "wb");
	uint32_t datasz = nblocks * LXR_BLOCK_FRAMES * 4, rate = 44140, byterate = rate * 4, v32; uint16_t v16;
	fwrite("RIFF", 1, 4, f); v32 = 36 + datasz; fwrite(&v32, 4, 1, f); fwrite("WAVEfmt ", 1, 8, f);
	v32 = 16; fwrite(&v32, 4, 1, f); v16 = 1; fwrite(&v16, 2, 1, f); v16 = 2; fwrite(&v16, 2, 1, f);
	fwrite(&rate, 4, 1, f); fwrite(&byterate, 4, 1, f); v16 = 4; fwrite(&v16, 2, 1, f); v16 = 16; fwrite(&v16, 2, 1, f);
	fwrite("data", 1, 4, f); fwrite(&datasz, 4, 1, f);

	lxr_init();
	float l[64], r[64], l2[64], r2[64], peak = 0, peak2 = 0;
	for (int b = 0; b < nblocks; b++) {
		if (b % blocks_per_hit == 0) {
			if (b) printf("voice %d peak %.3f (out2 %.3f)\n", b / blocks_per_hit - 1, peak, peak2);
			peak = peak2 = 0;
			lxr_note_on(b / blocks_per_hit, 60, 100);
		}
		lxr_render(l, r, l2, r2);
		for (int i = 0; i < 64; i++) {
			peak = fmaxf(peak, fmaxf(fabsf(l[i]), fabsf(r[i])));
			peak2 = fmaxf(peak2, fmaxf(fabsf(l2[i]), fabsf(r2[i])));
			int16_t s[2] = {(int16_t)(l[i] * 32767), (int16_t)(r[i] * 32767)};
			fwrite(s, 2, 2, f);
		}
	}
	printf("voice 6 peak %.3f (out2 %.3f)\n", peak, peak2);
	fclose(f);
	return 0;
}
