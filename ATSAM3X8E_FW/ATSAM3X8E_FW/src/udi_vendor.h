/*
 * SPDX-License-Identifier: GPL-3.0-or-later
 * SPDX-FileCopyrightText: 2014-2026 Angelo Leto <angelo@leto.blue>
 */
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
 *   EP 1 IN  bulk (0x81) — device -> host status frames
 *   EP 2 OUT bulk (0x02) — host -> device command frames
 *   EP 3 IN  iso  (0x83) — device -> host status frames (HS, 8 kHz)
 *   EP 4 OUT iso  (0x04) — host -> device command frames (HS, 8 kHz)
 */
#define  UDI_VENDOR_EP_IN        (1 | USB_EP_DIR_IN)   /* 0x81 */
#define  UDI_VENDOR_EP_OUT       (2 | USB_EP_DIR_OUT)  /* 0x02 */
#define  UDI_VENDOR_EP_ISO_IN    (3 | USB_EP_DIR_IN)   /* 0x83 */
#define  UDI_VENDOR_EP_ISO_OUT   (4 | USB_EP_DIR_OUT)  /* 0x04 */

/** Interface number (first and only interface). */
#define  UDI_VENDOR_IFACE_NUMBER   0

/** Max packet size per speed (USB 2.0 requires HS bulk = 512). */
#define  UDI_VENDOR_EP_SIZE_FS         64
#define  UDI_VENDOR_EP_SIZE_HS         512

/**
 * Iso packet size — sent every microframe.
 * The first 64 bytes carry the PhyCMD-64 frame; the remaining bytes
 * are reserved for protocol expansion and TX-padded with zeros.
 *
 * Sized for SAM3X UOTGHS DPRAM (4 KB total). HS iso EPs use TRIPLE-bank
 * by default in ASF (uotghs_device.c udd_ep_alloc), so a 512 B iso EP
 * actually consumes 1536 B of DPRAM. Two iso EPs at 512 B exhausted
 * the budget and SET_CONFIGURATION returned -EPIPE.
 *
 * Final allocation (HS):
 *   EP0 ctrl   :  64 B
 *   bulk IN/OUT: 512 B × 2 banks × 2 dirs = 2048 B
 *   iso  IN/OUT: 256 B × 3 banks × 2 dirs = 1536 B
 *   sum                                   ≈ 3648 B  (fits in 4 KB)
 *
 * 256 B still gives 4× the PhyCMD-64 frame for future protocol
 * expansion (oversampled ADC, timestamps, multi-channel PWM).
 */
#define  UDI_VENDOR_EP_SIZE_ISO_HS     256
#define  UDI_VENDOR_EP_SIZE_ISO_FS     64   /* not used in practice; kept valid for FS enumeration */

/**
 * iso bInterval — encoded per USB 2.0 §9.6.6:
 *   HS:  bInterval=1 → 2^(1-1)=1 microframe → 125 µs (8 kHz capacity)
 *   FS:  bInterval=1 → 1 frame → 1 ms
 */
#define  UDI_VENDOR_EP_ISO_INTERVAL    1

/**
 * UDI API exported for UDC's interface table. Defined in udi_vendor.c.
 */
extern UDC_DESC_STORAGE udi_api_t udi_api_vendor;

#ifdef __cplusplus
}
#endif

#endif /* _UDI_VENDOR_H_ */
