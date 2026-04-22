#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 Angelo Leto <angelo@leto.blue>
#
# pin_usb_irq.sh — pin the IRQ carrying the Arduino Due traffic to
# an isolated CPU core. Intended for PREEMPT_RT hosts booted with
# isolcpus=2,3 (or similar). Dramatically reduces the tail of the
# scheduler jitter histogram by keeping USB completion work off the
# core that runs physerver's hot loop.
#
# Usage:
#   sudo ./pin_usb_irq.sh           # auto-detect IRQ, pin to core 3
#   sudo ./pin_usb_irq.sh 3         # pin auto-detected IRQ to core 3
#   sudo ./pin_usb_irq.sh 3 23      # pin IRQ 23 to core 3
#
# Rationale for defaults:
#   - Core 3 by default (opposite of physerver's scheduler core 2)
#     so the IRQ handler and the RT scheduler don't contend.
#   - Auto-detection picks the IRQ matching "ehci_hcd|xhci_hcd" with
#     the highest interrupt count across all CPUs — the busiest USB
#     controller is almost always the one the Due lives on.

set -euo pipefail

CORE="${1:-3}"
IRQ="${2:-}"

if [[ $EUID -ne 0 ]]; then
    echo "error: must run as root (writes to /proc/irq/*/smp_affinity)" >&2
    exit 1
fi

if [[ -z "$IRQ" ]]; then
    # Auto-detect: pick the ehci/xhci line with the highest sum across
    # all CPU columns. awk walks all /proc/interrupts rows.
    IRQ=$(awk '
        /ehci_hcd|xhci_hcd/ {
            sum = 0
            for (i = 2; i <= NF; i++) if ($i ~ /^[0-9]+$/) sum += $i
            # strip trailing colon from IRQ id
            gsub(":", "", $1)
            if (sum > best_sum) { best_sum = sum; best_irq = $1 }
        }
        END { print best_irq }
    ' /proc/interrupts)

    if [[ -z "$IRQ" ]]; then
        echo "error: no ehci_hcd or xhci_hcd IRQ found in /proc/interrupts" >&2
        echo "       either the Due isn't connected or its IRQ uses a different name." >&2
        exit 2
    fi

    echo "auto-detected IRQ $IRQ as the busiest USB host controller"
fi

AFFINITY_FILE="/proc/irq/${IRQ}/smp_affinity_list"
if [[ ! -w "$AFFINITY_FILE" ]]; then
    echo "error: cannot write $AFFINITY_FILE (does IRQ $IRQ exist?)" >&2
    exit 3
fi

PREV=$(cat "$AFFINITY_FILE")
echo "$CORE" > "$AFFINITY_FILE"
NEW=$(cat "$AFFINITY_FILE")

echo "IRQ $IRQ: affinity ${PREV} -> ${NEW} (requested core $CORE)"

# Sanity: warn if irqbalance is running — it will fight us.
if systemctl is-active --quiet irqbalance 2>/dev/null; then
    echo "warning: irqbalance is active and will periodically reset the affinity." >&2
    echo "         consider: sudo systemctl disable --now irqbalance" >&2
fi
