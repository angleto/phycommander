#!/bin/bash
# Source-based policy routing for dual-homed eno0 + wlp2s0 on the same /24.
# Without this, reply packets exit the wrong interface (kernel picks the
# route with the lowest metric regardless of source IP), producing
# asymmetric routing that breaks inbound TCP/ICMP on the secondary NIC.

# NOTE: no `set -e` - we intentionally ignore "rule not found" failures
# on the flush step, which happens on the first invocation.

# Register custom routing tables (idempotent)
grep -q "^100 phywifi"  /etc/iproute2/rt_tables || echo "100 phywifi"  >> /etc/iproute2/rt_tables
grep -q "^200 phywired" /etc/iproute2/rt_tables || echo "200 phywired" >> /etc/iproute2/rt_tables

ETH_IF=eno0
WIFI_IF=wlp2s0
GW=192.168.0.1
SUBNET=192.168.0.0/24

# Wait up to 20s for interfaces to get IPs
for i in $(seq 1 10); do
  ETH_IP=$(ip -4 -o addr show $ETH_IF 2>/dev/null | awk "{print \$4}" | cut -d/ -f1 | head -1)
  WIFI_IP=$(ip -4 -o addr show $WIFI_IF 2>/dev/null | awk "{print \$4}" | cut -d/ -f1 | head -1)
  if [ -n "$ETH_IP" ] || [ -n "$WIFI_IP" ]; then break; fi
  sleep 2
done

apply_table() {
  local ip=$1 iface=$2 table=$3
  if [ -z "$ip" ]; then
    echo "no IP on $iface, skipping table $table"
    return 0
  fi
  # Flush existing rules/routes for this table (ignore errors)
  while ip rule del table $table 2>/dev/null; do :; done
  ip route flush table $table 2>/dev/null
  # Add routes
  ip route add $SUBNET dev $iface src $ip table $table
  ip route add default via $GW dev $iface src $ip table $table
  # Add rule: traffic sourced from this IP uses this table
  ip rule add from $ip table $table
  echo "applied: $ip via $iface in table $table"
}

apply_table "$ETH_IP"  "$ETH_IF"  phywired
apply_table "$WIFI_IP" "$WIFI_IF" phywifi

echo "done. eth=$ETH_IP wifi=$WIFI_IP"
exit 0
