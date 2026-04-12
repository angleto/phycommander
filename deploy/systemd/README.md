# PhyCommander deployment — systemd services

Network and PhyCommander service units installed on the test host
(`physical`, 192.168.0.22 eno0 / 192.168.0.21 wlp2s0).

## Files

| file | dest on target | purpose |
|---|---|---|
| `phycmd-network-setup.sh` | `/usr/local/bin/` | applies source-based policy routing for dual-homed eno0/wlp2s0 (same /24) to avoid asymmetric routing |
| `phycmd-network-setup.service` | `/etc/systemd/system/` | one-shot unit that runs the script after `network-online.target` and `wifi-ensure-connection.service` |
| `phycmd-wifi-keepalive.sh` | `/usr/local/bin/` | long-running loop (30s interval) that restarts `netplan-wpa-wlp2s0` when the Realtek RTL8821CE disassociates, and re-applies policy routing after reconnect |
| `phycmd-wifi-keepalive.service` | `/etc/systemd/system/` | long-running unit wrapping the keepalive script with `Restart=always` |

## Install

```bash
sudo install -m 755 phycmd-network-setup.sh   /usr/local/bin/
sudo install -m 755 phycmd-wifi-keepalive.sh  /usr/local/bin/
sudo install -m 644 phycmd-network-setup.service   /etc/systemd/system/
sudo install -m 644 phycmd-wifi-keepalive.service  /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now phycmd-network-setup.service
sudo systemctl enable --now phycmd-wifi-keepalive.service
```

## Why this is needed

### Source-based routing (dual-homed)

The host has both `eno0` (192.168.0.22) and `wlp2s0` (192.168.0.21) on
the same 192.168.0.0/24 subnet. Without policy routing, the kernel
picks a single default route (lowest metric wins, here `eno0` at
metric 100 beats `wlp2s0` at 600) regardless of the packet's source
IP. Reply packets to clients that reached the host via WiFi get
routed out of Ethernet, causing asymmetric routing that breaks
inbound TCP/ICMP.

The fix installs two routing tables (`phywired`, `phywifi`) and `ip
rule from <ip> lookup <table>` entries so that replies leave through
the same interface the request arrived on.

### WiFi keepalive

The in-tree `rtw88_8821ce` driver for the Realtek RTL8821CE has two
known bugs:
1. Its first scan after boot fails with `CTRL-EVENT-SCAN-FAILED ret=-95`.
   Handled by the existing `wifi-ensure-connection.service` (one-shot).
2. After several hours of uptime the card sometimes disassociates
   silently. The existing one-shot doesn't help here.

The keepalive loop polls `iw dev wlp2s0 link` every 30s and, if
disconnected, restarts `netplan-wpa-wlp2s0.service` and re-applies
policy routing once a new IP lands.

## Verify

```bash
# Must show both IPs reachable from a remote host:
ping 192.168.0.22   # ethernet
ping 192.168.0.21   # wifi
ssh angelo@192.168.0.21 'hostname'
curl http://192.168.0.21:8080/api/status
```
