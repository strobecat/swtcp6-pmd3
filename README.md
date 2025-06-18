# swtcp6-pmd3

Wrapper of `smoltcp`, used for userspace TUN. Current supports only TCP/IPv6.

## Installation
`pip install git+https://github.com/strobecat/swtcp6-pmd3` (requires rust toolchain)  
or pick and install a proper wheel from [GitHub Actions](https://github.com/strobecat/swtcp6-pmd3/actions/workflows/CI.yml).

## Example
### Loopback
```python
from swtcp6_pmd3 import Interface, VirtualNIC
from time import sleep

nic = VirtualNIC(1500)
intf = Interface(nic, "fe80::1", "ffff:ffff:ffff:ffff:0000:0000:0000:0000")
server_sock = intf.listen(12345)
client_sock = intf.connect("fe80::1", 12345)

client_sent = False
server_sent = False

while (delay := intf.poll_delay()) is not None:
    sleep(delay)
    intf.poll()
    if nic.can_consume_tx_buffer():
        nic.extend_rx_buffer(nic.consume_tx_buffer())

    if not client_sent and client_sock.can_send():
        client_sock.send(b"hello from client")
        client_sent = True
    if client_sock.can_recv():
        assert (recv := client_sock.recv()) == b"hello from server", recv
        print("client received:", recv)
        client_sock.close()
    if not server_sent and server_sock.can_send():
        server_sock.send(b"hello from server")
        server_sent = True
    if server_sock.can_recv():
        assert (recv := server_sock.recv()) == b"hello from client", recv
        print("server received:", recv)
```
