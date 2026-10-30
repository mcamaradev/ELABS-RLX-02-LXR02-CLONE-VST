/*
 * EuklidGenerator.c
 *
 *  Created on: 04.08.2012
 *
 * Copyright (C) 2012-2026 Erica Synths
 * SPDX-License-Identifier: GPL-3.0-only
 *
 * This file is part of the LXR firmware. It is licensed under the GNU General
 * Public License version 3, with the additional permission and terms in
 * NOTICE.md. See LICENSE and NOTICE.md in the root of the repository.
 */

#pragma GCC push_options
#pragma GCC optimize ("O1")


#include "Soundengine/Sequencer/EuklidGenerator.h"
#include "Soundengine/Sequencer/sequencer.h"

static uint8_t euklid_length[NUM_TRACKS];
static uint8_t euklid_steps[NUM_TRACKS];
static uint8_t euklid_rotation[NUM_TRACKS];



#if NUM_BARS != 4
#pragma message "if bars are not 4 this will break the union approach!"
#endif

static ActiveSteps euklid_patternBuffer;	/**< 16 bits for maximum 16 steps. 1 is a note 0 is a pause*/

static uint16_t euklid_nextCnt1=0;
static uint16_t euklid_originalLength=0;
//-----------------------------------------------------
void euklid_init()
{
	int i;
	for(i=0;i<NUM_TRACKS;i++)
	{
		euklid_length[i] = 16;
		euklid_steps[i] = 4;
		euklid_rotation[i] = 0;
	}

		euklid_patternBuffer.quadword = 0;
}
//-----------------------------------------------------
/** calculate how many elements are in one group.
 *  the leading zeroes up to the first 1 are counted using the _CLZ(uint32) function starting at MSB
 *  since a group has always at least 1 element we don't return 0
 */
static inline uint8_t getObjSize(uint16_t group)
{
	uint8_t ret = 32 - __CLZ((uint32_t)group);
	if(ret==0) return 1;
	return ret;
}
//-----------------------------------------------------
void euklid_calcRecursive(uint8_t length, uint8_t steps, uint8_t iteration, uint16_t group1, uint16_t group2)
{
	int i,j;
	uint16_t remainder;

	//omit division by zero
	if(steps!=0)
		remainder 	= length%steps;
	else
		remainder = 0;

	//In the first iteration we have to set the next count value to the number of steps
	if(euklid_nextCnt1==0)euklid_nextCnt1=steps;

	//calculate the objects per group and their size
	int objectCnt1 = euklid_nextCnt1;
	int objSize1 = getObjSize(group1);
	int objSize2 = getObjSize(group2);
	int objCnt2 = (euklid_originalLength - (objectCnt1*objSize1))  / (objSize2);

	//the number of group 1 objects in the next iteration equals the count of objects in the smaller group
	euklid_nextCnt1 = objectCnt1>objCnt2?objCnt2:objectCnt1;

	//if the 2nd group has only 1 or less objects left
	//the algorithm has finished
	//--> write the generated pattern to euklid_patternBuffer
	if(objCnt2<=1 )
	{
		uint8_t bufferPos=0;
		//group1 objects
		for(i=0;i<objectCnt1;i++)
		{
			for(j=objSize1-1;j>=0;j--)
			{
				if(group1&(1<<j))
				{
					euklid_patternBuffer.quadword |= (((uint64_t)1)<<bufferPos);
				}
				bufferPos++;
			}
		}
		//group 2 objects
		for( i=0;i<objCnt2;i++)
		{
			for( j=objSize2-1;j>=0;j--)
			{
				if(group2&(1<<j))
				{
					euklid_patternBuffer.quadword |= (((uint64_t)1)<<bufferPos);
				}
				bufferPos++;
			}
		}

		return;
	}

	//shift the group 1 value to make space for the new group2 part
	//and save the original group content
	uint16_t originalGroup1 = group1;
	group1 = group1 << objSize2;


	//paste the group 2 values to the free space at the end of group 1
	for(i=0;i<objSize2;i++)
	{
		//if group2 contains a 1, set in in group1
		if(group2 & (1<<i) )
		{
			group1 |= (1<<i);
		}
	}

	//if there are group 2 objects left group 2 stays the same
	//if no group 2 objects are left the rest of the old group1 will be the new group 2
	group2 = objCnt2-objectCnt1>0?group2:originalGroup1;

	//start the next iteration
	euklid_calcRecursive(steps,remainder,iteration+1,group1,group2);

}
//-----------------------------------------------------
void euklid_generate(uint8_t trackNr, uint8_t patternNr)
{
	uint8_t length,steps,rotation;
	length = euklid_length[trackNr];
	steps = euklid_steps[trackNr];
	rotation = euklid_rotation[trackNr];

	//todo ist das hier n�tig? zero steps sollte doch gehen...
	if(steps==0)steps++;

	//reset the global values
	euklid_nextCnt1 = 0;
	euklid_originalLength = length;

	//clear the buffer
	euklid_patternBuffer.quadword = 0;


	//let's calculate the new pattern
	euklid_calcRecursive(length,steps,1,1,0); //always start with 1st iterations
	//rotate resulting pattern
	//euklid_rotatePattern(length, seq_getTrackRotation(trackNr));
	euklid_rotatePattern(length, rotation);
	//and store it in the active track
	euklid_transferPattern(trackNr, patternNr);
}
//-----------------------------------------------------
uint8_t euklid_getLength(uint8_t trackNr)
{
	return euklid_length[trackNr];
}
//-----------------------------------------------------
uint8_t euklid_getSteps(uint8_t trackNr)
{
	return euklid_steps[trackNr];
};

//-----------------------------------------------------
void euklid_setLength(uint8_t trackNr, uint8_t value)
{
	if(value<=0)value=1;
	//else if(value>16)value=16;
	euklid_length[trackNr] = value;
}
//-----------------------------------------------------
void euklid_setSteps(uint8_t trackNr, uint8_t value, uint8_t patternNr)
{
	if(value<=0)value=1;

	if(value>euklid_length[trackNr]) value= euklid_length[trackNr];

	euklid_steps[trackNr] = value;
	euklid_generate(trackNr, patternNr);
}
//-----------------------------------------------------
uint8_t euklid_getRotation(uint8_t trackNr)
{
	return euklid_rotation[trackNr];
}
//-----------------------------------------------------
void euklid_setRotation(uint8_t trackNr, uint8_t value, uint8_t patternNr)
{
	if(value>euklid_length[trackNr]) value = euklid_rotation[trackNr];

	euklid_rotation[trackNr] = value;
	euklid_generate(trackNr, patternNr);
}
//-----------------------------------------------------
void euklid_rotatePattern(uint8_t length, uint8_t amount)
{
	if( amount==0 || length==1 || amount%length==0  ) return; //Nothing to be done

	amount = amount%length;

	euklid_patternBuffer.quadword = (euklid_patternBuffer.quadword<<amount) | (euklid_patternBuffer.quadword>>(length-amount));

	euklid_patternBuffer.quadword &= ~(0xffffffffffffffff<<length); //Remove "leftovers"
}
//-----------------------------------------------------
void euklid_transferPattern(uint8_t trackNr, uint8_t patternNr)
{
	uint8_t len=euklid_length[trackNr];
	for(int i=0; i<NUM_BARS; i++)
	{
#if USE_PATTERN_EDIT_BUFFER
		patternBackupBuffer.seq_mainSteps[i][trackNr] = euklid_patternBuffer.word[i];
#else
		seq_patternSet.seq_mainSteps[i][patternNr][trackNr] = euklid_patternBuffer.word[i];
#endif

	}
// **PATROT - pattern end is now stored differently

	if(len == 16)
		len=0;
#if USE_PATTERN_EDIT_BUFFER
	patternBackupBuffer.length[trackNr]=len;
#else
	seq_patternSet.length[patternNr][trackNr]=len;
#endif

}

#pragma GCC pop_options