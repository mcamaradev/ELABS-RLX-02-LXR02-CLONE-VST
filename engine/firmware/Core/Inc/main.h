/* USER CODE BEGIN Header */
/**
  ******************************************************************************
  * @file           : main.h
  * @brief          : Header for main.c file.
  *                   This file contains the common defines of the application.
  ******************************************************************************
  * @attention
  *
  * <h2><center>&copy; Copyright (c) 2020 STMicroelectronics.
  * All rights reserved.</center></h2>
  *
  * This software component is licensed by ST under Ultimate Liberty license
  * SLA0044, the "License"; You may not use this file except in compliance with
  * the License. You may obtain a copy of the License at:
  *                             www.st.com/SLA0044
  *
  ******************************************************************************
  */
/* USER CODE END Header */

/* Define to prevent recursive inclusion -------------------------------------*/
#ifndef __MAIN_H
#define __MAIN_H

#ifdef __cplusplus
extern "C" {
#endif

/* Includes ------------------------------------------------------------------*/
#include "stm32f7xx_hal.h"

/* Private includes ----------------------------------------------------------*/
/* USER CODE BEGIN Includes */
#include "delay.h"
extern float debugWatch;
/* USER CODE END Includes */

/* Exported types ------------------------------------------------------------*/
/* USER CODE BEGIN ET */
extern I2S_HandleTypeDef hi2s2;
extern I2S_HandleTypeDef hi2s3;
extern UART_HandleTypeDef huart3;
extern TIM_HandleTypeDef htim2;


/* USER CODE END ET */

/* Exported constants --------------------------------------------------------*/
/* USER CODE BEGIN EC */
/*
#define MAPLE_RELOAD_VAL     72000
#define CYCLES_PER_MICROSECOND  72
#define FUDGE                   42
*/
#define USE_SD_CARD 1
/* USER CODE END EC */

/* Exported macro ------------------------------------------------------------*/
/* USER CODE BEGIN EM */

/* USER CODE END EM */

/* Exported functions prototypes ---------------------------------------------*/
void Error_Handler(void);

/* USER CODE BEGIN EFP */

/* USER CODE END EFP */

/* Private defines -----------------------------------------------------------*/
#define Clock_Out_Pin GPIO_PIN_13
#define Clock_Out_GPIO_Port GPIOC
#define DIO_LD_Pin GPIO_PIN_2
#define DIO_LD_GPIO_Port GPIOB
#define LCD_D4_Pin GPIO_PIN_7
#define LCD_D4_GPIO_Port GPIOE
#define LCD_D5_Pin GPIO_PIN_8
#define LCD_D5_GPIO_Port GPIOE
#define LCD_D6_Pin GPIO_PIN_9
#define LCD_D6_GPIO_Port GPIOE
#define LCD_D7_Pin GPIO_PIN_10
#define LCD_D7_GPIO_Port GPIOE
#define LCD_E_Pin GPIO_PIN_11
#define LCD_E_GPIO_Port GPIOE
#define LCD_RS_Pin GPIO_PIN_12
#define LCD_RS_GPIO_Port GPIOE
#define Enc_A_Pin GPIO_PIN_13
#define Enc_A_GPIO_Port GPIOE
#define Enc_B_Pin GPIO_PIN_14
#define Enc_B_GPIO_Port GPIOE
#define Enc_Switch_Pin GPIO_PIN_15
#define Enc_Switch_GPIO_Port GPIOE
#define SDIO_CS_Pin GPIO_PIN_0
#define SDIO_CS_GPIO_Port GPIOD
#define SD_CD_Pin GPIO_PIN_1
#define SD_CD_GPIO_Port GPIOD
#define Clock_In_EXTI4_Pin GPIO_PIN_4
#define Clock_In_EXTI4_GPIO_Port GPIOD
#define Clock_In_EXTI4_EXTI_IRQn EXTI4_IRQn
#define Reset_In_EXTI5_Pin GPIO_PIN_5
#define Reset_In_EXTI5_GPIO_Port GPIOD
#define Reset_In_EXTI5_EXTI_IRQn EXTI9_5_IRQn
#define switch_l1_Pin GPIO_PIN_6
#define switch_l1_GPIO_Port GPIOD
#define switch_r1_Pin GPIO_PIN_7
#define switch_r1_GPIO_Port GPIOD
#define switch_l2_Pin GPIO_PIN_4
#define switch_l2_GPIO_Port GPIOB
#define switch_r2_Pin GPIO_PIN_6
#define switch_r2_GPIO_Port GPIOB
#define SW_43_Pin GPIO_PIN_7
#define SW_43_GPIO_Port GPIOB
#define LED_43_Pin GPIO_PIN_8
#define LED_43_GPIO_Port GPIOB
/* USER CODE BEGIN Private defines */


// Helper to declare variable in CCM memory, pre-loaded from flash at startup
#define INDTCM __attribute__ ((section(".dtcm")))

// Helper to declare variable in CCM memory that is zeroed at startup
#define INDTCMZ __attribute__ ((section(".dtcmz")))
/* USER CODE END Private defines */

#ifdef __cplusplus
}
#endif

#endif /* __MAIN_H */

/************************ (C) COPYRIGHT STMicroelectronics *****END OF FILE****/
