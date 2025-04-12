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

/* Defined in main.c — processes a PhyCMD-64 command frame and fills
 * the 64-byte status response.  Called from ISR context. */
extern void process_command_frame(const uint8_t *rx_buf, uint8_t *tx_buf);

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
 *   Configuration descriptor (interface + 2 bulk endpoints)
 *
 *   Layout:
 *     [9] usb_conf_desc_t
 *     [9] usb_iface_desc_t
 *     [7] usb_ep_desc_t (EP IN, 0x81)
 *     [7] usb_ep_desc_t (EP OUT, 0x02)
 *   Total = 32 bytes
 * ------------------------------------------------------------------------- */

COMPILER_PACK_SET(1)
typedef struct {
	usb_conf_desc_t   conf;
	usb_iface_desc_t  iface;
	usb_ep_desc_t     ep_in;
	usb_ep_desc_t     ep_out;
} udi_vendor_desc_t;
COMPILER_PACK_RESET()

/**
 * \brief Build one configuration descriptor at compile time for a given
 * bulk endpoint size. Called twice: once for FS (64) and once for HS (512).
 */
#define UDI_VENDOR_DESC_INIT(ep_size)                                    \
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
		.bNumEndpoints      = 2,                                 \
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
		.wMaxPacketSize   = LE16(ep_size),                       \
		.bInterval        = 0,                                   \
	},                                                               \
	.ep_out = {                                                      \
		.bLength          = sizeof(usb_ep_desc_t),               \
		.bDescriptorType  = USB_DT_ENDPOINT,                     \
		.bEndpointAddress = UDI_VENDOR_EP_OUT,                   \
		.bmAttributes     = USB_EP_TYPE_BULK,                    \
		.wMaxPacketSize   = LE16(ep_size),                       \
		.bInterval        = 0,                                   \
	},                                                               \
}

COMPILER_WORD_ALIGNED
UDC_DESC_STORAGE udi_vendor_desc_t udc_desc_fs =
	UDI_VENDOR_DESC_INIT(UDI_VENDOR_EP_SIZE_FS);

#ifdef USB_DEVICE_HS_SUPPORT
COMPILER_WORD_ALIGNED
UDC_DESC_STORAGE udi_vendor_desc_t udc_desc_hs =
	UDI_VENDOR_DESC_INIT(UDI_VENDOR_EP_SIZE_HS);
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
 *   missing response and time out on that slot. With NUM_TX_SLOTS=16
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

	/* Arm the first BULK-OUT transfer. After this the data path is
	 * self-sustaining through vendor_bulk_out_cb(). */
	if (!udd_ep_run(UDI_VENDOR_EP_OUT, true,
	                s_rx_buf, sizeof(s_rx_buf),
	                vendor_bulk_out_cb)) {
		return false;
	}
	return true;
}

static void udi_vendor_disable(void)
{
	/* UDC walks the interface descriptor and frees our endpoints
	 * before calling us — any in-flight transfer has already been
	 * aborted and vendor_bulk_out_cb() will have seen
	 * UDD_EP_TRANSFER_ABORT and bailed out. The ring state will be
	 * reset the next time udi_vendor_enable() is called. */
}

static bool udi_vendor_setup(void)
{
	/* No vendor-specific control requests supported yet. Return false
	 * so UDC issues the default STALL on unknown class/vendor requests. */
	return false;
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
