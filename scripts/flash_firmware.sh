#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 Angelo Leto <angelo@leto.blue>
#
# flash_firmware.sh — build and flash the ATSAM3X8E phycmd firmware
# onto an Arduino Due.
#
# ## Flow (why it matters)
#
#   1. Build `build/phycmd_fw.bin` via the Makefile.
#   2. Trigger SAM-BA mode. Two paths, tried in order:
#        a. POST /api/firmware/enter-bootloader to a running
#           physerver. The firmware handler clears GPNVM1 and writes
#           RSTC_CR = PROCRST|PERRST|EXTRST, which jumps to ROM
#           SAM-BA and re-enumerates the SAM3X UART path under the
#           ATmega16U2 (still ttyACM0). This is the JTAG-free
#           default once the firmware shipped with the
#           VREQ_FW_ENTER_BOOTLOADER 0x40 handler is on the chip.
#        b. Toggle the programming port at 1200 baud. The
#           ATmega16U2 on the Due detects a 1200-baud open/close
#           and pulses ERASE+RESET on the SAM3X. Used as fallback
#           when physerver isn't running, the API call fails, or
#           the firmware on the chip predates the new VREQ.
#   3. Stop `physerver.service` if it's running so it releases the
#      programming-port CDC exclusively. (The iso transport only
#      uses the native port, but `physerver` also opens the prog
#      port for fallback and would fight us for the ttyACM.)
#   4. Wait for the re-enumeration: the CDC reappears, still as
#      /dev/ttyACM0 but now backed by the SAM-BA ROM on the SAM3X
#      itself (not ASF CDC on application flash).
#   5. `bossac -e -w -v -b BIN` — erase, write, verify, set boot
#      bit (GPNVM1). Explicitly NOT `-R`: on SAM3X the -R reset
#      path goes through the ATmega16U2 again and either does not
#      kick the SAM3X back into application, or goes through the
#      1200-baud ERASE path and wipes the flash we just wrote.
#      See `memory/arduino_due_flash_flow.md`.
#   6. Soft-reset by writing RSTC_CR = 0xA500000D over the SAM-BA
#      protocol (key 0xA5 + PROCRST | PERRST). This makes the
#      SAM3X jump out of SAM-BA into the newly written application,
#      with all peripherals reset.
#   7. Wait for re-enumeration into the application firmware;
#      restart `physerver.service` so the iso transport picks up
#      the fresh device.
#
# ## Usage
#
#   # build + flash on this host (auto-detects best entry path)
#   sudo ./scripts/flash_firmware.sh
#
#   # build only
#   ./scripts/flash_firmware.sh --build-only
#
#   # flash a pre-built binary (skip make)
#   sudo ./scripts/flash_firmware.sh --bin path/to/phycmd_fw.bin
#
#   # force the legacy 1200-baud entry path
#   sudo ./scripts/flash_firmware.sh --entry 1200baud
#
#   # force the in-firmware HTTP entry path (fail if it doesn't work)
#   sudo ./scripts/flash_firmware.sh --entry fngen
#
# ## Recovery
#
#   If something aborts between step 3 and step 5 the Due is in
#   SAM-BA mode waiting for input; re-run the script to finish.
#   If step 5 succeeds but step 6 times out, the Due is still in
#   SAM-BA but the new firmware is present — a manual reset
#   button press will boot into it.

set -euo pipefail

FW_DIR_DEFAULT="ATSAM3X8E_FW/ATSAM3X8E_FW"
BIN=""
PORT=""
BUILD_ONLY=0
SKIP_BUILD=0
ENTRY="auto"   # auto | fngen | 1200baud
PHYSERVER_URL="${PHYSERVER_URL:-http://127.0.0.1:8080}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --bin) BIN="$2"; SKIP_BUILD=1; shift 2 ;;
        --port) PORT="$2"; shift 2 ;;
        --build-only) BUILD_ONLY=1; shift ;;
        --entry) ENTRY="$2"; shift 2 ;;
        --url) PHYSERVER_URL="$2"; shift 2 ;;
        -h|--help) sed -n '2,55p' "$0"; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 1 ;;
    esac
done

case "$ENTRY" in
    auto|fngen|1200baud) ;;
    *) echo "error: --entry must be auto|fngen|1200baud (got: $ENTRY)" >&2; exit 1 ;;
esac

# --- Locate firmware dir + binary ---
if [[ -z "$BIN" ]]; then
    # Run from repo root or inside the firmware dir; either way works.
    if [[ -d "$FW_DIR_DEFAULT" ]]; then
        FW_DIR="$FW_DIR_DEFAULT"
    elif [[ -f Makefile && -d src ]]; then
        FW_DIR="."
    else
        echo "error: cannot find firmware Makefile. Run from repo root, or pass --bin." >&2
        exit 2
    fi
    BIN="$FW_DIR/build/phycmd_fw.bin"
fi

# --- Build ---
if [[ $SKIP_BUILD -eq 0 ]]; then
    echo "=== building firmware ==="
    make -C "$FW_DIR" -j"$(nproc)"
fi

if [[ $BUILD_ONLY -eq 1 ]]; then
    echo "build-only requested; stopping after: $BIN"
    exit 0
fi

if [[ ! -f "$BIN" ]]; then
    echo "error: firmware binary not found at $BIN" >&2
    exit 3
fi

if [[ $EUID -ne 0 ]]; then
    echo "error: flashing needs sudo (writes to /dev/ttyACM*, invokes bossac)" >&2
    exit 4
fi

# --- Detect any pre-existing programming port (only used by the
#     1200-baud fallback below; the API path discovers the SAM-BA
#     CDC after the trigger fires). ---
detect_prog_port() {
    for dev in /dev/ttyACM*; do
        [[ -e "$dev" ]] || continue
        if udevadm info -q property "$dev" 2>/dev/null | grep -q 'ID_MODEL_ID=003d'; then
            echo "$dev"; return
        fi
    done
}
if [[ -z "$PORT" ]]; then
    PROG_PORT_AT_START="$(detect_prog_port)"
fi

# --- Step 2a: try the in-firmware "enter bootloader" path first ---
# We must hit the API before stopping physerver — the request goes
# through physerver to the SAM3X via vendor SETUP. The firmware
# resets mid-status-stage, so curl will see the connection reset
# (HTTP 502 from physerver, or a connection-reset libcurl exit
# code). Either of those means the trigger fired, so we treat
# any non-network-down outcome as success.
ENTRY_OK=0
if [[ "$ENTRY" == "auto" || "$ENTRY" == "fngen" ]]; then
    if command -v curl >/dev/null 2>&1; then
        echo "=== entering SAM-BA mode via /api/firmware/enter-bootloader ==="
        # Reach physerver first; if the host doesn't answer at all
        # we fall back to 1200-baud rather than hammering it.
        if curl -fsS --max-time 2 "$PHYSERVER_URL/api/health" >/dev/null 2>&1; then
            # We don't care about the response body — physerver
            # may report an upstream error because the device went
            # away mid-request. -m 5 caps total time; --no-keepalive
            # so a pending connection doesn't wedge us.
            curl -sS --max-time 5 --no-keepalive \
                -X POST "$PHYSERVER_URL/api/firmware/enter-bootloader" \
                >/dev/null 2>&1 || true
            ENTRY_OK=1
        elif [[ "$ENTRY" == "fngen" ]]; then
            echo "error: physerver not reachable at $PHYSERVER_URL (--entry fngen forced)" >&2
            exit 7
        else
            echo "physerver not reachable; falling back to 1200-baud trick"
        fi
    elif [[ "$ENTRY" == "fngen" ]]; then
        echo "error: curl not installed (--entry fngen forced)" >&2
        exit 7
    fi
fi

# --- Step 3: stop physerver so it releases the device ---
if systemctl is-active --quiet physerver 2>/dev/null; then
    echo "=== stopping physerver.service ==="
    systemctl stop physerver
    STOPPED_PHYSERVER=1
else
    STOPPED_PHYSERVER=0
fi

# --- Step 2b: legacy 1200-baud trick (fallback or forced) ---
if [[ $ENTRY_OK -eq 0 ]]; then
    if [[ -z "$PROG_PORT_AT_START" ]]; then
        echo "error: 1200-baud fallback needs the Due programming port (ID 003d) plugged in" >&2
        echo "       and exposed as /dev/ttyACM*; none was detected. Plug in the second" >&2
        echo "       USB cable on the Due (closest to the DC jack), or use --entry fngen." >&2
        exit 5
    fi
    echo "=== entering SAM-BA mode (1200-baud toggle on $PROG_PORT_AT_START) ==="
    stty -F "$PROG_PORT_AT_START" 1200 || true
    sleep 1
fi

# --- Step 4: wait for SAM-BA CDC to appear ---
# Two cases:
#   (a) API path on the native port: the SAM3X's ROM SAM-BA exposes a
#       CDC interface via 03eb:6124 on the same physical USB cable
#       physerver was using. udev assigns a new /dev/ttyACMx.
#   (b) 1200-baud trick on the programming port: the same /dev/ttyACMx
#       sticks around (the ATmega16U2 stays as ttyACM*; the SAM3X is
#       now in SAM-BA over USART through it).
# Either way: wait for a ttyACM whose udev properties point at SAM-BA
# (ID_MODEL_ID=6124 for native, ID_MODEL_ID=003d for programming).
echo "=== waiting for SAM-BA CDC (Due to re-enumerate) ==="
PORT=""
for i in {1..120}; do
    sleep 0.1
    for dev in /dev/ttyACM*; do
        [[ -e "$dev" ]] || continue
        # ID_MODEL_ID=6124  → SAM3X native port in ROM SAM-BA
        # ID_MODEL_ID=003d  → ATmega16U2 (programming port; carries SAM-BA over UART)
        if udevadm info -q property "$dev" 2>/dev/null \
              | grep -qE 'ID_MODEL_ID=(6124|003d)'; then
            PORT="$dev"; break 2
        fi
    done
done
sleep 0.8

if [[ -z "$PORT" || ! -e "$PORT" ]]; then
    echo "error: no SAM-BA CDC appeared within 12 s after trigger" >&2
    exit 6
fi
echo "=== SAM-BA port: $PORT ==="

# --- Step 5: bossac write ---
echo "=== bossac write ==="
# -e erase, -w write, -v verify, -b set GPNVM1 so the SAM3X boots
# from flash instead of the ROM bootloader. NO -R: see memory note.
bossac --port="$(basename "$PORT")" -e -w -v -b "$BIN"

# --- Step 6: RSTC_CR soft-reset via SAM-BA ---
# After bossac returns without -R, the Due is still in SAM-BA mode
# serving the CDC. SAM-BA accepts text commands terminated by '#'.
# 'W400E1A00,A500000D#' = write 0xA500000D to RSTC_CR =
#   KEY(0xA5) | PROCRST | PERRST, issuing a full CPU + peripheral
# reset which boots into the newly written flash.
echo "=== soft-reset via RSTC_CR ==="
stty -F "$PORT" 115200 raw -echo -echoe -echok -echoctl -echoke || true
# Send the command. The Due won't reply — it resets mid-write —
# so we redirect stderr and accept a broken pipe.
printf 'W400E1A00,A500000D#' > "$PORT" 2>/dev/null || true

# --- Step 7: wait for re-enumeration into application ---
# After RSTC_CR the SAM3X reboots into the freshly-written firmware
# and re-attaches as 2341:003e (vendor class). On systems with both
# USB cables plugged, the programming port (003d) also re-attaches
# alongside; we just need *one* of them to show up so physerver can
# reopen the device.
echo "=== waiting for application firmware to come back ==="
sleep 2
for i in {1..60}; do
    sleep 0.2
    if lsusb 2>/dev/null | grep -qE 'ID 2341:003[de]'; then
        break
    fi
done

if [[ $STOPPED_PHYSERVER -eq 1 ]]; then
    echo "=== restarting physerver.service ==="
    # The stale SHM segment from the old run will be unlinked by
    # IpcServer::new() (see physerver/src/ipc/mod.rs), so no manual
    # cleanup needed here.
    systemctl start physerver
fi

echo "=== done ==="
