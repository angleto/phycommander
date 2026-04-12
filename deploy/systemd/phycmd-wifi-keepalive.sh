#!/bin/bash
# Keepalive loop for the Realtek RTL8821CE WiFi driver (rtw88_8821ce).
# The driver sometimes disassociates after prolonged uptime. When this
# happens, restart netplan-wpa-wlp2s0.service to reconnect.
#
# Runs forever as a systemd service. Checks every 30s.

IFACE=wlp2s0
CHECK_INTERVAL=30

while true; do
  if ! iw dev $IFACE link 2>/dev/null | grep -q Connected; then
    echo "$(date -Iseconds) $IFACE disconnected, restarting netplan-wpa..."
    systemctl restart netplan-wpa-$IFACE.service 2>/dev/null
    sleep 20
    if iw dev $IFACE link 2>/dev/null | grep -q Connected; then
      echo "$(date -Iseconds) $IFACE reconnected"
      iw dev $IFACE set power_save off 2>/dev/null
      # Re-apply policy routing now that WiFi has an IP
      /usr/local/bin/phycmd-network-setup.sh || true
    else
      echo "$(date -Iseconds) $IFACE still disconnected after restart"
    fi
  fi
  sleep $CHECK_INTERVAL
done
