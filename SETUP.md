# Setup

## Wire STLINK to board

Connect SWDIO (orange wire on adapter, red wire on board) and SWCLK (white wires). Connect ground, and connect 5v to BAT.

## Identify STLINK device info

I believe this is always the same. Here is how I found it.

```
> lsusb | grep ST-LINK
Bus 001 Device 005: ID 0483:3748 STMicroelectronics ST-LINK/V2

> lsusb -s 001:005 -v
...
  idVendor           0x0483 STMicroelectronics
  idProduct          0x3748 ST-LINK/V2
...
```

I believe udev wants the ids without the leading `0x`.

Create the file `/etc/udev/rules.d/99-stlink.rules` with

```
SUBSYSTEMS=="usb", ATTRS{idVendor}=="0483", ATTRS{idProduct}=="3748", MODE="777"

```

## Install dependencies

- `sudo apt-get install gcc-arm-none-eabi`
- `cargo install probe-run`
- `cargo install cargo-flash`
- `cargo install cargo-binutils && rustup component add llvm-tools-preview`
- `cargo install flip-link`

