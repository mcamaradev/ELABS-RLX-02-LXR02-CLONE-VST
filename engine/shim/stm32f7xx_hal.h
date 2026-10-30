#pragma once
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#define __IO volatile
#define __STATIC_INLINE static inline
#define __disable_irq() ((void)0)
#define __enable_irq() ((void)0)
static inline int32_t __SSAT_f(int32_t x,int b){int32_t m=(1<<(b-1))-1;return x>m?m:(x<-m-1?-m-1:x);}
#define __SSAT(x,b) __SSAT_f((int32_t)(x),(b))
static inline int32_t __QADD(int32_t a,int32_t b){int64_t s=(int64_t)a+b;return s>INT32_MAX?INT32_MAX:(s<INT32_MIN?INT32_MIN:(int32_t)s);}
static inline int32_t __QSUB(int32_t a,int32_t b){int64_t s=(int64_t)a-b;return s>INT32_MAX?INT32_MAX:(s<INT32_MIN?INT32_MIN:(int32_t)s);}
static inline int16_t _s16(int32_t v){return v>32767?32767:(v<-32768?-32768:v);}
static inline uint32_t __QADD16(uint32_t a,uint32_t b){return (uint16_t)_s16((int16_t)a+(int16_t)b)|((uint32_t)(uint16_t)_s16((int16_t)(a>>16)+(int16_t)(b>>16))<<16);}
static inline uint32_t __QSUB16(uint32_t a,uint32_t b){return (uint16_t)_s16((int16_t)a-(int16_t)b)|((uint32_t)(uint16_t)_s16((int16_t)(a>>16)-(int16_t)(b>>16))<<16);}
static inline uint64_t __SMLALD(uint32_t a,uint32_t b,uint64_t acc){return acc+(int64_t)(int16_t)a*(int16_t)b+(int64_t)(int16_t)(a>>16)*(int16_t)(b>>16);}
#define __CLZ(x) ((x)?__builtin_clz(x):32)
#define I2S_AUDIOFREQ_44K 44100u
typedef struct { int dummy; } RNG_HandleTypeDef;
static inline uint32_t HAL_RNG_GetRandomNumber(RNG_HandleTypeDef*h){(void)h;return (uint32_t)arc4random();}
typedef struct { int dummy; } I2S_HandleTypeDef, UART_HandleTypeDef, TIM_HandleTypeDef, DMA_HandleTypeDef, SPI_HandleTypeDef, ADC_HandleTypeDef;
typedef struct { volatile uint32_t CTRL, CYCCNT; } DWT_Type; static DWT_Type _dwt; 
#define DWT (&_dwt)
static inline uint32_t HAL_RCC_GetHCLKFreq(void){return 216000000u;}
static inline uint32_t HAL_GetTick(void){return 0;}
#define __PKHBT(a,b,s) ((((uint32_t)(a))&0xFFFFu)|((((uint32_t)(b))<<(s))&0xFFFF0000u))
typedef union { struct { uint32_t _r:27; uint32_t Q:1; uint32_t V:1,C:1,Z:1,N:1; } b; uint32_t w; } APSR_Type;
static inline uint32_t __get_APSR(void){return 0;}
typedef struct { volatile uint32_t IDR, ODR, BSRR; } GPIO_TypeDef; static GPIO_TypeDef _gpio_dummy={0xffffffff,0,0};
#define GPIOA (&_gpio_dummy)
#define GPIOB (&_gpio_dummy)
#define GPIOC (&_gpio_dummy)
#define GPIOD (&_gpio_dummy)
#define GPIOE (&_gpio_dummy)
#define GPIO_PIN_0 0x1u
#define GPIO_PIN_1 0x2u
#define GPIO_PIN_2 0x4u
#define GPIO_PIN_3 0x8u
#define GPIO_PIN_4 0x10u
#define GPIO_PIN_5 0x20u
#define GPIO_PIN_6 0x40u
#define GPIO_PIN_7 0x80u
#define GPIO_PIN_8 0x100u
#define GPIO_PIN_9 0x200u
#define GPIO_PIN_10 0x400u
#define GPIO_PIN_11 0x800u
#define GPIO_PIN_12 0x1000u
#define GPIO_PIN_13 0x2000u
#define GPIO_PIN_14 0x4000u
#define GPIO_PIN_15 0x8000u
typedef enum { GPIO_PIN_RESET=0, GPIO_PIN_SET } GPIO_PinState;
static inline void HAL_GPIO_WritePin(GPIO_TypeDef*p,uint16_t pin,GPIO_PinState s){(void)p;(void)pin;(void)s;}
static inline GPIO_PinState HAL_GPIO_ReadPin(GPIO_TypeDef*p,uint16_t pin){(void)p;(void)pin;return GPIO_PIN_RESET;}
