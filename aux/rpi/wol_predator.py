#!/usr/bin/env python3
"""Send a Wake-on-LAN magic packet to predator. Runs daily from cron (07:00)."""

import socket

MAC = "98:29:a6:3a:a7:50"
BROADCAST = "192.168.1.255"
PORT = 9

mac_bytes = bytes(int(b, 16) for b in MAC.split(":"))
packet = b"\xff" * 6 + mac_bytes * 16

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
sock.sendto(packet, (BROADCAST, PORT))
print(f"WOL sent to {MAC} via {BROADCAST}:{PORT}")
