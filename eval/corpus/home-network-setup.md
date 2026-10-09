# Home network notes

The fibre modem is in bridge mode; the router behind it does NAT, DHCP and the firewall. Three mesh nodes cover the flat: living room (wired backhaul), office and bedroom.

Smart home devices live on a separate VLAN and wireless network (IoT) that cannot reach the laptops. The guest network is isolated too.

DNS goes to a Pi-hole on the Raspberry Pi (192.168.1.2), which blocks ads and trackers for every device, with Quad9 as upstream. If the internet seems down, first check whether the Pi-hole is running before restarting the router. The router admin password is in the password manager under "router".
