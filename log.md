# Log

What I've done I might want to remember later.

## Setup BLE sniffer following <https://learn.adafruit.com/introducing-the-adafruit-bluefruit-le-sniffer/using-with-sniffer-v2-and-python3>

Need to also run `sudo modprobe cp210x` and `sudo modprobe usbserial`.

To get it working without superuser, identify the idVendor and idProduct of the device.

```
> lsusb -v
...
  idVendor           0x10c4 Silicon Labs
  idProduct          0xea60 CP210x UART Bridge
...
```

Write the following to `/etc/udev/rules.d/52-ble-sniffer.rules`

```
SUBSYSTEMS=="usb", ATTRS{idVendor}=="0x10c4", ATTRS{idProduct}=="0xea60", MODE="777"
```

Configure wireshark to not require root

```
> sudo dpkg-reconfigure wireshark-common
> sudo chmod +x /usr/bin/dumpcap
```

Finally, reboot.
