/*
 * clockSync.c
 *
 *  Created on: 03.02.2013
 *
 * Copyright (C) 2012-2026 Erica Synths
 * SPDX-License-Identifier: GPL-3.0-only
 *
 * This file is part of the LXR firmware. It is licensed under the GNU General
 * Public License version 3, with the additional permission and terms in
 * NOTICE.md. See LICENSE and NOTICE.md in the root of the repository.
 */



#include "Soundengine/Sequencer/clockSync.h"

static float sync_window[WINDOW_SIZE];
float sync_timeLast = 0;
uint8_t sync_writePos = 0;
uint8_t sync_armed = 0;

uint8_t sync_clockCnt = 0;

uint32_t sync_delayedStepTimer = 0;


//---------------------------------------------------------
uint16_t sync_calcBpm(float timePerPulse)
{
	// 1 Minute has 60 * 1000 [ms]
	//we have 24 pulses per quarter
	//120 bpm = 120 quarters per minute

	float quarterDuration = 24 * (timePerPulse/4);
	float bpm = (60 * 1000) / quarterDuration;

	return bpm>0?bpm:1;
}
//---------------------------------------------------------
//called by midi clock
void sync_tick()
{

#if 1
	//with the new sequencer without substeps we have classical 4ppq resolution
	//MIDI clock is 24ppq
	//so we can simply advance the sequencer every 6 clock pulses
	if(sync_clockCnt == 0)
	{
		uint8_t isStepShuffled = seq_getMasterStepCnt()%2 == 1;

		if(isStepShuffled)
		{
			float dt = seq_getDeltaTShuffleDelay(seq_getBpm());
			sync_delayedStepTimer = systick_ticks + dt;
		}
		else
		{
			seq_nextStep();
			seq_prescaleCounter=0;
			seq_MidiSyncQueue(0);
		}
	}

	sync_clockCnt++;
	if(sync_clockCnt >= 6)
	{
		sync_clockCnt = 0;


	}

	//Fix for external sync LFO sync calculation

	float time = systick_ticks - sync_timeLast;
	sync_timeLast = systick_ticks;

	sync_window[sync_writePos++] = time;
	sync_writePos &= WINDOW_MASK;

	//calc average time in [ms] between midi clock pulses
	int i;
	float avg=0;
	for(i=0;i<WINDOW_SIZE;i++)
	{
		avg += sync_window[i];
	}
	avg /= WINDOW_SIZE;
	seq_setBpm(sync_calcBpm(avg));




#else
	float time = systick_ticks - sync_timeLast;
	sync_timeLast = systick_ticks;

	sync_window[sync_writePos++] = time;
	sync_writePos &= WINDOW_MASK;

	//calc average time in [ms] between midi clock pulses
	int i;
	float avg=0;
	for(i=0;i<WINDOW_SIZE;i++)
	{
		avg += sync_window[i];
	}
	avg /= WINDOW_SIZE;

	seq_setBpm(sync_calcBpm(avg));

	sync_clockCnt++;

	if(seq_isRunning())
	{
		//now handle sequencer transport (4 steps every 3 clocks)
		if(sync_clockCnt == 1 || sync_clockCnt >= 4) {
			sync_clockCnt = 1;
			seq_resetDeltaAndTick();
		}
	}
    
#endif


}
//---------------------------------------------------------
void sync_midiResetPosition()
{
	sync_clockCnt = 0;
	seq_resetPosition();
}
//---------------------------------------------------------
void sync_midiStartStop(uint8_t isStart)
{
	if(isStart) {
		// this message does NOT mean start immediately! it just 'arms' the seq to start at the next MIDI_CLOCK received

		seq_setRunning(1);
		sync_timeLast = systick_ticks;
	} else {
		seq_setRunning(0);
	}
}
//---------------------------------------------------------
uint8_t sync_getClockCnt()
{
	return sync_clockCnt;
}
//---------------------------------------------------------
