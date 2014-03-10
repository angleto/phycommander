/**
 * \file
 *
 * \brief Arduino Due/X Board Definition.
 *
 * Copyright (c) 2011 - 2013 Atmel Corporation. All rights reserved.
 *
 * \asf_license_start
 *
 * \page License
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions are met:
 *
 * 1. Redistributions of source code must retain the above copyright notice,
 *    this list of conditions and the following disclaimer.
 *
 * 2. Redistributions in binary form must reproduce the above copyright notice,
 *    this list of conditions and the following disclaimer in the documentation
 *    and/or other materials provided with the distribution.
 *
 * 3. The name of Atmel may not be used to endorse or promote products derived
 *    from this software without specific prior written permission.
 *
 * 4. This software may only be redistributed and used in connection with an
 *    Atmel microcontroller product.
 *
 * THIS SOFTWARE IS PROVIDED BY ATMEL "AS IS" AND ANY EXPRESS OR IMPLIED
 * WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF
 * MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NON-INFRINGEMENT ARE
 * EXPRESSLY AND SPECIFICALLY DISCLAIMED. IN NO EVENT SHALL ATMEL BE LIABLE FOR
 * ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
 * OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
 * HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT,
 * STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN
 * ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 *
 * \asf_license_stop
 *
 */

#ifndef ARDUINO_DUE_X_H_INCLUDED
#define ARDUINO_DUE_X_H_INCLUDED

#include "compiler.h"
#include "system_sam3x.h"
#include "exceptions.h"

/* ------------------------------------------------------------------------ */

/**
 *  \page arduino_due_x_opfreq "Arduino Due/X - Operating frequencies"
 *  This page lists several definition related to the board operating frequency
 *
 *  \section Definitions
 *  - \ref BOARD_FREQ_*
 *  - \ref BOARD_MCK
 */

/*! Board oscillator settings */
#define BOARD_FREQ_SLCK_XTAL            (32768U)
#define BOARD_FREQ_SLCK_BYPASS          (32768U)
#define BOARD_FREQ_MAINCK_XTAL          (12000000U)
#define BOARD_FREQ_MAINCK_BYPASS        (12000000U)

/*! Master clock frequency */
#define BOARD_MCK                       CHIP_FREQ_CPU_MAX
#define BOARD_NO_32K_XTAL

/** board main clock xtal statup time */
#define BOARD_OSC_STARTUP_US   15625

/* ------------------------------------------------------------------------ */

/**
 * \page arduino_due_x_board_info "Arduino Due/X - Board informations"
 * This page lists several definition related to the board description.
 *
 * \section Definitions
 * - \ref BOARD_NAME
 */

/*! Name of the board */
#define BOARD_NAME "Arduino Due/X"
/*! Board definition */
#define arduinoduex
/*! Family definition (already defined) */
#define sam3x
/*! Core definition */
#define cortexm3

/* ------------------------------------------------------------------------ */

/**
 * \page arduino_due_x_piodef "Arduino Due/X - PIO definitions"
 * This pages lists all the pio definitions. The constants
 * are named using the following convention: PIN_* for a constant which defines
 * a single Pin instance (but may include several PIOs sharing the same
 * controller), and PINS_* for a list of Pin instances.
 *
 */

/**
 * \file
 * ADC
 * - \ref PIN_ADC0_AD1
 * - \ref PINS_ADC
 *
 */

/**
 * \note ADC pins are automatically configured by the ADC peripheral as soon as
 * the corresponding channel is enabled.
 *
 * \note On Arduino Due/X, Channel 1 is labelled A6 on the PCB.
 */

/*! ADC_AD1 pin definition. */
#define PIN_ADC0_AD1 {PIO_PA3X1_AD1, PIOA, ID_PIOA, PIO_INPUT, PIO_DEFAULT}
#define PINS_ADC_TRIG  PIO_PA11_IDX
#define PINS_ADC_TRIG_FLAG  (PIO_PERIPH_B | PIO_DEFAULT)
/*! Pins ADC */
#define PINS_ADC PIN_ADC0_AD1

/**
 * \file
 * DAC
 *
 */

/**
 * \note DAC pins are automatically configured by the DAC peripheral as soon
 * as the corresponding channel is enabled.
 *
 * \note On Arduino Due/X, channel 0 is labelled A12 and channel 1 is labelled
 * A13 on the PCB.
 */


/**
 * \file
 * LEDs
 *
 */

/* ------------------------------------------------------------------------ */
/* LEDS                                                                     */
/* ------------------------------------------------------------------------ */
/*! Power LED pin definition (ORANGE). L */
#define PIN_POWER_LED   {PIO_PB27, PIOB, ID_PIOB, PIO_OUTPUT_1, PIO_DEFAULT}
/*! LED #1 pin definition */
#define PIN_USER_LED1   {PIO_PC21, PIOC, ID_PIOC, PIO_OUTPUT_1, PIO_DEFAULT}
/*! LED #2 pin definition */
#define PIN_USER_LED2   {PIO_PC22, PIOC, ID_PIOC, PIO_OUTPUT_1, PIO_DEFAULT}
/*! LED #3 pin definition */
#define PIN_USER_LED3   {PIO_PC23, PIOC, ID_PIOC, PIO_OUTPUT_1, PIO_DEFAULT}

/*! List of all LEDs definitions. */
#define PINS_LEDS   PIN_USER_LED1, PIN_USER_LED2, PIN_USER_LED3, PIN_POWER_LED

/*! LED #0 "L" pin definition (ORANGE).*/
#define LED_0_NAME      "Orange_LED"
#define LED0_GPIO       (PIO_PB27_IDX)
#define LED0_FLAGS      (PIO_TYPE_PIO_OUTPUT_1 | PIO_DEFAULT)
#define LED0_ACTIVE_LEVEL 0

#define PIN_LED_0       {1 << 27, PIOB, ID_PIOB, PIO_OUTPUT_0, PIO_DEFAULT}
#define PIN_LED_0_MASK  (1 << 27)
#define PIN_LED_0_PIO   PIOB
#define PIN_LED_0_ID    ID_PIOB
#define PIN_LED_0_TYPE  PIO_OUTPUT_0
#define PIN_LED_0_ATTR  PIO_DEFAULT

/*! LED #1 pin definition */
#define LED_1_NAME      "External_LED_on_PWM9_connector_output"
#define LED1_GPIO       (PIO_PC21_IDX)
#define LED1_FLAGS      (PIO_TYPE_PIO_OUTPUT_1 | PIO_DEFAULT)
#define LED1_ACTIVE_LEVEL 0

#define PIN_LED_1       {1 << 21, PIOC, ID_PIOC, PIO_OUTPUT_1, PIO_DEFAULT}
#define PIN_LED_1_MASK  (1 << 21)
#define PIN_LED_1_PIO   PIOC
#define PIN_LED_1_ID    ID_PIOC
#define PIN_LED_1_TYPE  PIO_OUTPUT_1
#define PIN_LED_1_ATTR  PIO_DEFAULT

/*! LED #2 pin detection */
#define LED2_GPIO       (PIO_PC22_IDX)
#define LED2_FLAGS      (PIO_TYPE_PIO_OUTPUT_1 | PIO_DEFAULT)
#define LED2_ACTIVE_LEVEL 0

#define PIN_LED_2       {1 << 22, PIOC, ID_PIOC, PIO_OUTPUT_1, PIO_DEFAULT}
#define PIN_LED_2_MASK  (1 << 22)
#define PIN_LED_2_PIO   PIOC
#define PIN_LED_2_ID    ID_PIOC
#define PIN_LED_2_TYPE  PIO_OUTPUT_1
#define PIN_LED_2_ATTR  PIO_DEFAULT

/*! LED #3 pin detection */
#define LED3_GPIO       (PIO_PC23_IDX)
#define LED3_FLAGS      (PIO_TYPE_PIO_OUTPUT_1 | PIO_DEFAULT)
#define LED3_ACTIVE_LEVEL 1

#define BOARD_NUM_OF_LED 4
#define PIN_LED_3       {1 << 23, PIOC, ID_PIOC, PIO_OUTPUT_1, PIO_DEFAULT}
#define PIN_LED_3_MASK  (1 << 23)
#define PIN_LED_3_PIO   PIOC
#define PIN_LED_3_ID    ID_PIOC
#define PIN_LED_3_TYPE  PIO_OUTPUT_1
#define PIN_LED_3_ATTR  PIO_DEFAULT

/**
 * \file
 * Push buttons
 * - \ref PIN_PB_LEFT_CLICK
 * - \ref PIN_PB_RIGHT_CLICK
 * - \ref PINS_PUSHBUTTONS
 * - \ref PUSHBUTTON_BP1
 * - \ref PUSHBUTTON_BP2
 *
 */

/* ------------------------------------------------------------------------ */
/* PUSHBUTTONS                                                              */
/* ------------------------------------------------------------------------ */

/**************************changing**********************************/

/** Push button LEFT CLICK definition.
 *  Attributes = pull-up + debounce + interrupt on falling edge. */
#define PIN_PB_LEFT_CLICK    {PIO_PD8, PIOD, ID_PIOD, PIO_INPUT,\
	PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_FALL_EDGE}

/** Push button RIGHT CLICK definition.
 *  Attributes = pull-up + debounce + interrupt on falling edge. */
#define PIN_PB_RIGHT_CLICK    {PIO_PC28, PIOC, ID_PIOC, PIO_INPUT,\
	PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_FALL_EDGE}

/*! List of all push button definitions. */
#define PINS_PUSHBUTTONS    PIN_PB_LEFT_CLICK, PIN_PB_RIGHT_CLICK

/*! Push button #1 index. */
#define PUSHBUTTON_BP1   0
/*! Push button #2 index. */
#define PUSHBUTTON_BP2   1

/*! Push button LEFT CLICK index. */
#define PUSHBUTTON_LEFT     0
/*! Push button RIGHT CLICK index. */
#define PUSHBUTTON_RIGHT    1

/** Push button #0 definition.
 *  Attributes = pull-up + debounce + interrupt on rising edge. */
#define PUSHBUTTON_1_NAME    "External_PB1_on_PWM12_connector_output"

#define GPIO_PUSH_BUTTON_1           (PIO_PD8_IDX)
#define GPIO_PUSH_BUTTON_1_FLAGS\
	(PIO_INPUT | PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_RISE_EDGE)

#define PIN_PUSHBUTTON_1    {PIO_PD8, PIOD, ID_PIOD, PIO_INPUT,\
	PIO_PULLUP }
#define PIN_PUSHBUTTON_1_MASK PIO_PD8
#define PIN_PUSHBUTTON_1_PIO PIOD
#define PIN_PUSHBUTTON_1_ID ID_PIOD
#define PIN_PUSHBUTTON_1_TYPE PIO_INPUT
#define PIN_PUSHBUTTON_1_ATTR (PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_RISE_EDGE)

/** Push button #1 definition.
 *  Attributes = pull-up + debounce + interrupt on falling edge. */
#define PUSHBUTTON_2_NAME    "External_PB2_on_PWM3_connector_output"
#define GPIO_PUSH_BUTTON_2           (PIO_PC28_IDX)
#define GPIO_PUSH_BUTTON_2_FLAGS\
	(PIO_INPUT | PIO_PULLUP)

#define PIN_PUSHBUTTON_2    {PIO_PC28, PIOC, ID_PIOC, PIO_INPUT,\
	PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_FALL_EDGE}
#define PIN_PUSHBUTTON_2_MASK PIO_PC28
#define PIN_PUSHBUTTON_2_PIO PIOC
#define PIN_PUSHBUTTON_2_ID ID_PIOC
#define PIN_PUSHBUTTON_2_TYPE PIO_INPUT
#define PIN_PUSHBUTTON_2_ATTR (PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_FALL_EDGE)


#define PIN_TC0_TIOA1           (PIO_PA2_IDX)
#define PIN_TC0_TIOA1_MUX       (IOPORT_MODE_MUX_A)
#define PIN_TC0_TIOA1_FLAGS     (PIO_PERIPH_A | PIO_DEFAULT)

#define PIN_TC0_TIOA1_PIO     PIOA
#define PIN_TC0_TIOA1_MASK    PIO_PA2
#define PIN_TC0_TIOA1_ID      ID_PIOA
#define PIN_TC0_TIOA1_TYPE    PIO_PERIPH_A
#define PIN_TC0_TIOA1_ATTR    PIO_DEFAULT


#define PIN_TC0_TIOA0         (PIO_PB25_IDX)
#define PIN_TC0_TIOA0_MUX     (IOPORT_MODE_MUX_B)
#define PIN_TC0_TIOA0_FLAGS   (PIO_INPUT | PIO_DEFAULT)

#define PIN_TC0_TIOA0_PIO     PIOB
#define PIN_TC0_TIOA0_MASK    PIO_PB25
#define PIN_TC0_TIOA0_ID      ID_PIOB
#define PIN_TC0_TIOA0_TYPE    PIO_INPUT
#define PIN_TC0_TIOA0_ATTR    PIO_DEFAULT

/**
 * \file
 * PWMC
 * - \ref PIN_PWMC_PWMH0
 * - \ref PIN_PWMC_PWML4
 * - \ref PIN_PWMC_PWML5
 * - \ref PIN_PWMC_PWML6
 * - \ref PIN_PWM_LED0
 * - \ref PIN_PWM_LED1
 * - \ref PIN_PWM_LED2
 *
 */

/* ------------------------------------------------------------------------ */
/* PWM                                                                      */
/* ------------------------------------------------------------------------ */
/*! PWMC PWM0 TRIG pin definition: Output High. */
#define PIN_PWMC_PWMH0_TRIG   PIO_PB12_IDX
#define PIN_PWMC_PWMH0_TRIG_FLAG   PIO_PERIPH_B | PIO_DEFAULT

/*! PWMC PWM4 pin definition: Output Low. */
#define PIN_PWMC_PWML4\
        {PIO_PC21B_PWML4, PIOC, ID_PIOC, PIO_PERIPH_B, PIO_DEFAULT}
/*! PWMC PWM5 pin definition: Output Low. */
#define PIN_PWMC_PWML5\
	{PIO_PC22B_PWML5, PIOC, ID_PIOC, PIO_PERIPH_B, PIO_DEFAULT}
/*! PWMC PWM6 pin definition: Output High. */
#define PIN_PWMC_PWML6\
	{PIO_PC23B_PWML6, PIOC, ID_PIOC, PIO_PERIPH_B, PIO_DEFAULT}

/*! PWM pins definition for LED0 */
#define PIN_PWM_LED0 PIN_PWMC_PWML4
/*! PWM pins definition for LED1 */
#define PIN_PWM_LED1 PIN_PWMC_PWML5
/*! PWM pins definition for LED2 */
#define PIN_PWM_LED2 PIN_PWMC_PWML6


/*! PWM channel for LED0 */
#define CHANNEL_PWM_LED0 PWM_CHANNEL_4
/*! PWM channel for LED1 */
#define CHANNEL_PWM_LED1 PWM_CHANNEL_5
/*! PWM channel for LED2 */
#define CHANNEL_PWM_LED2 PWM_CHANNEL_6

/*! PWM "PWM7" LED0 pin definitions.*/
#define PIN_PWM_LED0_GPIO    PIO_PC21_IDX
#define PIN_PWM_LED0_FLAGS   (PIO_PERIPH_B | PIO_DEFAULT)
#define PIN_PWM_LED0_CHANNEL PWM_CHANNEL_4

/*! PWM "PWM8" LED1 pin definitions.*/
#define PIN_PWM_LED1_GPIO    PIO_PC22_IDX
#define PIN_PWM_LED1_FLAGS   (PIO_PERIPH_B | PIO_DEFAULT)
#define PIN_PWM_LED1_CHANNEL PWM_CHANNEL_5

/*! PWM "PWM9" LED2 pin definitions.*/
#define PIN_PWM_LED2_GPIO    PIO_PC23_IDX
#define PIN_PWM_LED2_FLAGS   (PIO_PERIPH_B | PIO_DEFAULT)
#define PIN_PWM_LED2_CHANNEL PWM_CHANNEL_6


/**
 * \file
 * UART
 * - \ref PINS_UART
 *
 */

/* ------------------------------------------------------------------------ */
/* UART                                                                     */
/* ------------------------------------------------------------------------ */
/*! UART pins (UTXD0 and URXD0) definitions, PA8,9. (labeled RX0->0 and TX0->1)*/
#define PINS_UART        (PIO_PA8A_URXD | PIO_PA9A_UTXD)
#define PINS_UART_FLAGS  (PIO_PERIPH_A | PIO_DEFAULT)

#define PINS_UART_MASK (PIO_PA8A_URXD | PIO_PA9A_UTXD)
#define PINS_UART_PIO  PIOA
#define PINS_UART_ID   ID_PIOA
#define PINS_UART_TYPE PIO_PERIPH_A
#define PINS_UART_ATTR PIO_DEFAULT

/**
 * \file
 * USART0
 * - \ref PIN_USART0_RXD
 * - \ref PIN_USART0_TXD
 */
/* ------------------------------------------------------------------------ */
/* USART0                                                                   */
/* ------------------------------------------------------------------------ */
/*! USART0 pin RX  (labeled RX1 19)*/
#define PIN_USART0_RXD\
	{PIO_PA10A_RXD0, PIOA, ID_PIOA, PIO_PERIPH_A, PIO_DEFAULT}
#define PIN_USART0_RXD_IDX        (PIO_PA10_IDX)
#define PIN_USART0_RXD_FLAGS      (PIO_PERIPH_A | PIO_DEFAULT)

/*! USART0 pin TX  (labeled TX1 18) */
#define PIN_USART0_TXD\
	{PIO_PA11A_TXD0, PIOA, ID_PIOA, PIO_PERIPH_A, PIO_DEFAULT}
#define PIN_USART0_TXD_IDX        (PIO_PA11_IDX)
#define PIN_USART0_TXD_FLAGS      (PIO_PERIPH_A | PIO_DEFAULT)

/**
 * \file
 * USART1
 * - \ref PIN_USART1_RXD
 * - \ref PIN_USART1_TXD
 */
/* ------------------------------------------------------------------------ */
/* USART1                                                                   */
/* ------------------------------------------------------------------------ */
/*! USART1 pin RX (labeled RX2 17) */
#define PIN_USART1_RXD\
	{PIO_PA12A_RXD1, PIOA, ID_PIOA, PIO_PERIPH_A, PIO_DEFAULT}
#define PIN_USART1_RXD_IDX        (PIO_PA12_IDX)
#define PIN_USART1_RXD_FLAGS      (PIO_PERIPH_A | PIO_DEFAULT)
/*! USART1 pin TX (labeled TX2 16) */
#define PIN_USART1_TXD\
	{PIO_PA13A_TXD1, PIOA, ID_PIOA, PIO_PERIPH_A, PIO_DEFAULT}
#define PIN_USART1_TXD_IDX        (PIO_PA13_IDX)
#define PIN_USART1_TXD_FLAGS      (PIO_PERIPH_A | PIO_DEFAULT)
/**
 * \file
 * USART3
 * - \ref PIN_USART3_RXD
 * - \ref PIN_USART3_TXD
 */

/* ------------------------------------------------------------------------ */
/* USART3                                                                   */
/* ------------------------------------------------------------------------ */
/*! USART3 pin RX (labeled RX3 15) */
#define PIN_USART3_RXD\
	{PIO_PD5B_RXD3, PIOD, ID_PIOD, PIO_PERIPH_B, PIO_DEFAULT}
#define PIN_USART3_RXD_IDX        (PIO_PD5_IDX)
#define PIN_USART3_RXD_FLAGS      (PIO_PERIPH_B | PIO_DEFAULT)
/*! USART3 pin TX (labeled RX3 14) */
#define PIN_USART3_TXD\
	{PIO_PD4B_TXD3, PIOD, ID_PIOD, PIO_PERIPH_B, PIO_DEFAULT}
#define PIN_USART3_TXD_IDX        (PIO_PD4_IDX)
#define PIN_USART3_TXD_FLAGS      (PIO_PERIPH_B | PIO_DEFAULT)
/**
 * \file
 * USB
 * - \ref PIN_USBOTG_VBOF
 * - \ref PIN_USB_FAULT
 *
 */

/* ------------------------------------------------------------------------ */
/* USB                                                                      */
/* ------------------------------------------------------------------------ */
/*! USB OTG VBus On/Off: Bus Power Control Port. */
#define PIN_UOTGHS_VBOF  { PIO_PB10, PIOB, ID_PIOB, PIO_PERIPH_A, PIO_PULLUP }
/*! USB OTG Identification: Mini Connector Identification Port. */
#define PIN_UOTGHS_ID    { PIO_PB11, PIOB, ID_PIOB, PIO_PERIPH_A, PIO_PULLUP }

/*! Multiplexed pin used for USB_ID: */
#define USB_ID                      PIO_PB11_IDX
#define USB_ID_GPIO                 (PIO_PB11_IDX)
#define USB_ID_FLAGS                (PIO_PERIPH_A | PIO_DEFAULT)
/*! Multiplexed pin used for USB_VBOF: */
#define USB_VBOF                    PIO_PB10_IDX
#define USB_VBOF_GPIO               (PIO_PB10_IDX)
#define USB_VBOF_FLAGS              (PIO_PERIPH_A | PIO_DEFAULT)
/*! Active level of the USB_VBOF output pin. */
#define USB_VBOF_ACTIVE_LEVEL       LOW


/* ------------------------------------------------------------------------ */
/* GPIO MAPPING                                                             */
/* ------------------------------------------------------------------------ */

#define PHYCMD_DIGITAL_OUTPUT_0				(PIO_PA14_IDX)
#define PHYCMD_DIGITAL_OUTPUT_0_FLAGS		(PIO_TYPE_PIO_OUTPUT_1 | PIO_DEFAULT)
#define PHYCMD_DIGITAL_OUTPUT_1				(PIO_PD0_IDX)
#define PHYCMD_DIGITAL_OUTPUT_1_FLAGS		(PIO_TYPE_PIO_OUTPUT_1 | PIO_DEFAULT)
#define PHYCMD_DIGITAL_OUTPUT_2				(PIO_PD2_IDX)
#define PHYCMD_DIGITAL_OUTPUT_2_FLAGS		(PIO_TYPE_PIO_OUTPUT_1 | PIO_DEFAULT)
#define PHYCMD_DIGITAL_OUTPUT_3				(PIO_PD6_IDX)
#define PHYCMD_DIGITAL_OUTPUT_3_FLAGS		(PIO_TYPE_PIO_OUTPUT_1 | PIO_DEFAULT)
#define PHYCMD_DIGITAL_OUTPUT_4				(PIO_PA7_IDX)
#define PHYCMD_DIGITAL_OUTPUT_4_FLAGS		(PIO_TYPE_PIO_OUTPUT_1 | PIO_DEFAULT)
#define PHYCMD_DIGITAL_OUTPUT_5				(PIO_PC1_IDX)
#define PHYCMD_DIGITAL_OUTPUT_5_FLAGS		(PIO_TYPE_PIO_OUTPUT_1 | PIO_DEFAULT)
#define PHYCMD_DIGITAL_OUTPUT_6				(PIO_PC3_IDX)
#define PHYCMD_DIGITAL_OUTPUT_6_FLAGS		(PIO_TYPE_PIO_OUTPUT_1 | PIO_DEFAULT)
#define PHYCMD_DIGITAL_OUTPUT_7				(PIO_PC5_IDX)
#define PHYCMD_DIGITAL_OUTPUT_7_FLAGS		(PIO_TYPE_PIO_OUTPUT_1 | PIO_DEFAULT)

#define PHYCMD_DIGITAL_OUTPUT_8				(PIO_PC7_IDX)
#define PHYCMD_DIGITAL_OUTPUT_8_FLAGS		(PIO_TYPE_PIO_OUTPUT_1 | PIO_DEFAULT)
#define PHYCMD_DIGITAL_OUTPUT_9				(PIO_PC9_IDX)
#define PHYCMD_DIGITAL_OUTPUT_9_FLAGS		(PIO_TYPE_PIO_OUTPUT_1 | PIO_DEFAULT)
#define PHYCMD_DIGITAL_OUTPUT_10			(PIO_PA20_IDX)
#define PHYCMD_DIGITAL_OUTPUT_10_FLAGS		(PIO_TYPE_PIO_OUTPUT_1 | PIO_DEFAULT)
#define PHYCMD_DIGITAL_OUTPUT_11			(PIO_PC18_IDX)
#define PHYCMD_DIGITAL_OUTPUT_11_FLAGS		(PIO_TYPE_PIO_OUTPUT_1 | PIO_DEFAULT)
#define PHYCMD_DIGITAL_OUTPUT_12			(PIO_PC16_IDX)
#define PHYCMD_DIGITAL_OUTPUT_12_FLAGS		(PIO_TYPE_PIO_OUTPUT_1 | PIO_DEFAULT)
#define PHYCMD_DIGITAL_OUTPUT_13			(PIO_PC14_IDX)
#define PHYCMD_DIGITAL_OUTPUT_13_FLAGS		(PIO_TYPE_PIO_OUTPUT_1 | PIO_DEFAULT)
#define PHYCMD_DIGITAL_OUTPUT_14			(PIO_PC12_IDX)
#define PHYCMD_DIGITAL_OUTPUT_14_FLAGS		(PIO_TYPE_PIO_OUTPUT_1 | PIO_DEFAULT)
#define PHYCMD_DIGITAL_OUTPUT_15			(PIO_PB14_IDX)
#define PHYCMD_DIGITAL_OUTPUT_15_FLAGS		(PIO_TYPE_PIO_OUTPUT_1 | PIO_DEFAULT)
#define PHYCMD_DIGITAL_OUTPUT_NUM			16

#define PHYCMD_DIGITAL_INPUT_0				(PIO_PB26_IDX)
#define PHYCMD_DIGITAL_INPUT_0_FLAGS		(PIO_INPUT | PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_RISE_EDGE)
#define PHYCMD_DIGITAL_INPUT_1				(PIO_PA15_IDX)
#define PHYCMD_DIGITAL_INPUT_1_FLAGS		(PIO_INPUT | PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_RISE_EDGE)
#define PHYCMD_DIGITAL_INPUT_2				(PIO_PD1_IDX)
#define PHYCMD_DIGITAL_INPUT_2_FLAGS		(PIO_INPUT | PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_RISE_EDGE)
#define PHYCMD_DIGITAL_INPUT_3				(PIO_PD3_IDX)
#define PHYCMD_DIGITAL_INPUT_3_FLAGS		(PIO_INPUT | PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_RISE_EDGE)
#define PHYCMD_DIGITAL_INPUT_4				(PIO_PD9_IDX)
#define PHYCMD_DIGITAL_INPUT_4_FLAGS		(PIO_INPUT | PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_RISE_EDGE)
#define PHYCMD_DIGITAL_INPUT_5				(PIO_PD10_IDX)
#define PHYCMD_DIGITAL_INPUT_5_FLAGS		(PIO_INPUT | PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_RISE_EDGE)
#define PHYCMD_DIGITAL_INPUT_6				(PIO_PC2_IDX)
#define PHYCMD_DIGITAL_INPUT_6_FLAGS		(PIO_INPUT | PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_RISE_EDGE)
#define PHYCMD_DIGITAL_INPUT_7				(PIO_PC4_IDX)
#define PHYCMD_DIGITAL_INPUT_7_FLAGS		(PIO_INPUT | PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_RISE_EDGE)
#define PHYCMD_DIGITAL_INPUT_8				(PIO_PC6_IDX)
#define PHYCMD_DIGITAL_INPUT_8_FLAGS		(PIO_INPUT | PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_RISE_EDGE)
#define PHYCMD_DIGITAL_INPUT_9				(PIO_PC8_IDX)
#define PHYCMD_DIGITAL_INPUT_9_FLAGS		(PIO_INPUT | PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_RISE_EDGE)
#define PHYCMD_DIGITAL_INPUT_10				(PIO_PA19_IDX)
#define PHYCMD_DIGITAL_INPUT_10_FLAGS		(PIO_INPUT | PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_RISE_EDGE)
#define PHYCMD_DIGITAL_INPUT_11				(PIO_PC19_IDX)
#define PHYCMD_DIGITAL_INPUT_11_FLAGS		(PIO_INPUT | PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_RISE_EDGE)
#define PHYCMD_DIGITAL_INPUT_12				(PIO_PC17_IDX)
#define PHYCMD_DIGITAL_INPUT_12_FLAGS		(PIO_INPUT | PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_RISE_EDGE)
#define PHYCMD_DIGITAL_INPUT_13				(PIO_PC15_IDX)
#define PHYCMD_DIGITAL_INPUT_13_FLAGS		(PIO_INPUT | PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_RISE_EDGE)
#define PHYCMD_DIGITAL_INPUT_14				(PIO_PC13_IDX)
#define PHYCMD_DIGITAL_INPUT_14_FLAGS		(PIO_INPUT | PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_RISE_EDGE)
#define PHYCMD_DIGITAL_INPUT_15				(PIO_PB21_IDX)
#define PHYCMD_DIGITAL_INPUT_15_FLAGS		(PIO_INPUT | PIO_PULLUP | PIO_DEBOUNCE | PIO_IT_RISE_EDGE)
#define PHYCMD_DIGITAL_INPUT_NUM			16	
#endif /* ARDUINO_DUE_X_H_INCLUDED */
