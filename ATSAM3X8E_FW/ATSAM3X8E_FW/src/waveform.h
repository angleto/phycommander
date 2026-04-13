/**
 * \file
 *
 * \brief PhyCommander on-chip function generator — public types and API.
 *
 * Implements the per-channel control plane documented in
 * `docs/firmware/PROTOCOL.md` (vendor SETUP requests 0x10..0x21).
 *
 * Wire layout of every struct in this header is byte-identical with
 * the host-side definitions in
 * `physerver/crates/phycmd-core/src/protocol/wave_types.rs`.
 * Both sides assert sizes at compile time; if either side's struct
 * drifts, the build fails immediately and loudly.
 */

#ifndef _WAVEFORM_H_
#define _WAVEFORM_H_

#include <stdint.h>
#include <stdbool.h>
#include "compiler.h"   /* for COMPILER_PACK_SET / COMPILER_WORD_ALIGNED */

#ifdef __cplusplus
extern "C" {
#endif

/* -------------------------------------------------------------------------
 *   Vendor SETUP request opcodes (bRequest)
 *
 *   See PROTOCOL.md §2.1 for direction / wIndex / wValue conventions.
 * ------------------------------------------------------------------------- */
#define VREQ_GEN_GET_CAPS           0x10  /* IN  → Capabilities      */
#define VREQ_GEN_GET_STATE          0x11  /* IN  → ChannelState      */
#define VREQ_GEN_PLAY_BUILTIN       0x12  /* OUT ← WaveBuiltinSpec   */
#define VREQ_GEN_PLAY_ARBITRARY     0x13  /* OUT ← WaveArbHeader+pcm */
#define VREQ_GEN_STOP               0x14  /* OUT, no payload         */
#define VREQ_DAC_SET_CLOCK          0x18  /* OUT ← u32 clock_hz      */
#define VREQ_DAC_GET_CLOCK          0x19  /* IN  → u32 clock_hz      */
#define VREQ_ADC_SET_RATE           0x20  /* OUT ← u32 rate_hz       */
#define VREQ_ADC_GET_RATE           0x21  /* IN  → u32 rate_hz       */

/* -------------------------------------------------------------------------
 *   Wave shape selector (1 byte enum, transported as uint8_t)
 *
 *   SHAPE_OFF in ChannelState means the channel is back to MANUAL
 *   (the streaming Command frame drives it). Any other value means a
 *   generator is currently active.
 * ------------------------------------------------------------------------- */
typedef enum {
	SHAPE_OFF       = 0,
	SHAPE_DC        = 1,
	SHAPE_SINE      = 2,
	SHAPE_SQUARE    = 3,
	SHAPE_TRIANGLE  = 4,
	SHAPE_SAWTOOTH  = 5,
	SHAPE_ARBITRARY = 6,
} wave_shape_t;

/* -------------------------------------------------------------------------
 *   Channel kind (1 byte enum)
 * ------------------------------------------------------------------------- */
typedef enum {
	CHAN_KIND_DAC  = 0,
	CHAN_KIND_PWM  = 1,
	CHAN_KIND_DOUT = 2,
	CHAN_KIND_DIN  = 3,
	CHAN_KIND_ADC  = 4,
} channel_kind_t;

/* -------------------------------------------------------------------------
 *   Mode bitmask used in Capabilities.modes_<kind>
 * ------------------------------------------------------------------------- */
#define MODE_MANUAL     (1u << 0)
#define MODE_BUILTIN    (1u << 1)
#define MODE_ARBITRARY  (1u << 2)
/* bits 3..7 reserved for MODE_PWM_DUTY, MODE_TC_TOGGLE,
 * MODE_CLOSED_LOOP, etc.  Set to 0 in v1 firmware. */

/* -------------------------------------------------------------------------
 *   Channel inventory exposed by this firmware revision.
 *
 *   Flat channel ID space (used in wIndex of vendor SETUP requests):
 *     0..1   → DAC0..DAC1   (full waveform generator)
 *     2..9   → PWM0..PWM7   (MANUAL only in v1)
 *     10..25 → DOUT0..DOUT15 (MANUAL only in v1)
 *     26..41 → DIN0..DIN15  (read-only)
 *     42..49 → ADC0..ADC7   (read-only)
 *
 *   The mapping is reported through GEN_GET_CAPS so the host doesn't
 *   need to hard-code it.
 * ------------------------------------------------------------------------- */
#define WAVE_NUM_DAC          2u
#define WAVE_NUM_PWM          8u
#define WAVE_NUM_DOUT        16u
#define WAVE_NUM_DIN         16u
#define WAVE_NUM_ADC          8u
#define WAVE_NUM_CHANNELS    (WAVE_NUM_DAC + WAVE_NUM_PWM + WAVE_NUM_DOUT \
                              + WAVE_NUM_DIN + WAVE_NUM_ADC)   /* 50 */

/* DAC channel IDs (used in wIndex). Other kinds are derived by offset
 * if/when the firmware grows support for them (see PROTOCOL.md §2.4). */
#define WAVE_CHID_DAC0  0u
#define WAVE_CHID_DAC1  1u

/* Hardware limits exposed via Capabilities. */
#define WAVE_MAX_DAC_SAMPLE_RATE_HZ   1000000u   /* SAM3X DACC peak */
#define WAVE_MAX_ARB_SAMPLES          1024u      /* per-channel buffer */
#define WAVE_DEFAULT_DAC_CLOCK_HZ     1000000u   /* 1 MSPS shared default */

/* -------------------------------------------------------------------------
 *   Wire-format structs (single source of truth, mirrors PROTOCOL.md §6.1)
 *
 *   All fields naturally aligned, all little-endian, no implicit padding.
 *   Sizes verified by _Static_assert below.
 * ------------------------------------------------------------------------- */

COMPILER_PACK_SET(1)

struct Capabilities {
	uint8_t  protocol_version;       /* 0  : 1 in this revision      */
	uint8_t  reserved0;              /* 1  : must be 0               */
	uint16_t firmware_minor;         /* 2                            */
	uint16_t firmware_major;         /* 4                            */
	uint16_t reserved1;              /* 6                            */
	uint8_t  num_dac;                /* 8                            */
	uint8_t  num_pwm;                /* 9                            */
	uint8_t  num_dout;               /* 10                           */
	uint8_t  num_din;                /* 11                           */
	uint8_t  num_adc;                /* 12                           */
	uint8_t  reserved2[3];           /* 13..15                       */
	uint8_t  modes_dac;              /* 16 : bitmask MODE_*          */
	uint8_t  modes_pwm;              /* 17                           */
	uint8_t  modes_dout;             /* 18                           */
	uint8_t  modes_din;              /* 19                           */
	uint8_t  modes_adc;              /* 20                           */
	uint8_t  reserved3[3];           /* 21..23                       */
	uint32_t max_dac_sample_rate_hz; /* 24                           */
	uint32_t max_arb_buffer_samples; /* 28                           */
};                                    /* total 32 bytes              */

struct WaveBuiltinSpec {
	uint8_t  shape;                  /* 0  : wave_shape_t            */
	uint8_t  flags;                  /* 1  : reserved (must be 0)    */
	uint16_t duty_x10;               /* 2  : 0..1000 = 0..100%       */
	uint16_t amplitude;              /* 4  : peak-to-peak            */
	uint16_t offset;                 /* 6  : mid-point               */
	uint32_t freq_mHz;               /* 8  : signal frequency, mHz   */
	uint16_t phase_offset_x16;       /* 12 : reserved (must be 0)    */
	uint16_t reserved1;              /* 14 : must be 0               */
};                                    /* total 16 bytes              */

struct WaveArbHeader {
	uint16_t n_samples;              /* 0  : 1..max_arb_buffer_samples */
	uint16_t loop_count;             /* 2  : 0 = infinite              */
	uint32_t sample_rate_hz;         /* 4  : playback rate             */
	/* int16_t samples[n_samples] follows */
};                                    /* total 8 bytes (header only)  */

struct ChannelState {
	uint8_t  channel_kind;           /* 0  : channel_kind_t          */
	uint8_t  channel_index;          /* 1                            */
	uint8_t  shape;                  /* 2  : SHAPE_OFF = MANUAL      */
	uint8_t  flags;                  /* 3                            */
	uint32_t freq_mHz;               /* 4                            */
	uint16_t duty_x10;               /* 8                            */
	uint16_t amplitude;              /* 10                           */
	uint16_t offset;                 /* 12                           */
	uint16_t phase_offset_x16;       /* 14                           */
	uint16_t arb_n_samples;          /* 16                           */
	uint16_t arb_loops_remaining;    /* 18                           */
	uint32_t arb_sample_rate_hz;     /* 20                           */
	uint32_t cur_phase_q24_8;        /* 24                           */
	uint32_t reserved;               /* 28                           */
};                                    /* total 32 bytes              */

COMPILER_PACK_RESET()

/* Build-time guards. If any of these fire, do NOT widen the struct
 * with extra padding — fix the field alignment so the wire layout
 * matches the doc, then tell the host to do the same. */
_Static_assert(sizeof(struct Capabilities)    == 32, "Capabilities wire size");
_Static_assert(sizeof(struct WaveBuiltinSpec) == 16, "WaveBuiltinSpec wire size");
_Static_assert(sizeof(struct WaveArbHeader)   ==  8, "WaveArbHeader wire size");
_Static_assert(sizeof(struct ChannelState)    == 32, "ChannelState wire size");

/* -------------------------------------------------------------------------
 *   Public API (called from main / udi_vendor)
 * ------------------------------------------------------------------------- */

/**
 * \brief Initialise the function generator subsystem.
 *
 * Must be called once after `dac_setup()` and before `udc_start()`.
 * Configures TC0 channel 0 as the shared DAC sample clock at
 * WAVE_DEFAULT_DAC_CLOCK_HZ, allocates the ping-pong buffers in
 * SRAM, and parks all channels in SHAPE_OFF (MANUAL, default).
 */
void waveform_init(void);

/**
 * \brief Stop every running generator and return all channels to
 * SHAPE_OFF. Called from `udi_vendor_disable()` for safety so that
 * a host disconnect can never leave a DAC oscillating in the wild.
 */
void waveform_stop_all(void);

/**
 * \brief Returns true iff DAC channel `dac_idx` (0 or 1) is currently
 * generator-driven (i.e. its shape != SHAPE_OFF). The streaming
 * `apply_command_frame()` calls this to decide whether to write the
 * Command frame's dacN field to the DAC peripheral.
 */
bool waveform_dac_is_generating(uint8_t dac_idx);

/* -------------------------------------------------------------------------
 *   Vendor SETUP entrypoints (called from udi_vendor_setup)
 *
 *   Each one returns true on success (the UDC will ACK the control
 *   transfer) or false on validation failure (the UDC will STALL).
 *
 *   `data` and `len` describe the SETUP payload (DATA stage) when
 *   present; pass NULL/0 for requests with no payload.
 * ------------------------------------------------------------------------- */

/**
 * \brief Fill `out` with the current Capabilities of this firmware.
 * Returns true if `out_len >= sizeof(Capabilities)`.
 */
bool waveform_get_caps(void *out, uint16_t out_len);

/**
 * \brief Fill `out` with the live ChannelState for the given flat
 * channel ID. STALLs if `channel_id >= WAVE_NUM_CHANNELS` or the
 * out buffer is too small.
 */
bool waveform_get_state(uint16_t channel_id, void *out, uint16_t out_len);

/**
 * \brief Start (or seamlessly update) a built-in waveform on a DAC
 * channel. Validates the spec per PROTOCOL.md §2.5. Returns true on
 * success.
 */
bool waveform_play_builtin(uint16_t channel_id, const void *data, uint16_t len);

/**
 * \brief Start (or replace) playback of an arbitrary waveform on a
 * DAC channel. The DATA payload is `WaveArbHeader` followed by
 * `n_samples * sizeof(int16_t)` bytes.
 */
bool waveform_play_arbitrary(uint16_t channel_id, const void *data, uint16_t len);

/**
 * \brief Stop the generator on the channel and revert to SHAPE_OFF.
 * Idempotent: stopping an already-stopped channel succeeds silently.
 */
bool waveform_stop(uint16_t channel_id);

/**
 * \brief Set / get the shared DAC sample clock in Hz. Range
 * 1..WAVE_MAX_DAC_SAMPLE_RATE_HZ. Both DAC channels share this clock.
 */
bool waveform_set_dac_clock(uint32_t clock_hz);
uint32_t waveform_get_dac_clock(void);

/**
 * \brief Set / get the ADC sampling rate in Hz. Independent of the
 * DAC clock; range 1..WAVE_MAX_DAC_SAMPLE_RATE_HZ.
 */
bool waveform_set_adc_rate(uint32_t rate_hz);
uint32_t waveform_get_adc_rate(void);

#ifdef __cplusplus
}
#endif

#endif /* _WAVEFORM_H_ */
