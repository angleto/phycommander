/*
 * SPDX-License-Identifier: GPL-3.0-or-later
 * SPDX-FileCopyrightText: 2014-2026 Angelo Leto <angelo@leto.blue>
 */
/**
 * \file
 *
 * \brief PhyCommander Vendor Class bulk interface — descriptors + UDI.
 *
 * This module replaces ASF's UDI_CDC layer. It provides:
 *   - USB device descriptor
 *   - USB device qualifier descriptor (for HS)
 *   - USB configuration descriptor (FS + HS variants), each containing
 *     one vendor-specific interface with two bulk endpoints
 *   - A minimal udi_api_t implementation that UDC calls on
 *     SET_CONFIGURATION / SET_INTERFACE / vendor SETUP requests
 *   - The top-level udc_config that UDC walks on enumeration
 *
 * Step 1 (descriptor-only stub): enable/disable callbacks are empty
 * placeholders. No bulk traffic handled yet. The intent of this step
 * is to verify the device enumerates correctly on the host with the
 * expected VID/PID/endpoint layout.
 */

#include "conf_usb.h"
#include "usb_protocol.h"
#include "udd.h"
#include "udc_desc.h"
#include "udi.h"
#include "udi_vendor.h"
#include <string.h>

/* Defined in main.c — processes a PhyCMD-64 command frame and fills
 * the 64-byte status response.  Called from ISR context. */
extern void process_command_frame(const uint8_t *rx_buf, uint8_t *tx_buf);

/* Defined in main.c — split halves of process_command_frame() used by
 * the iso path, where command and status flow through separate EPs and
 * are not bound to a single bulk callback. */
extern void apply_command_frame(const uint8_t *rx_buf);
extern void build_status_frame(uint8_t *tx_buf);

#include "waveform.h"   /* vendor SETUP requests */

/* -------------------------------------------------------------------------
 *   Device descriptor
 * ------------------------------------------------------------------------- */

COMPILER_WORD_ALIGNED
UDC_DESC_STORAGE usb_dev_desc_t udc_device_desc = {
	.bLength            = sizeof(usb_dev_desc_t),
	.bDescriptorType    = USB_DT_DEVICE,
	.bcdUSB             = LE16(USB_V2_0),
	.bDeviceClass       = 0xFF,   /* Vendor-specific */
	.bDeviceSubClass    = 0x00,
	.bDeviceProtocol    = 0x00,
	.bMaxPacketSize0    = USB_DEVICE_EP_CTRL_SIZE,
	.idVendor           = LE16(USB_DEVICE_VENDOR_ID),
	.idProduct          = LE16(USB_DEVICE_PRODUCT_ID),
	.bcdDevice          = LE16((USB_DEVICE_MAJOR_VERSION << 8)
	                           | USB_DEVICE_MINOR_VERSION),
	.iManufacturer      = 1,
	.iProduct           = 2,
	.iSerialNumber      = 3,
	.bNumConfigurations = 1,
};

#ifdef USB_DEVICE_HS_SUPPORT
COMPILER_WORD_ALIGNED
UDC_DESC_STORAGE usb_dev_qual_desc_t udc_device_qual = {
	.bLength            = sizeof(usb_dev_qual_desc_t),
	.bDescriptorType    = USB_DT_DEVICE_QUALIFIER,
	.bcdUSB             = LE16(USB_V2_0),
	.bDeviceClass       = 0xFF,
	.bDeviceSubClass    = 0x00,
	.bDeviceProtocol    = 0x00,
	.bMaxPacketSize0    = USB_DEVICE_EP_CTRL_SIZE,
	.bNumConfigurations = 1,
	.bReserved          = 0,
};
#endif

/* -------------------------------------------------------------------------
 *   Configuration descriptor (interface + 2 bulk + 2 iso endpoints)
 *
 *   Layout (62 bytes total):
 *     [9]  usb_conf_desc_t
 *     [9]  usb_iface_desc_t
 *     [7]  usb_ep_desc_t (EP IN  bulk, 0x81)
 *     [7]  usb_ep_desc_t (EP OUT bulk, 0x02)
 *     [7]  usb_ep_desc_t (EP IN  iso,  0x83)
 *     [7]  usb_ep_desc_t (EP OUT iso,  0x04)
 *
 *   Iso EPs share the same interface (alt setting 0). Hosts that only
 *   want bulk just submit on EP1/EP2 and ignore the iso EPs entirely;
 *   the iso EPs only consume bandwidth when the host actively schedules
 *   transfers on them.
 * ------------------------------------------------------------------------- */

COMPILER_PACK_SET(1)
typedef struct {
	usb_conf_desc_t   conf;
	usb_iface_desc_t  iface;
	usb_ep_desc_t     ep_in;
	usb_ep_desc_t     ep_out;
	usb_ep_desc_t     ep_iso_in;
	usb_ep_desc_t     ep_iso_out;
} udi_vendor_desc_t;
COMPILER_PACK_RESET()

/**
 * \brief Build one configuration descriptor at compile time.
 *
 * \param bulk_size  Bulk EP wMaxPacketSize (64 for FS, 512 for HS).
 * \param iso_size   Iso  EP wMaxPacketSize (64 for FS, 512 for HS).
 */
#define UDI_VENDOR_DESC_INIT(bulk_size, iso_size)                        \
{                                                                        \
	.conf = {                                                        \
		.bLength             = sizeof(usb_conf_desc_t),          \
		.bDescriptorType     = USB_DT_CONFIGURATION,             \
		.wTotalLength        = LE16(sizeof(udi_vendor_desc_t)),  \
		.bNumInterfaces      = 1,                                \
		.bConfigurationValue = 1,                                \
		.iConfiguration      = 0,                                \
		.bmAttributes        = USB_CONFIG_ATTR_MUST_SET          \
		                     | USB_DEVICE_ATTR,                  \
		.bMaxPower           = USB_CONFIG_MAX_POWER(USB_DEVICE_POWER), \
	},                                                               \
	.iface = {                                                       \
		.bLength            = sizeof(usb_iface_desc_t),          \
		.bDescriptorType    = USB_DT_INTERFACE,                  \
		.bInterfaceNumber   = UDI_VENDOR_IFACE_NUMBER,           \
		.bAlternateSetting  = 0,                                 \
		.bNumEndpoints      = 4,                                 \
		.bInterfaceClass    = 0xFF,                              \
		.bInterfaceSubClass = 0x00,                              \
		.bInterfaceProtocol = 0x00,                              \
		.iInterface         = 0,                                 \
	},                                                               \
	.ep_in = {                                                       \
		.bLength          = sizeof(usb_ep_desc_t),               \
		.bDescriptorType  = USB_DT_ENDPOINT,                     \
		.bEndpointAddress = UDI_VENDOR_EP_IN,                    \
		.bmAttributes     = USB_EP_TYPE_BULK,                    \
		.wMaxPacketSize   = LE16(bulk_size),                     \
		.bInterval        = 0,                                   \
	},                                                               \
	.ep_out = {                                                      \
		.bLength          = sizeof(usb_ep_desc_t),               \
		.bDescriptorType  = USB_DT_ENDPOINT,                     \
		.bEndpointAddress = UDI_VENDOR_EP_OUT,                   \
		.bmAttributes     = USB_EP_TYPE_BULK,                    \
		.wMaxPacketSize   = LE16(bulk_size),                     \
		.bInterval        = 0,                                   \
	},                                                               \
	.ep_iso_in = {                                                   \
		.bLength          = sizeof(usb_ep_desc_t),               \
		.bDescriptorType  = USB_DT_ENDPOINT,                     \
		.bEndpointAddress = UDI_VENDOR_EP_ISO_IN,                \
		.bmAttributes     = USB_EP_TYPE_ISOCHRONOUS,             \
		.wMaxPacketSize   = LE16(iso_size),                      \
		.bInterval        = UDI_VENDOR_EP_ISO_INTERVAL,          \
	},                                                               \
	.ep_iso_out = {                                                  \
		.bLength          = sizeof(usb_ep_desc_t),               \
		.bDescriptorType  = USB_DT_ENDPOINT,                     \
		.bEndpointAddress = UDI_VENDOR_EP_ISO_OUT,               \
		.bmAttributes     = USB_EP_TYPE_ISOCHRONOUS,             \
		.wMaxPacketSize   = LE16(iso_size),                      \
		.bInterval        = UDI_VENDOR_EP_ISO_INTERVAL,          \
	},                                                               \
}

COMPILER_WORD_ALIGNED
UDC_DESC_STORAGE udi_vendor_desc_t udc_desc_fs =
	UDI_VENDOR_DESC_INIT(UDI_VENDOR_EP_SIZE_FS, UDI_VENDOR_EP_SIZE_ISO_FS);

#ifdef USB_DEVICE_HS_SUPPORT
COMPILER_WORD_ALIGNED
UDC_DESC_STORAGE udi_vendor_desc_t udc_desc_hs =
	UDI_VENDOR_DESC_INIT(UDI_VENDOR_EP_SIZE_HS, UDI_VENDOR_EP_SIZE_ISO_HS);
#endif

/* -------------------------------------------------------------------------
 *   Bulk endpoint handlers — Step 2 raw echo, deeply pipelined
 *
 *   Goal: let the host pipeline many exchanges in flight without
 *   corrupting responses. The critical constraint is that
 *   udd_ep_run(IN, ...) is a *single-transfer* queue on UOTGHS — the
 *   second submit while the first is pending is silently dropped.
 *
 *   Architecture (ring buffer with manual IN queue):
 *
 *     s_tx_buf[N]    : ring of N TX payloads, N = NUM_TX_SLOTS
 *     s_tx_head      : index of the next slot vendor_bulk_out_cb() will fill
 *     s_tx_tail      : index of the next slot that must go out on IN
 *     s_in_busy      : true iff an IN transfer is currently in flight
 *
 *   Flow:
 *     vendor_bulk_out_cb()
 *       - memcpy's s_rx_buf into s_tx_buf[s_tx_head]
 *       - advances s_tx_head
 *       - calls kick_in_queue() which submits the slot at s_tx_tail
 *         on IN *only* if s_in_busy is false (otherwise leaves it
 *         queued; cb_in will pick it up)
 *       - re-arms BULK-OUT unconditionally
 *
 *     vendor_bulk_in_cb()
 *       - advances s_tx_tail (the slot just transmitted is now free)
 *       - clears s_in_busy
 *       - calls kick_in_queue() to drain any queued slots
 *
 *   All callbacks run from the UOTGHS interrupt handler so they are
 *   serialised w.r.t. each other — no atomic ops needed.
 *
 *   If the host pipelines faster than the chip can drain the IN queue
 *   and the ring fills (s_tx_head would collide with s_tx_tail),
 *   the incoming frame is DROPPED on the firmware side (safer than
 *   corrupting a pending reply). The host will observe this as a
 *   missing response and time out on that slot. With NUM_TX_SLOTS=64
 *   this never happens in practice at the rates we target.
 * ------------------------------------------------------------------------- */

#define NUM_TX_SLOTS  64        /* must be a power of two */
#define TX_MASK       (NUM_TX_SLOTS - 1)

static COMPILER_WORD_ALIGNED uint8_t s_rx_buf[64];
static COMPILER_WORD_ALIGNED uint8_t s_tx_buf[NUM_TX_SLOTS][64];
static uint8_t s_tx_head;
static uint8_t s_tx_tail;
static bool    s_in_busy;
static uint32_t s_dropped_frames;  /* RX ring-full counter (debug) */

static void vendor_bulk_in_cb(udd_ep_status_t status,
                              iram_size_t n,
                              udd_ep_id_t ep);

static inline void kick_in_queue(void)
{
	if (s_in_busy) return;
	if (s_tx_tail == s_tx_head) return;           /* empty */
	uint8_t idx = s_tx_tail;
	s_in_busy = true;
	(void)udd_ep_run(UDI_VENDOR_EP_IN, true,
	                 s_tx_buf[idx], sizeof(s_tx_buf[idx]),
	                 vendor_bulk_in_cb);
}

static void vendor_bulk_in_cb(udd_ep_status_t status,
                              iram_size_t n,
                              udd_ep_id_t ep)
{
	(void)status; (void)n; (void)ep;
	/* The slot at s_tx_tail has now gone out. Free it and try to
	 * start the next one. */
	s_tx_tail = (s_tx_tail + 1) & TX_MASK;
	s_in_busy = false;
	kick_in_queue();
}

static void vendor_bulk_out_cb(udd_ep_status_t status,
                               iram_size_t n,
                               udd_ep_id_t ep)
{
	(void)ep;

	if (status != UDD_EP_TRANSFER_OK) {
		/* Transfer aborted (e.g. interface disabled, cable unplugged).
		 * Do not touch the endpoint further — UDC will have freed it. */
		return;
	}

	if (n == sizeof(s_rx_buf)) {
		/* Check if ring has room: if (head+1)&MASK == tail -> full */
		uint8_t next_head = (s_tx_head + 1) & TX_MASK;
		if (next_head == s_tx_tail) {
			/* Ring full: drop the frame and count it. The host
			 * will notice a timeout/missing response on one of
			 * its in-flight slots. Tuning NUM_TX_SLOTS upward
			 * avoids this path. */
			s_dropped_frames++;
		} else {
			uint8_t idx = s_tx_head;
			/* Step 3: validate CRC, apply outputs, build status
			 * frame into the TX ring slot. */
			process_command_frame(s_rx_buf, s_tx_buf[idx]);
			s_tx_head = next_head;
			kick_in_queue();
		}
	}

	/* Re-arm BULK-OUT for the next incoming frame. */
	(void)udd_ep_run(UDI_VENDOR_EP_OUT, true,
	                 s_rx_buf, sizeof(s_rx_buf),
	                 vendor_bulk_out_cb);
}

/* -------------------------------------------------------------------------
 *   Isochronous endpoint handlers (Phase 1: dual-mode coexisting with bulk)
 *
 *   The host submits a stream of iso transfers on EP 0x83 (IN) and 0x04
 *   (OUT). Each transfer is wMaxPacketSize=512 B, scheduled every
 *   microframe (125 µs in HS = 8 kHz capacity).
 *
 *   Wire layout per iso packet (both directions):
 *     [0..63]    PhyCMD-64 frame (CRC-validated by protocol layer)
 *     [64..511]  zero-padding (TX side fills with zeros at boot, RX side
 *                ignores). Reserved for future protocol expansion.
 *
 *   Loss handling (iso has no NAK/retry at USB level):
 *     - OUT lost  : firmware just sees no callback for that microframe
 *                   and the previous setpoints stay applied (control-hold
 *                   semantics, standard for motion-control iso links).
 *     - IN lost   : host counts gaps in seq_num and reports them as
 *                   iso_in_lost in the RT stats.
 *
 *   Both EPs are independent of the bulk path: they never touch
 *   s_tx_buf[]/s_in_busy. The bulk loop continues to work in parallel.
 * ------------------------------------------------------------------------- */

/* Double-buffered iso endpoints: keeps a pre-filled buffer always
 * ready to be picked up at the next microframe SOF.
 *
 *   In (device -> host):
 *     - On boot we pre-fill BOTH buffers with a status frame and
 *       submit [0]. When that transfer completes, the callback
 *       immediately submits [1] (already filled from the previous
 *       cycle / boot), then rebuilds [0] in the remainder of the
 *       ISR. Net effect: the SOF -> next-buffer-queued latency is
 *       just the udd_ep_run call (~1-2 µs), independent of how
 *       long `build_status_frame` takes. The worst case for a
 *       single callback running long is that build might not finish
 *       before the NEXT completion fires, not that we miss a
 *       microframe right now.
 *
 *   Out (host -> device):
 *     - Same symmetry. We always have a buffer armed to receive.
 *       `apply_command_frame` runs on the just-completed buffer
 *       after we've re-armed the other one for the next microframe.
 *
 * `s_iso_tx_active_idx` / `s_iso_rx_active_idx` are modified only
 * from ISR context so no atomic is required; they are declared
 * volatile purely so the compiler doesn't cache them across the
 * udd_ep_run call boundary.
 */
#define ISO_NBUFS 2

static COMPILER_WORD_ALIGNED uint8_t
    s_iso_tx_buf[ISO_NBUFS][UDI_VENDOR_EP_SIZE_ISO_HS];
static COMPILER_WORD_ALIGNED uint8_t
    s_iso_rx_buf[ISO_NBUFS][UDI_VENDOR_EP_SIZE_ISO_HS];

static volatile uint8_t s_iso_tx_active_idx;
static volatile uint8_t s_iso_rx_active_idx;

/* Diagnostic counters incremented on iso EP errors. Only readable via
 * the bulk path or via a future debug control request. */
static uint32_t s_iso_out_errors;
static uint32_t s_iso_in_errors;

static void vendor_iso_in_cb(udd_ep_status_t status,
                             iram_size_t n,
                             udd_ep_id_t ep);

static void vendor_iso_out_cb(udd_ep_status_t status,
                              iram_size_t n,
                              udd_ep_id_t ep);

static void vendor_iso_in_cb(udd_ep_status_t status,
                             iram_size_t n,
                             udd_ep_id_t ep)
{
	(void)n; (void)ep;

	if (status == UDD_EP_TRANSFER_ABORT) {
		/* Interface disabled — UDC freed the EP, do not re-arm. */
		return;
	}

	if (status != UDD_EP_TRANSFER_OK) {
		s_iso_in_errors++;
		/* fall through and re-arm — iso link is best-effort */
	}

	/* The completed buffer is the one we submitted last. The peer
	 * buffer has been pre-filled (at boot for the very first
	 * completion, by the previous callback after that) and is
	 * ready to go — submit it first, then build the freed buffer
	 * for the next cycle. */
	uint8_t done_idx = s_iso_tx_active_idx;
	uint8_t next_idx = done_idx ^ 1;

	(void)udd_ep_run(UDI_VENDOR_EP_ISO_IN, false,
	                 s_iso_tx_buf[next_idx], sizeof(s_iso_tx_buf[next_idx]),
	                 vendor_iso_in_cb);
	s_iso_tx_active_idx = next_idx;

	/* Refill the buffer we just freed. Padding [64..511] was
	 * zero-initialised at boot and never touched again. Any extra
	 * latency here only steals from the time budget BEFORE the
	 * next completion callback fires — it does not delay the
	 * microframe currently in flight, because that one is already
	 * queued on the USB controller. */
	build_status_frame(s_iso_tx_buf[done_idx]);
}

static void vendor_iso_out_cb(udd_ep_status_t status,
                              iram_size_t n,
                              udd_ep_id_t ep)
{
	(void)ep;

	if (status == UDD_EP_TRANSFER_ABORT) {
		return;
	}

	uint8_t done_idx = s_iso_rx_active_idx;
	uint8_t next_idx = done_idx ^ 1;

	/* Arm the NEXT receive slot first so the controller never sees
	 * a gap between completing a transfer and having a fresh
	 * descriptor to fill. */
	(void)udd_ep_run(UDI_VENDOR_EP_ISO_OUT, false,
	                 s_iso_rx_buf[next_idx], sizeof(s_iso_rx_buf[next_idx]),
	                 vendor_iso_out_cb);
	s_iso_rx_active_idx = next_idx;

	/* Only process the first 64 bytes; the rest is reserved padding
	 * ignored by this firmware revision. */
	if (status != UDD_EP_TRANSFER_OK) {
		s_iso_out_errors++;
	} else if (n >= 64) {
		apply_command_frame(s_iso_rx_buf[done_idx]);
	}
}

/* -------------------------------------------------------------------------
 *   UDI API (called by UDC)
 * ------------------------------------------------------------------------- */

static bool udi_vendor_enable(void)
{
	/* Reset ring buffer state on every interface enable (host
	 * connect or USB bus reset). If a previous host disconnected
	 * with frames still pending in the TX ring, s_tx_head /
	 * s_tx_tail / s_in_busy would otherwise stay desynchronised
	 * and the first few exchanges on the next connect would echo
	 * stale bytes — that bug cost ~30 minutes of debugging in
	 * Phase C hardware testing. Now we always start clean. */
	s_tx_head        = 0;
	s_tx_tail        = 0;
	s_in_busy        = false;
	s_dropped_frames = 0;

	s_iso_in_errors  = 0;
	s_iso_out_errors = 0;

	/* Iso TX padding bytes [64..511] must be zero on the wire. The
	 * first 64 will be overwritten by build_status_frame() on every
	 * iso IN, but the padding is set once here and never touched
	 * again. Zero both double-buffers. */
	for (unsigned k = 0; k < ISO_NBUFS; k++) {
		for (size_t i = 0; i < sizeof(s_iso_tx_buf[k]); i++) s_iso_tx_buf[k][i] = 0;
	}

	s_iso_tx_active_idx = 0;
	s_iso_rx_active_idx = 0;

	/* Arm the first BULK-OUT transfer. After this the bulk data path
	 * is self-sustaining through vendor_bulk_out_cb(). */
	if (!udd_ep_run(UDI_VENDOR_EP_OUT, true,
	                s_rx_buf, sizeof(s_rx_buf),
	                vendor_bulk_out_cb)) {
		return false;
	}

	/* Arm the iso EPs. These are best-effort: if the host never
	 * schedules iso transfers (i.e. it only uses bulk), udd_ep_run()
	 * still returns true but no callbacks will fire. If allocation
	 * fails (e.g. DPRAM exhausted), bulk continues unharmed.
	 *
	 * Pre-fill BOTH tx buffers so the first TWO iso IN packets
	 * after enumeration are meaningful (buffer [0] is the one we
	 * submit now; buffer [1] is what the first callback will flip
	 * to before it gets a chance to refill [0]). */
	build_status_frame(s_iso_tx_buf[0]);
	build_status_frame(s_iso_tx_buf[1]);

	(void)udd_ep_run(UDI_VENDOR_EP_ISO_IN, false,
	                 s_iso_tx_buf[0], sizeof(s_iso_tx_buf[0]),
	                 vendor_iso_in_cb);
	(void)udd_ep_run(UDI_VENDOR_EP_ISO_OUT, false,
	                 s_iso_rx_buf[0], sizeof(s_iso_rx_buf[0]),
	                 vendor_iso_out_cb);

	return true;
}

static void udi_vendor_disable(void)
{
	/* UDC walks the interface descriptor and frees our endpoints
	 * before calling us — any in-flight transfer has already been
	 * aborted and vendor_bulk_out_cb() will have seen
	 * UDD_EP_TRANSFER_ABORT and bailed out. The ring state will be
	 * reset the next time udi_vendor_enable() is called.
	 *
	 * Safety: on host disconnect we also stop every running waveform
	 * generator — see PROTOCOL.md §4.3. Without this a Python script
	 * crashing while DAC0 is playing a 1 kHz square wave would leave
	 * the DAC oscillating in the wild until the device is power-cycled. */
	waveform_stop_all();
}

/* Scratch for SETUP DATA-stage payloads. Sized to the largest
 * inbound payload we accept (WaveArbHeader + WAVE_MAX_ARB_SAMPLES
 * × int16_t = 8 + 2048 = 2056 bytes; round up). */
#define VENDOR_SETUP_BUF_SIZE 2056u
COMPILER_WORD_ALIGNED
static uint8_t s_setup_buf[VENDOR_SETUP_BUF_SIZE];

/* Outbound payload for IN requests (Capabilities, ChannelState).
 * 32 bytes is enough for both. */
COMPILER_WORD_ALIGNED
static uint8_t s_setup_in_buf[32];

/* Called by UDC when the host has finished sending the SETUP DATA stage. */
static void vendor_setup_out_done(void)
{
	uint8_t  bRequest = udd_g_ctrlreq.req.bRequest;
	uint16_t wIndex   = udd_g_ctrlreq.req.wIndex;
	uint16_t wLength  = udd_g_ctrlreq.req.wLength;

	bool ok = false;
	switch (bRequest) {
	case VREQ_GEN_PLAY_BUILTIN:
		ok = waveform_play_builtin(wIndex, s_setup_buf, wLength);
		break;
	case VREQ_GEN_PLAY_ARBITRARY:
		ok = waveform_play_arbitrary(wIndex, s_setup_buf, wLength);
		break;
	case VREQ_GEN_PLAY_LUT:
		ok = waveform_play_lut(wIndex, s_setup_buf, wLength);
		break;
	case VREQ_GEN_PLAY_THRESHOLD:
		ok = waveform_play_threshold(wIndex, s_setup_buf, wLength);
		break;
	case VREQ_GEN_PLAY_PULSE_TRIG:
		ok = waveform_play_pulse_trig(wIndex, s_setup_buf, wLength);
		break;
	case VREQ_GEN_PLAY_PID:
		ok = waveform_play_pid(wIndex, s_setup_buf, wLength);
		break;
	case VREQ_DAC_SET_CLOCK:
		if (wLength == 4) {
			uint32_t v;
			memcpy(&v, s_setup_buf, 4);
			ok = waveform_set_dac_clock(v);
		}
		break;
	case VREQ_ADC_SET_RATE:
		if (wLength == 4) {
			uint32_t v;
			memcpy(&v, s_setup_buf, 4);
			ok = waveform_set_adc_rate(v);
		}
		break;
	default:
		break;
	}

	if (!ok) {
		/* UDC has no public hook to STALL after-the-fact; the best we
		 * can do is silently drop the side-effect. The host's
		 * libusb_control_transfer still returns success because the
		 * STATUS stage was ACKed. To make errors more visible we
		 * could refuse the DATA stage in the SETUP callback by
		 * returning false there, but that requires knowing the
		 * payload size at SETUP time. v1: rely on host using
		 * GEN_GET_STATE to verify the side-effect landed. */
	}
}

/* Called by UDC for class-recipient SETUP requests targeting our
 * interface. We have none — return false so UDC STALLs them. */
static bool udi_vendor_setup(void)
{
	return false;
}

/* Called by UDC for vendor-recipient=device SETUP requests via the
 * USB_DEVICE_SPECIFIC_REQUEST hook in conf_usb.h. */
bool phycmd_vendor_request(void)
{
	uint8_t  bmRequestType = udd_g_ctrlreq.req.bmRequestType;
	uint8_t  bRequest      = udd_g_ctrlreq.req.bRequest;
	uint16_t wIndex        = udd_g_ctrlreq.req.wIndex;
	uint16_t wLength       = udd_g_ctrlreq.req.wLength;

	/* Only vendor type, recipient=device (the rest STALL upstream). */
	if ((bmRequestType & USB_REQ_TYPE_MASK)  != USB_REQ_TYPE_VENDOR)  return false;
	if ((bmRequestType & USB_REQ_RECIP_MASK) != USB_REQ_RECIP_DEVICE) return false;

	bool dir_in = (bmRequestType & USB_REQ_DIR_IN) != 0;

	if (dir_in) {
		/* IN requests: prepare s_setup_in_buf and let UDC stream it. */
		uint16_t reply_len = 0;
		bool ok = false;
		switch (bRequest) {
		case VREQ_GEN_GET_CAPS:
			ok = waveform_get_caps(s_setup_in_buf, sizeof(s_setup_in_buf));
			reply_len = 32;
			break;
		case VREQ_GEN_GET_STATE:
			ok = waveform_get_state(wIndex, s_setup_in_buf, sizeof(s_setup_in_buf));
			reply_len = 32;
			break;
		case VREQ_DAC_GET_CLOCK: {
			uint32_t v = waveform_get_dac_clock();
			memcpy(s_setup_in_buf, &v, 4);
			ok = true; reply_len = 4;
			break;
		}
		case VREQ_ADC_GET_RATE: {
			uint32_t v = waveform_get_adc_rate();
			memcpy(s_setup_in_buf, &v, 4);
			ok = true; reply_len = 4;
			break;
		}
		default:
			return false;        /* STALL */
		}
		if (!ok) return false;
		if (wLength > reply_len) wLength = reply_len;   /* truncate to actual */
		udd_g_ctrlreq.payload      = s_setup_in_buf;
		udd_g_ctrlreq.payload_size = wLength;
		return true;
	}

	/* OUT requests */
	switch (bRequest) {
	case VREQ_GEN_STOP:
		/* No DATA stage. Apply immediately. */
		(void)waveform_stop(wIndex);
		return true;
	case VREQ_GEN_PLAY_BUILTIN:
	case VREQ_GEN_PLAY_ARBITRARY:
	case VREQ_GEN_PLAY_LUT:
	case VREQ_GEN_PLAY_THRESHOLD:
	case VREQ_GEN_PLAY_PULSE_TRIG:
	case VREQ_GEN_PLAY_PID:
	case VREQ_DAC_SET_CLOCK:
	case VREQ_ADC_SET_RATE:
		/* Has DATA stage. Tell UDC to land it in s_setup_buf. */
		if (wLength > VENDOR_SETUP_BUF_SIZE) return false;
		udd_g_ctrlreq.payload          = s_setup_buf;
		udd_g_ctrlreq.payload_size     = wLength;
		udd_g_ctrlreq.callback         = vendor_setup_out_done;
		return true;
	default:
		return false;            /* STALL on unknown vendor OUT request */
	}
}

static uint8_t udi_vendor_getsetting(void)
{
	return 0;  /* Only alternate setting 0 exists. */
}

UDC_DESC_STORAGE udi_api_t udi_api_vendor = {
	.enable     = udi_vendor_enable,
	.disable    = udi_vendor_disable,
	.setup      = udi_vendor_setup,
	.getsetting = udi_vendor_getsetting,
	.sof_notify = NULL,
};

/* -------------------------------------------------------------------------
 *   UDC configuration glue
 * ------------------------------------------------------------------------- */

UDC_DESC_STORAGE udi_api_t *udi_apis[USB_DEVICE_NB_INTERFACE] = {
	&udi_api_vendor,
};

UDC_DESC_STORAGE udc_config_speed_t udc_config_fs[1] = { {
	.desc     = (usb_conf_desc_t UDC_DESC_STORAGE *)&udc_desc_fs,
	.udi_apis = udi_apis,
} };

#ifdef USB_DEVICE_HS_SUPPORT
UDC_DESC_STORAGE udc_config_speed_t udc_config_hs[1] = { {
	.desc     = (usb_conf_desc_t UDC_DESC_STORAGE *)&udc_desc_hs,
	.udi_apis = udi_apis,
} };
#endif

UDC_DESC_STORAGE udc_config_t udc_config = {
	.confdev_lsfs = &udc_device_desc,
	.conf_lsfs    = udc_config_fs,
#ifdef USB_DEVICE_HS_SUPPORT
	.confdev_hs   = &udc_device_desc,
	.qualifier    = &udc_device_qual,
	.conf_hs      = udc_config_hs,
#endif
};
