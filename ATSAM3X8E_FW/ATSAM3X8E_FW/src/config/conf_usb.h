/**
 * \file
 *
 * \brief USB configuration file for PhyCommander Vendor Class
 *        (bulk + isochronous, dual-mode).
 *
 * This replaces the original ASF UDI_CDC configuration. The firmware
 * exposes a single vendor-specific interface with FOUR endpoints:
 *   - EP 1 IN  bulk (0x81)  device -> host (64 B FS / 512 B HS)
 *   - EP 2 OUT bulk (0x02)  host   -> device (64 B FS / 512 B HS)
 *   - EP 3 IN  iso  (0x83)  device -> host (512 B HS, bInterval=1)
 *   - EP 4 OUT iso  (0x04)  host   -> device (512 B HS, bInterval=1)
 *
 * Bulk EPs preserve the original behaviour. Iso EPs were added for
 * 5–8 kHz hard-RT loops (each microframe = 125 µs guaranteed slot).
 *
 * The host side (physerver/src/transport/usb.rs) talks directly to
 * these endpoints via libusb. No CDC/ACM kernel driver is involved.
 */

#ifndef _CONF_USB_H_
#define _CONF_USB_H_

#include "compiler.h"

/**
 * USB Device identification
 */
#define  USB_DEVICE_VENDOR_ID             0x2341  /* Arduino */
#define  USB_DEVICE_PRODUCT_ID            0x003E  /* PhysicalCommander (vendor bulk) */
#define  USB_DEVICE_MAJOR_VERSION         1
#define  USB_DEVICE_MINOR_VERSION         0
#define  USB_DEVICE_POWER                 500 /* mA on Vbus */
#define  USB_DEVICE_ATTR                  (USB_CONFIG_ATTR_SELF_POWERED)

/* String descriptors (UDC builds them from these defines) */
#define  USB_DEVICE_MANUFACTURE_NAME      "Ephemeralbit"
#define  USB_DEVICE_PRODUCT_NAME          "PhysicalCommander"
#define  USB_DEVICE_SERIAL_NAME           "EB000001"

/* Enable high-speed (480 Mbps) support on UOTGHS */
#define  USB_DEVICE_HS_SUPPORT

/**
 * USB Device low-level configuration (for UDC / UDD / UOTGHS driver)
 *
 *   USB_DEVICE_EP_CTRL_SIZE  = control EP0 max packet size
 *   USB_DEVICE_MAX_EP        = number of non-control endpoints used
 *                               (EP 1 IN bulk + EP 2 OUT bulk +
 *                                EP 3 IN iso  + EP 4 OUT iso  -> 4)
 */
#define  USB_DEVICE_EP_CTRL_SIZE          64
#define  USB_DEVICE_NB_INTERFACE          1
#define  USB_DEVICE_MAX_EP                4

/**
 * USB Device Callbacks definitions (Optional, unused here)
 */
/* #define  UDC_VBUS_EVENT(b_vbus_high)      user_callback_vbus_action(b_vbus_high) */
/* #define  UDC_SOF_EVENT()                  user_callback_sof_action() */
/* #define  UDC_SUSPEND_EVENT()              user_callback_suspend_action() */
/* #define  UDC_RESUME_EVENT()               user_callback_resume_action() */

/* NB: udi_vendor.h is intentionally NOT included here to avoid a
 * circular include — conf_usb.h is processed while udc_desc.h has
 * not yet been seen, so UDC_DESC_STORAGE is still undefined. The
 * vendor header is pulled in directly by udi_vendor.c and main.c
 * (after asf.h / udc.h bring in the UDC macros). */

#endif /* _CONF_USB_H_ */
