/**
 * \file
 *
 * \brief PhyCommander Vendor Class bulk interface — public header
 *
 * Declarations shared between conf_usb.h / udi_vendor.c / main.c.
 *
 * Step 1 of the L1 migration (see docs/reports/FIRMWARE_HIGH_RATE_INVESTIGATION.md):
 * this file replaces udi_cdc_conf.h / udi_cdc.h.
 */

#ifndef _UDI_VENDOR_H_
#define _UDI_VENDOR_H_

#include "conf_usb.h"
#include "usb_protocol.h"
#include "udd.h"
#include "udc_desc.h"
#include "udi.h"

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Endpoint addresses. Each value contains the USB direction bit in bit 7.
 *
 * SAM3X UOTGHS hardware endpoints are uni-directional, so IN and OUT
 * must use different endpoint *numbers* (not just different direction bits).
 *
 *   EP 1 IN  (0x81) — device -> host (status frames)
 *   EP 2 OUT (0x02) — host -> device (command frames)
 */
#define  UDI_VENDOR_EP_IN    (1 | USB_EP_DIR_IN)   /* 0x81 */
#define  UDI_VENDOR_EP_OUT   (2 | USB_EP_DIR_OUT)  /* 0x02 */

/** Interface number (first and only interface). */
#define  UDI_VENDOR_IFACE_NUMBER   0

/** Max packet size per speed (USB 2.0 requires HS bulk = 512). */
#define  UDI_VENDOR_EP_SIZE_FS    64
#define  UDI_VENDOR_EP_SIZE_HS    512

/**
 * UDI API exported for UDC's interface table. Defined in udi_vendor.c.
 */
extern UDC_DESC_STORAGE udi_api_t udi_api_vendor;

#ifdef __cplusplus
}
#endif

#endif /* _UDI_VENDOR_H_ */
