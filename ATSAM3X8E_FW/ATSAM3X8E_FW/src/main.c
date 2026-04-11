/**
 * \file
 *
 * \brief PhyCommander firmware for ATSAM3X8E (Arduino Due).
 *
 * Implements the PhyCMD-64 protocol expected by the Rust physerver:
 *   - 64-byte fixed-size frames over USB CDC
 *   - Command  (host -> device): header 0xAA55, CRC-16-CCITT over bytes [0..14)
 *   - Status   (device -> host): header 0x55AA, CRC-16-CCITT over bytes [0..24)
 *
 * Hardware setup (unchanged from the original firmware):
 *   - 16 digital inputs, 16 digital outputs (PIO)
 *   - 8-channel 12-bit ADC, DMA-buffered (free running)
 *   - 2-channel 12-bit DAC
 *
 * PWM and the comm watchdog are not implemented in this revision; the
 * corresponding status flag bits are reported as inactive.
 */

#include <asf.h>
#include <string.h>
#include <stdint.h>
#include <stddef.h>

/* ============================================================
 *  Protocol layout (must match physerver/src/protocol/types.rs)
 * ============================================================ */

#define MSG_SIZE             64
#define COMMAND_HEADER       0xAA55u
#define STATUS_HEADER        0x55AAu
#define CRC_OVER_CMD_BYTES   14u
#define CRC_OVER_STAT_BYTES  24u

#define DAC_MAX              4095u

/* Command flags */
#define FLAG_ADC_ENABLE        (1u << 0)
#define FLAG_DAC_ENABLE        (1u << 1)
#define FLAG_PWM_ENABLE        (1u << 2)
#define FLAG_RESET_SEQ         (1u << 3)
#define FLAG_WATCHDOG_DISABLE  (1u << 4)

/* Status flags */
#define STATUS_ADC_ACTIVE         (1u << 0)
#define STATUS_DAC_ACTIVE         (1u << 1)
#define STATUS_PWM_ACTIVE         (1u << 2)
#define STATUS_ERROR_FLAG         (1u << 3)
#define STATUS_WATCHDOG_TRIGGERED (1u << 4)
#define STATUS_USB_CONFIGURED     (1u << 5)
#define STATUS_OVERRUN            (1u << 6)

typedef struct __attribute__((packed)) {
	uint16_t header;        /* 0xAA55 */
	uint16_t digital_out;
	uint16_t dac0;
	uint16_t dac1;
	uint16_t pwm0;
	uint16_t pwm1;
	uint8_t  flags;
	uint8_t  seq_num;
	uint16_t crc;
	uint8_t  reserved[48];
} command_msg_t;

typedef struct __attribute__((packed)) {
	uint16_t header;        /* 0x55AA */
	uint16_t digital_in;
	uint16_t digital_out;
	uint16_t adc[8];
	uint8_t  status_flags;
	uint8_t  seq_num;
	uint16_t crc;
	uint16_t loop_time_us;
	uint32_t uptime_ms;
	uint16_t error_count;
	uint8_t  reserved[30];
} status_msg_t;

_Static_assert(sizeof(command_msg_t) == MSG_SIZE, "command_msg_t must be 64 bytes");
_Static_assert(sizeof(status_msg_t)  == MSG_SIZE, "status_msg_t must be 64 bytes");

/* ============================================================
 *  CRC-16-CCITT (poly=0x1021, init=0xFFFF, no final XOR, MSB-first).
 *  Bit-by-bit implementation; matches physerver/src/protocol/crc.rs.
 * ============================================================ */

static uint16_t crc16_ccitt(const uint8_t *data, size_t len)
{
	uint16_t crc = 0xFFFFu;
	for (size_t i = 0; i < len; i++) {
		crc ^= ((uint16_t)data[i]) << 8;
		for (int b = 0; b < 8; b++) {
			if (crc & 0x8000u) {
				crc = (uint16_t)((crc << 1) ^ 0x1021u);
			} else {
				crc = (uint16_t)(crc << 1);
			}
		}
	}
	return crc;
}

/* ============================================================
 *  Buffers (single instance, statically allocated)
 * ============================================================ */

static command_msg_t s_cmd;
static status_msg_t  s_stat;

/* I/O scratch (read/write directly into the packed structures) */
static uint8_t * const s_in  = (uint8_t *)&s_cmd;
static uint8_t * const s_out = (uint8_t *)&s_stat;

/* ============================================================
 *  Uptime / loop time
 *  SysTick @ 1 kHz produces millisecond uptime. Loop time is
 *  measured by reading the cycle counter (DWT or fallback).
 * ============================================================ */

static volatile uint32_t s_uptime_ms = 0;
static uint16_t s_error_count = 0;
static uint16_t s_last_loop_us = 0;

/* BISECTION: re-add SysTick_Handler ONLY (without ADC_Handler) */
void SysTick_Handler(void); /* prototype */
void SysTick_Handler(void)
{
	s_uptime_ms++;
}

static inline uint32_t cycles_now(void)
{
	/* DWT->CYCCNT is enabled by ASF cycle_counter init; if not, returns 0. */
	return DWT->CYCCNT;
}

static inline uint16_t cycles_to_us(uint32_t cycles)
{
	/* sysclk_get_main_hz() returns 84_000_000 on Arduino Due. */
	uint32_t mhz = sysclk_get_main_hz() / 1000000u;
	if (mhz == 0) mhz = 84;
	uint32_t us = cycles / mhz;
	if (us > 0xFFFFu) us = 0xFFFFu;
	return (uint16_t)us;
}

/* ============================================================
 *  Existing GPIO / ADC / DAC bringup (kept verbatim from original)
 * ============================================================ */

bool main_callback_cdc_enable(void);
void main_callback_cdc_disable(void);
void my_callback_rx_notify(uint8_t port);
void my_callback_tx_empty_notify(uint8_t port);
void my_callback_config(uint8_t port, usb_cdc_line_coding_t *cfg);
void my_callback_cdc_set_dtr(uint8_t port, bool b_enable);
void my_callback_cdc_set_rts(uint8_t port, bool b_enable);

bool main_callback_cdc_enable(void) { return true; }
void main_callback_cdc_disable(void) { }

void my_callback_rx_notify(uint8_t port) { (void)port; }
void my_callback_tx_empty_notify(uint8_t port) { (void)port; }
void my_callback_config(uint8_t port, usb_cdc_line_coding_t *cfg) { (void)port; (void)cfg; }
void my_callback_cdc_set_dtr(uint8_t port, bool b_enable) { (void)port; (void)b_enable; }
void my_callback_cdc_set_rts(uint8_t port, bool b_enable) { (void)port; (void)b_enable; }

static Pio *s_dig_in_ports[PHYCMD_DIGITAL_INPUT_NUM];
static Pio *s_dig_out_ports[PHYCMD_DIGITAL_OUTPUT_NUM];

static void init_dig_in_ports(Pio **arr)
{
	arr[0]  = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_0  >> 5)));
	arr[1]  = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_1  >> 5)));
	arr[2]  = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_2  >> 5)));
	arr[3]  = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_3  >> 5)));
	arr[4]  = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_4  >> 5)));
	arr[5]  = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_5  >> 5)));
	arr[6]  = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_6  >> 5)));
	arr[7]  = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_7  >> 5)));
	arr[8]  = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_8  >> 5)));
	arr[9]  = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_9  >> 5)));
	arr[10] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_10 >> 5)));
	arr[11] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_11 >> 5)));
	arr[12] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_12 >> 5)));
	arr[13] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_13 >> 5)));
	arr[14] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_14 >> 5)));
	arr[15] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_15 >> 5)));
}

static void init_dig_out_ports(Pio **arr)
{
	arr[0]  = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_0  >> 5)));
	arr[1]  = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_1  >> 5)));
	arr[2]  = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_2  >> 5)));
	arr[3]  = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_3  >> 5)));
	arr[4]  = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_4  >> 5)));
	arr[5]  = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_5  >> 5)));
	arr[6]  = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_6  >> 5)));
	arr[7]  = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_7  >> 5)));
	arr[8]  = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_8  >> 5)));
	arr[9]  = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_9  >> 5)));
	arr[10] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_10 >> 5)));
	arr[11] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_11 >> 5)));
	arr[12] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_12 >> 5)));
	arr[13] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_13 >> 5)));
	arr[14] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_14 >> 5)));
	arr[15] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_15 >> 5)));
}

static inline uint16_t get_dig_in_value(void)
{
	uint16_t v = 0;
	v |= ((s_dig_in_ports[0]->PIO_PDSR  >> (PHYCMD_DIGITAL_INPUT_0  & 0x1F)) & 1u) << 0;
	v |= ((s_dig_in_ports[1]->PIO_PDSR  >> (PHYCMD_DIGITAL_INPUT_1  & 0x1F)) & 1u) << 1;
	v |= ((s_dig_in_ports[2]->PIO_PDSR  >> (PHYCMD_DIGITAL_INPUT_2  & 0x1F)) & 1u) << 2;
	v |= ((s_dig_in_ports[3]->PIO_PDSR  >> (PHYCMD_DIGITAL_INPUT_3  & 0x1F)) & 1u) << 3;
	v |= ((s_dig_in_ports[4]->PIO_PDSR  >> (PHYCMD_DIGITAL_INPUT_4  & 0x1F)) & 1u) << 4;
	v |= ((s_dig_in_ports[5]->PIO_PDSR  >> (PHYCMD_DIGITAL_INPUT_5  & 0x1F)) & 1u) << 5;
	v |= ((s_dig_in_ports[6]->PIO_PDSR  >> (PHYCMD_DIGITAL_INPUT_6  & 0x1F)) & 1u) << 6;
	v |= ((s_dig_in_ports[7]->PIO_PDSR  >> (PHYCMD_DIGITAL_INPUT_7  & 0x1F)) & 1u) << 7;
	v |= ((s_dig_in_ports[8]->PIO_PDSR  >> (PHYCMD_DIGITAL_INPUT_8  & 0x1F)) & 1u) << 8;
	v |= ((s_dig_in_ports[9]->PIO_PDSR  >> (PHYCMD_DIGITAL_INPUT_9  & 0x1F)) & 1u) << 9;
	v |= ((s_dig_in_ports[10]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_10 & 0x1F)) & 1u) << 10;
	v |= ((s_dig_in_ports[11]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_11 & 0x1F)) & 1u) << 11;
	v |= ((s_dig_in_ports[12]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_12 & 0x1F)) & 1u) << 12;
	v |= ((s_dig_in_ports[13]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_13 & 0x1F)) & 1u) << 13;
	v |= ((s_dig_in_ports[14]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_14 & 0x1F)) & 1u) << 14;
	v |= ((s_dig_in_ports[15]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_15 & 0x1F)) & 1u) << 15;
	return v;
}

static inline void set_dig_out_value(uint16_t v)
{
#define PHY_SET(idx, def) \
	if ((v >> (idx)) & 1u) \
		s_dig_out_ports[idx]->PIO_SODR = 1u << ((def) & 0x1F); \
	else \
		s_dig_out_ports[idx]->PIO_CODR = 1u << ((def) & 0x1F)

	PHY_SET(0,  PHYCMD_DIGITAL_OUTPUT_0);
	PHY_SET(1,  PHYCMD_DIGITAL_OUTPUT_1);
	PHY_SET(2,  PHYCMD_DIGITAL_OUTPUT_2);
	PHY_SET(3,  PHYCMD_DIGITAL_OUTPUT_3);
	PHY_SET(4,  PHYCMD_DIGITAL_OUTPUT_4);
	PHY_SET(5,  PHYCMD_DIGITAL_OUTPUT_5);
	PHY_SET(6,  PHYCMD_DIGITAL_OUTPUT_6);
	PHY_SET(7,  PHYCMD_DIGITAL_OUTPUT_7);
	PHY_SET(8,  PHYCMD_DIGITAL_OUTPUT_8);
	PHY_SET(9,  PHYCMD_DIGITAL_OUTPUT_9);
	PHY_SET(10, PHYCMD_DIGITAL_OUTPUT_10);
	PHY_SET(11, PHYCMD_DIGITAL_OUTPUT_11);
	PHY_SET(12, PHYCMD_DIGITAL_OUTPUT_12);
	PHY_SET(13, PHYCMD_DIGITAL_OUTPUT_13);
	PHY_SET(14, PHYCMD_DIGITAL_OUTPUT_14);
	PHY_SET(15, PHYCMD_DIGITAL_OUTPUT_15);
#undef PHY_SET
}

static inline uint16_t get_dig_out_echo(void)
{
	uint16_t v = 0;
	v |= ((s_dig_out_ports[0]->PIO_ODSR  >> (PHYCMD_DIGITAL_OUTPUT_0  & 0x1F)) & 1u) << 0;
	v |= ((s_dig_out_ports[1]->PIO_ODSR  >> (PHYCMD_DIGITAL_OUTPUT_1  & 0x1F)) & 1u) << 1;
	v |= ((s_dig_out_ports[2]->PIO_ODSR  >> (PHYCMD_DIGITAL_OUTPUT_2  & 0x1F)) & 1u) << 2;
	v |= ((s_dig_out_ports[3]->PIO_ODSR  >> (PHYCMD_DIGITAL_OUTPUT_3  & 0x1F)) & 1u) << 3;
	v |= ((s_dig_out_ports[4]->PIO_ODSR  >> (PHYCMD_DIGITAL_OUTPUT_4  & 0x1F)) & 1u) << 4;
	v |= ((s_dig_out_ports[5]->PIO_ODSR  >> (PHYCMD_DIGITAL_OUTPUT_5  & 0x1F)) & 1u) << 5;
	v |= ((s_dig_out_ports[6]->PIO_ODSR  >> (PHYCMD_DIGITAL_OUTPUT_6  & 0x1F)) & 1u) << 6;
	v |= ((s_dig_out_ports[7]->PIO_ODSR  >> (PHYCMD_DIGITAL_OUTPUT_7  & 0x1F)) & 1u) << 7;
	v |= ((s_dig_out_ports[8]->PIO_ODSR  >> (PHYCMD_DIGITAL_OUTPUT_8  & 0x1F)) & 1u) << 8;
	v |= ((s_dig_out_ports[9]->PIO_ODSR  >> (PHYCMD_DIGITAL_OUTPUT_9  & 0x1F)) & 1u) << 9;
	v |= ((s_dig_out_ports[10]->PIO_ODSR >> (PHYCMD_DIGITAL_OUTPUT_10 & 0x1F)) & 1u) << 10;
	v |= ((s_dig_out_ports[11]->PIO_ODSR >> (PHYCMD_DIGITAL_OUTPUT_11 & 0x1F)) & 1u) << 11;
	v |= ((s_dig_out_ports[12]->PIO_ODSR >> (PHYCMD_DIGITAL_OUTPUT_12 & 0x1F)) & 1u) << 12;
	v |= ((s_dig_out_ports[13]->PIO_ODSR >> (PHYCMD_DIGITAL_OUTPUT_13 & 0x1F)) & 1u) << 13;
	v |= ((s_dig_out_ports[14]->PIO_ODSR >> (PHYCMD_DIGITAL_OUTPUT_14 & 0x1F)) & 1u) << 14;
	v |= ((s_dig_out_ports[15]->PIO_ODSR >> (PHYCMD_DIGITAL_OUTPUT_15 & 0x1F)) & 1u) << 15;
	return v;
}

/* ---- ADC with PDC/DMA --------------------------------------------------- */
#define ADC_CHANNEL_NUM 8

volatile int g_bufn;
uint16_t g_adc_buf[16][ADC_CHANNEL_NUM];

/* No ADC PDC chain servicing in this revision: the ADC fills the first
 * two buffers (g_adc_buf[0], g_adc_buf[1]) then stops. We always read
 * g_adc_buf[0] in the main loop, which holds the last sampled values.
 * Adequate for protocol bring-up; can be replaced with proper IRQ-based
 * chaining once everything else is verified. */

static void adc_setup(void)
{
	pmc_enable_periph_clk(ID_ADC);
	adc_init(ADC, sysclk_get_main_hz(), ADC_FREQ_MAX, ADC_STARTUP_FAST);

	adc_set_resolution(ADC, ADC_MR_LOWRES_BITS_12);

	adc_enable_channel(ADC, ADC_CHANNEL_0);
	adc_enable_channel(ADC, ADC_CHANNEL_1);
	adc_enable_channel(ADC, ADC_CHANNEL_2);
	adc_enable_channel(ADC, ADC_CHANNEL_3);
	adc_enable_channel(ADC, ADC_CHANNEL_4);
	adc_enable_channel(ADC, ADC_CHANNEL_5);
	adc_enable_channel(ADC, ADC_CHANNEL_6);
	adc_enable_channel(ADC, ADC_CHANNEL_7);

	ADC->ADC_MR |= 0x80;   /* free running */
	ADC->ADC_CHER = 0x80;
	ADC->ADC_IDR = ~(1u << 27);
	ADC->ADC_IER = 1u << 27;
	ADC->ADC_RPR = (uint32_t)g_adc_buf[0];
	ADC->ADC_RCR = ADC_CHANNEL_NUM;
	ADC->ADC_RNPR = (uint32_t)g_adc_buf[1];
	ADC->ADC_RNCR = ADC_CHANNEL_NUM;
	g_bufn = 1;
	ADC->ADC_PTCR = 1;
	ADC->ADC_CR = 2;

	/* TEMPORARY: ADC IRQ disabled to bisect the boot crash */
	/* NVIC_EnableIRQ(ADC_IRQn); */
}

static void dac_setup(void)
{
	pmc_enable_periph_clk(ID_DACC);
	dacc_reset(DACC);
	dacc_set_writeprotect(DACC, 0);
	dacc_set_transfer_mode(DACC, 1);
	dacc_enable_flexible_selection(DACC);
	DACC->DACC_CHER = 3;  /* enable channel 0 and 1 */
	/* dacc_set_timing(DACC, 0x01, 1, DACC_MR_STARTUP_0);
	 * This call hung the chip on this build. Default timing after
	 * dacc_reset is used instead. */
	/* dacc_set_analog_control(DACC, ...) -- power optimization, not needed */
}

static inline void dac_write_pair(uint16_t dac0, uint16_t dac1)
{
	if (dac0 > DAC_MAX) dac0 = DAC_MAX;
	if (dac1 > DAC_MAX) dac1 = DAC_MAX;
	/* Bit 12 is the channel-select tag in flexible selection mode for DACC. */
	uint32_t word = ((uint32_t)(dac1 | 0x1000u) << 16) | (uint32_t)dac0;
	dacc_write_conversion_data(DACC, word);
}

/* ============================================================
 *  Main loop
 * ============================================================ */

/* Process one fully-validated command and emit the corresponding status. */
static void process_command_and_reply(uint8_t *last_seq)
{
	bool cmd_valid = (s_cmd.header == COMMAND_HEADER) &&
	                 (crc16_ccitt(s_in, CRC_OVER_CMD_BYTES) == s_cmd.crc);

	if (!cmd_valid) {
		if (s_error_count != 0xFFFFu) {
			s_error_count++;
		}
	} else {
		set_dig_out_value(s_cmd.digital_out);
		*last_seq = s_cmd.seq_num;
		if (s_cmd.flags & FLAG_RESET_SEQ) {
			*last_seq = 0;
		}
	}

	memset(&s_stat, 0, sizeof(s_stat));
	s_stat.header      = STATUS_HEADER;
	s_stat.digital_in  = get_dig_in_value();
	s_stat.digital_out = get_dig_out_echo();
	uint8_t sf = STATUS_USB_CONFIGURED;
	if (!cmd_valid) sf |= STATUS_ERROR_FLAG;
	s_stat.status_flags = sf;
	s_stat.seq_num      = *last_seq;
	s_stat.error_count  = s_error_count;
	s_stat.crc          = crc16_ccitt(s_out, CRC_OVER_STAT_BYTES);

	/* Send the response. udi_cdc_write_buf returns the number of
	 * bytes that COULD NOT be queued — we ignore it because the
	 * host will simply time out for one frame and retry, and the
	 * next iteration will produce a fresh response. */
	(void)udi_cdc_write_buf(s_out, MSG_SIZE);
}

int main(void)
{
	sysclk_init();
	irq_initialize_vectors();
	cpu_irq_enable();
	board_init();

	udc_start();

	init_dig_in_ports(s_dig_in_ports);
	init_dig_out_ports(s_dig_out_ports);

	/* Disable the SAM3X watchdog (WDT_MR is write-once). Recovery from
	 * a hung firmware is handled at the systemd level: physerver has
	 * Restart=always so a stuck transport will retry indefinitely. */
	WDT->WDT_MR = WDT_MR_WDDIS;

	uint8_t last_seq = 0xFFu;
	int collected = 0;
	uint8_t in_buf[MSG_SIZE];

	/* Byte-by-byte framing loop. Two key properties make it robust
	 * against the ASF UDI_CDC double-buffer race that bites the
	 * udi_cdc_read_buf() block path:
	 *
	 *   1. We never ask UDI_CDC for more bytes than it currently has
	 *      in the visible (selected) buffer — we only call read_buf()
	 *      with `min(available, remaining)`. read_buf() never has to
	 *      wait, so it cannot deadlock waiting for the other buffer.
	 *
	 *   2. We re-synchronise on the 0x55 0xAA frame header on every
	 *      received byte, so even if a frame is dropped or chopped
	 *      we re-align on the next valid header without losing the
	 *      stream forever.
	 */
	for (;;) {
		iram_size_t avail = udi_cdc_get_nb_received_data();
		if (avail == 0) {
			/* Yield to interrupts. WFI wakes on any pending
			 * interrupt (USB SOF, transfer complete, SETUP). */
			__asm__ volatile ("wfi");
			continue;
		}

		/* Read up to one frame worth of bytes from the current buffer. */
		uint8_t chunk[MSG_SIZE];
		iram_size_t to_read = avail;
		if (to_read > MSG_SIZE) {
			to_read = MSG_SIZE;
		}
		(void)udi_cdc_read_buf(chunk, to_read);

		for (iram_size_t i = 0; i < to_read; i++) {
			uint8_t b = chunk[i];
			switch (collected) {
			case 0:
				if (b == 0x55) {
					in_buf[0] = 0x55;
					collected = 1;
				}
				break;
			case 1:
				if (b == 0xAA) {
					in_buf[1] = 0xAA;
					collected = 2;
				} else if (b == 0x55) {
					/* still a possible header start */
					in_buf[0] = 0x55;
				} else {
					collected = 0;
				}
				break;
			default:
				in_buf[collected++] = b;
				if (collected == MSG_SIZE) {
					memcpy(s_in, in_buf, MSG_SIZE);
					process_command_and_reply(&last_seq);
					collected = 0;
				}
				break;
			}
		}
	}
}
