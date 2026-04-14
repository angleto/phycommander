/**
 * \file
 *
 * \brief PhyCommander firmware — Step 3 (PhyCMD-64 protocol on vendor bulk).
 *
 * Hardware initialisation (GPIO, ADC, DAC, SysTick) runs once in
 * main(). The USB transport (vendor class, two bulk endpoints) is
 * managed by udi_vendor.c. When a 64-byte frame arrives on the OUT
 * endpoint, the USB ISR callback in udi_vendor.c calls
 * process_command_frame() here, which validates the PhyCMD-64 CRC,
 * applies digital/DAC outputs, reads digital/ADC inputs, builds the
 * 64-byte status response in the caller-supplied TX buffer, and
 * returns. The ISR then queues that buffer for BULK IN transmission.
 *
 * The entire protocol round-trip is IRQ-driven — main()'s infinite
 * loop is an idle spin (no WFI, per Bug 3 from the investigation).
 */

#include <asf.h>
#include <string.h>
#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

#include "waveform.h"

/* ============================================================
 *  Protocol constants (must match physerver/src/protocol/)
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

/* ============================================================
 *  Protocol wire types (packed, 64 B each)
 * ============================================================ */

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

_Static_assert(sizeof(command_msg_t) == MSG_SIZE, "command_msg_t != 64");
_Static_assert(sizeof(status_msg_t)  == MSG_SIZE, "status_msg_t != 64");

/* ============================================================
 *  CRC-16-CCITT (poly=0x1021, init=0xFFFF, MSB-first)
 * ============================================================ */

static uint16_t crc16_ccitt(const uint8_t *data, size_t len)
{
	uint16_t crc = 0xFFFFu;
	for (size_t i = 0; i < len; i++) {
		crc ^= ((uint16_t)data[i]) << 8;
		for (int b = 0; b < 8; b++) {
			if (crc & 0x8000u)
				crc = (uint16_t)((crc << 1) ^ 0x1021u);
			else
				crc = (uint16_t)(crc << 1);
		}
	}
	return crc;
}

/* ============================================================
 *  Protocol state (persistent across frames)
 * ============================================================ */

static uint8_t  s_last_seq    = 0xFFu;
static uint16_t s_error_count = 0;
static volatile uint32_t s_uptime_ms = 0;

void SysTick_Handler(void);
void SysTick_Handler(void)
{
	s_uptime_ms++;
	waveform_systick_1ms();    /* drives PULSE_TRIG cooldown / pulse end */
}

/* ============================================================
 *  Digital I/O (16 in + 16 out, PIO direct register access)
 * ============================================================ */

static Pio *s_dig_in_ports[PHYCMD_DIGITAL_INPUT_NUM];
static Pio *s_dig_out_ports[PHYCMD_DIGITAL_OUTPUT_NUM];

static void init_dig_in_ports(void)
{
	Pio **a = s_dig_in_ports;
	a[0]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_0  >> 5)));
	a[1]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_1  >> 5)));
	a[2]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_2  >> 5)));
	a[3]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_3  >> 5)));
	a[4]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_4  >> 5)));
	a[5]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_5  >> 5)));
	a[6]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_6  >> 5)));
	a[7]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_7  >> 5)));
	a[8]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_8  >> 5)));
	a[9]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_9  >> 5)));
	a[10] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_10 >> 5)));
	a[11] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_11 >> 5)));
	a[12] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_12 >> 5)));
	a[13] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_13 >> 5)));
	a[14] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_14 >> 5)));
	a[15] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_15 >> 5)));
}

static void init_dig_out_ports(void)
{
	Pio **a = s_dig_out_ports;
	a[0]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_0  >> 5)));
	a[1]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_1  >> 5)));
	a[2]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_2  >> 5)));
	a[3]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_3  >> 5)));
	a[4]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_4  >> 5)));
	a[5]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_5  >> 5)));
	a[6]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_6  >> 5)));
	a[7]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_7  >> 5)));
	a[8]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_8  >> 5)));
	a[9]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_9  >> 5)));
	a[10] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_10 >> 5)));
	a[11] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_11 >> 5)));
	a[12] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_12 >> 5)));
	a[13] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_13 >> 5)));
	a[14] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_14 >> 5)));
	a[15] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_15 >> 5)));
}

uint16_t get_dig_in_value(void);   /* externally linkable: also called from waveform.c */
uint16_t get_dig_in_value(void)
{
	uint16_t v = 0;
	for (int i = 0; i < 16; i++) {
		static const uint32_t pins[] = {
			PHYCMD_DIGITAL_INPUT_0,  PHYCMD_DIGITAL_INPUT_1,
			PHYCMD_DIGITAL_INPUT_2,  PHYCMD_DIGITAL_INPUT_3,
			PHYCMD_DIGITAL_INPUT_4,  PHYCMD_DIGITAL_INPUT_5,
			PHYCMD_DIGITAL_INPUT_6,  PHYCMD_DIGITAL_INPUT_7,
			PHYCMD_DIGITAL_INPUT_8,  PHYCMD_DIGITAL_INPUT_9,
			PHYCMD_DIGITAL_INPUT_10, PHYCMD_DIGITAL_INPUT_11,
			PHYCMD_DIGITAL_INPUT_12, PHYCMD_DIGITAL_INPUT_13,
			PHYCMD_DIGITAL_INPUT_14, PHYCMD_DIGITAL_INPUT_15,
		};
		v |= ((s_dig_in_ports[i]->PIO_PDSR >> (pins[i] & 0x1F)) & 1u) << i;
	}
	return v;
}

/* Apply a partial DOUT update from waveform.c reactive modes. Only
 * the bits in `mask` are touched; the rest keep whatever the
 * streaming Command frame last wrote them. The pin index table is
 * defined as a function-local in set_dig_out_value() below — we
 * duplicate it here rather than promoting it to a file-static so
 * that any future renumbering stays mechanically obvious by
 * sitting next to its consumer. */
void reactive_dout_write(uint16_t mask, uint16_t bits);
void reactive_dout_write(uint16_t mask, uint16_t bits)
{
	static const uint32_t pins[] = {
		PHYCMD_DIGITAL_OUTPUT_0,  PHYCMD_DIGITAL_OUTPUT_1,
		PHYCMD_DIGITAL_OUTPUT_2,  PHYCMD_DIGITAL_OUTPUT_3,
		PHYCMD_DIGITAL_OUTPUT_4,  PHYCMD_DIGITAL_OUTPUT_5,
		PHYCMD_DIGITAL_OUTPUT_6,  PHYCMD_DIGITAL_OUTPUT_7,
		PHYCMD_DIGITAL_OUTPUT_8,  PHYCMD_DIGITAL_OUTPUT_9,
		PHYCMD_DIGITAL_OUTPUT_10, PHYCMD_DIGITAL_OUTPUT_11,
		PHYCMD_DIGITAL_OUTPUT_12, PHYCMD_DIGITAL_OUTPUT_13,
		PHYCMD_DIGITAL_OUTPUT_14, PHYCMD_DIGITAL_OUTPUT_15,
	};
	for (uint8_t i = 0; i < 16; i++) {
		if (!(mask & (1u << i))) continue;
		Pio *p = s_dig_out_ports[i];
		uint32_t pin_mask = 1u << (pins[i] & 0x1F);
		if (bits & (1u << i)) p->PIO_SODR = pin_mask;
		else                  p->PIO_CODR = pin_mask;
	}
}

static inline void set_dig_out_value(uint16_t v)
{
	static const uint32_t pins[] = {
		PHYCMD_DIGITAL_OUTPUT_0,  PHYCMD_DIGITAL_OUTPUT_1,
		PHYCMD_DIGITAL_OUTPUT_2,  PHYCMD_DIGITAL_OUTPUT_3,
		PHYCMD_DIGITAL_OUTPUT_4,  PHYCMD_DIGITAL_OUTPUT_5,
		PHYCMD_DIGITAL_OUTPUT_6,  PHYCMD_DIGITAL_OUTPUT_7,
		PHYCMD_DIGITAL_OUTPUT_8,  PHYCMD_DIGITAL_OUTPUT_9,
		PHYCMD_DIGITAL_OUTPUT_10, PHYCMD_DIGITAL_OUTPUT_11,
		PHYCMD_DIGITAL_OUTPUT_12, PHYCMD_DIGITAL_OUTPUT_13,
		PHYCMD_DIGITAL_OUTPUT_14, PHYCMD_DIGITAL_OUTPUT_15,
	};
	uint16_t reactive = waveform_reactive_dout_mask();
	for (int i = 0; i < 16; i++) {
		if (reactive & (1u << i)) continue;   /* owned by generator */
		if ((v >> i) & 1u)
			s_dig_out_ports[i]->PIO_SODR = 1u << (pins[i] & 0x1F);
		else
			s_dig_out_ports[i]->PIO_CODR = 1u << (pins[i] & 0x1F);
	}
}

static inline uint16_t get_dig_out_echo(void)
{
	uint16_t v = 0;
	static const uint32_t pins[] = {
		PHYCMD_DIGITAL_OUTPUT_0,  PHYCMD_DIGITAL_OUTPUT_1,
		PHYCMD_DIGITAL_OUTPUT_2,  PHYCMD_DIGITAL_OUTPUT_3,
		PHYCMD_DIGITAL_OUTPUT_4,  PHYCMD_DIGITAL_OUTPUT_5,
		PHYCMD_DIGITAL_OUTPUT_6,  PHYCMD_DIGITAL_OUTPUT_7,
		PHYCMD_DIGITAL_OUTPUT_8,  PHYCMD_DIGITAL_OUTPUT_9,
		PHYCMD_DIGITAL_OUTPUT_10, PHYCMD_DIGITAL_OUTPUT_11,
		PHYCMD_DIGITAL_OUTPUT_12, PHYCMD_DIGITAL_OUTPUT_13,
		PHYCMD_DIGITAL_OUTPUT_14, PHYCMD_DIGITAL_OUTPUT_15,
	};
	for (int i = 0; i < 16; i++)
		v |= ((s_dig_out_ports[i]->PIO_ODSR >> (pins[i] & 0x1F)) & 1u) << i;
	return v;
}

/* ============================================================
 *  ADC (8 channels, PDC/DMA free-running)
 * ============================================================ */

#define ADC_CHANNEL_NUM 8
uint16_t g_adc_buf[16][ADC_CHANNEL_NUM];

static void adc_setup(void)
{
	pmc_enable_periph_clk(ID_ADC);
	adc_init(ADC, sysclk_get_main_hz(), ADC_FREQ_MAX, ADC_STARTUP_FAST);
	adc_set_resolution(ADC, ADC_MR_LOWRES_BITS_12);

	for (int ch = 0; ch < ADC_CHANNEL_NUM; ch++)
		adc_enable_channel(ADC, (enum adc_channel_num_t)ch);

	ADC->ADC_MR  |= 0x80;       /* free running */
	ADC->ADC_CHER = 0x80;
	ADC->ADC_IDR  = ~(1u << 27);
	ADC->ADC_IER  = 1u << 27;
	ADC->ADC_RPR  = (uint32_t)g_adc_buf[0];
	ADC->ADC_RCR  = ADC_CHANNEL_NUM;
	ADC->ADC_RNPR = (uint32_t)g_adc_buf[1];
	ADC->ADC_RNCR = ADC_CHANNEL_NUM;
	ADC->ADC_PTCR = 1;
	ADC->ADC_CR   = 2;
	/* ADC IRQ disabled — we just read the latest snapshot from
	 * g_adc_buf[0] in the status frame builder. Sufficient for
	 * protocol bring-up. */
}

/* ============================================================
 *  DAC (2 channels, flexible selection)
 * ============================================================ */

static void dac_setup(void)
{
	pmc_enable_periph_clk(ID_DACC);
	dacc_reset(DACC);
	dacc_set_writeprotect(DACC, 0);
	dacc_set_transfer_mode(DACC, 1);
	dacc_enable_flexible_selection(DACC);
	DACC->DACC_CHER = 3;  /* enable channels 0 and 1 */
}

static inline void dac_write_pair(uint16_t dac0, uint16_t dac1)
{
	if (dac0 > DAC_MAX) dac0 = DAC_MAX;
	if (dac1 > DAC_MAX) dac1 = DAC_MAX;
	dacc_write_conversion_data(DACC, dac0);
	DACC->DACC_CDR = dac1 | 0x1000u; /* channel 1 tag */
}

/* ============================================================
 *  process_command_frame() — called from udi_vendor.c ISR
 *
 *  Validates the 64-byte command in rx_buf, applies outputs,
 *  reads inputs, and fills the 64-byte status response in tx_buf.
 * ============================================================ */

/* Tracks whether the last command applied was valid. Bulk used to
 * compute this inline and immediately fold it into the status response;
 * with the iso path apply and build run on different EPs (and may even
 * run at different rates), so we cache the last-command outcome here. */
static volatile bool s_last_cmd_valid = true;

/* Last command handler's wall-clock execution time, in microseconds.
 * Reported to the host via status_msg_t.loop_time_us so the dashboard
 * can show what fraction of a 125-us microframe we spend inside the
 * apply path. Measured with the DWT cycle counter — enabled once at
 * boot via enable_dwt_cyccnt(). */
static volatile uint16_t s_last_loop_time_us = 0;

static inline uint32_t dwt_cyccnt(void)   { return DWT->CYCCNT; }

static void enable_dwt_cyccnt(void)
{
	/* Enable trace + DWT, then the cycle counter. SAM3X (Cortex-M3
	 * r2p0) doesn't expose the DWT_LAR unlock register, so we skip
	 * it — TRCENA + CYCCNTENA is enough for this core. */
	CoreDebug->DEMCR |= CoreDebug_DEMCR_TRCENA_Msk;
	DWT->CYCCNT = 0;
	DWT->CTRL |= DWT_CTRL_CYCCNTENA_Msk;
}

void apply_command_frame(const uint8_t *rx_buf)
{
	uint32_t t0 = dwt_cyccnt();
	const command_msg_t *cmd = (const command_msg_t *)rx_buf;

	bool cmd_valid = (cmd->header == COMMAND_HEADER) &&
	                 (crc16_ccitt(rx_buf, CRC_OVER_CMD_BYTES) == cmd->crc);

	if (!cmd_valid) {
		if (s_error_count < 0xFFFFu)
			s_error_count++;
		s_last_cmd_valid = false;
		return;
	}

	/* Apply digital outputs immediately */
	set_dig_out_value(cmd->digital_out);

	/* Apply DAC outputs — but only on channels currently in MANUAL.
	 * A channel in GENERATOR mode is being driven by the on-chip
	 * TC + DACC PDC chain in waveform.c; writing to its CDR here
	 * would race with the PDC and cause glitches. */
	if (cmd->flags & FLAG_DAC_ENABLE) {
		uint16_t v0 = cmd->dac0, v1 = cmd->dac1;
		if (v0 > DAC_MAX) v0 = DAC_MAX;
		if (v1 > DAC_MAX) v1 = DAC_MAX;
		bool gen0 = waveform_dac_is_generating(0);
		bool gen1 = waveform_dac_is_generating(1);
		if (!gen0) dacc_write_conversion_data(DACC, v0);
		if (!gen1) DACC->DACC_CDR = v1 | 0x1000u;   /* tag → CH1 */
	}

	/* Sequence tracking */
	s_last_seq = cmd->seq_num;
	if (cmd->flags & FLAG_RESET_SEQ)
		s_last_seq = 0;

	s_last_cmd_valid = true;

	/* Measure handler duration. CYCCNT is 32-bit @ CPU clock; at 84 MHz
	 * wrap is every ~51 s which is way longer than any apply call. */
	uint32_t dt_cyc = dwt_cyccnt() - t0;
	uint32_t dt_us  = dt_cyc / 84u;           /* 84 MHz MCK on SAM3X */
	s_last_loop_time_us = dt_us > 0xFFFFu ? 0xFFFFu : (uint16_t)dt_us;
}

void build_status_frame(uint8_t *tx_buf)
{
	status_msg_t *stat = (status_msg_t *)tx_buf;

	/* Zero only the 64-byte protocol header. The iso path passes a
	 * 512-byte buffer with [64..511] pre-filled with zeros once at
	 * boot — re-zeroing the padding here would waste cycles every
	 * microframe. */
	memset(stat, 0, sizeof(*stat));

	stat->header      = STATUS_HEADER;
	stat->digital_in  = get_dig_in_value();
	stat->digital_out = get_dig_out_echo();

	/* ADC: read latest PDC snapshot */
	for (int i = 0; i < ADC_CHANNEL_NUM; i++)
		stat->adc[i] = g_adc_buf[0][i];

	/* Status flags */
	uint8_t sf = STATUS_USB_CONFIGURED;
	if (!s_last_cmd_valid) sf |= STATUS_ERROR_FLAG;
	stat->status_flags = sf;

	stat->seq_num       = s_last_seq;
	stat->loop_time_us  = s_last_loop_time_us;
	stat->uptime_ms     = s_uptime_ms;
	stat->error_count   = s_error_count;

	/* CRC over first 24 bytes of the status frame */
	stat->crc = crc16_ccitt(tx_buf, CRC_OVER_STAT_BYTES);
}

/* Bulk path keeps the original combined entry point — apply + build are
 * called back-to-back from a single bulk OUT callback, just like before. */
void process_command_frame(const uint8_t *rx_buf, uint8_t *tx_buf)
{
	apply_command_frame(rx_buf);
	build_status_frame(tx_buf);
}

/* ============================================================
 *  main()
 * ============================================================ */

int main(void)
{
	sysclk_init();
	irq_initialize_vectors();
	cpu_irq_enable();
	board_init();

	/* SysTick at 1 kHz for uptime_ms */
	SysTick_Config(sysclk_get_main_hz() / 1000);

	/* DWT cycle counter — used by apply_command_frame to fill
	 * status.loop_time_us. No-op on production runs since the
	 * counter just wraps freely. */
	enable_dwt_cyccnt();

	/* Disable the SAM3X watchdog (WDT_MR is write-once). */
	WDT->WDT_MR = WDT_MR_WDDIS;

	/* Initialise I/O peripherals */
	init_dig_in_ports();
	init_dig_out_ports();
	adc_setup();
	dac_setup();
	waveform_init();   /* must come after dac_setup — sets up TC0/PDC for DACC */

	/* Start USB device stack */
	udc_start();

	/* Idle spin — open-loop protocol work happens in the UOTGHS ISR
	 * via vendor_bulk_out_cb → process_command_frame, and on the
	 * iso path via vendor_iso_out_cb. The reactive paths are also
	 * mostly ISR-driven (DACC_Handler ENDTX for waveform refill,
	 * SysTick for pulse cooldowns), with one remaining job here:
	 *
	 * polling DIN at high rate so reactive LUT (DIN-mask source) and
	 * PULSE_TRIG see edges within ~1 µs. We don't use PIO change
	 * interrupts in v1 — the polled path is simpler, doesn't risk
	 * priority-inverting the UOTGHS / DACC ISRs, and at the SAM3X's
	 * 84 MHz with no other CPU work happening between IRQs we get
	 * sub-microsecond latency anyway. */
	uint16_t prev_din = get_dig_in_value();
	for (;;) {
		uint16_t now_din = get_dig_in_value();
		if (now_din != prev_din) {
			waveform_on_din_change(now_din, prev_din);
			prev_din = now_din;
		}
	}
}
