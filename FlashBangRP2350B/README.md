# FlashBangRP2350B Firmware

Self-contained SST39SF040 programmer for a custom **RP2350B** board. The only
client you need is a **serial terminal**. Binary transfers use **XMODEM-CRC**,
which every common terminal supports (minicom, tio, PuTTY, TeraTerm, …).

`download`/`upload` stream directly between the chip and the serial link (a
128-byte block buffer), so no large RAM buffer is required. A full 512 KByte
RAM image does **not** fit next to the Arduino-Pico USB/network stack (it
overflows SRAM by ~11 KB), so streaming is used instead — it needs almost no
RAM and supports `download all` over the whole chip.

## Hardware

RP2350B (48-GPIO package) wired directly to an SST39SF040 (512 KByte). The pin
mapping lives in [Docs/PinConnections.md](Docs/PinConnections.md) and is
mirrored 1:1 in [src/pins.h](src/pins.h).

> A18 is on GPIO25, which also drives the on-board LED via a 5K6 resistor. This
> is intentional/experimental — see the note in `PinConnections.md`.

## Build & flash

```sh
cd FlashBangRP2350B
pio run                 # build
pio run -t upload       # flash (BOOTSEL/UF2)
pio device monitor      # open the serial console
```

The firmware is deliberately lean on RAM: it streams chip data through a small
128-byte block buffer during download/upload, so it runs comfortably on the
RP2350B and there is no image-size limit beyond the 512 KByte chip itself.

## Commands

Type `help` in the console. Numbers accept `0x` hex or decimal; addresses are
byte addresses in the 512 KByte space.

| Command | Description |
| --- | --- |
| `help` | show the command reference |
| `id` | read manufacturer/device ID (expect `0xBF` / `0xB7`) |
| `read <start> <len>` | hex-dump a range to the terminal |
| `write <start> <len> <val>` | program a range with a constant byte |
| `erase sector <n>` | erase 4 KByte sector `n` (0..127) |
| `erase addr <addr>` | erase the sector containing `addr` |
| `erase all` | chip erase (whole 512 KByte) |
| `download all` | XMODEM-send the whole chip to the PC |
| `download <start> <len>` | XMODEM-send a range to the PC |
| `upload <start> <len>` | XMODEM-receive a file and program the range |

### Important: erase before programming

Flash can only clear bits (`1 → 0`). `write` and `upload` do **not** auto-erase.
Erase the target sectors first, then program. Both commands run a verify pass
afterwards and report any cell that did not match (a common symptom of a
forgotten erase).

## Typical workflows

**Dump the whole chip to a file (`tio`):**

```
> download all
Start XMODEM *receive* in your terminal now.
```
Then trigger an XMODEM receive in your terminal (`tio`: press `Ctrl-t y`).

**Program a file into the chip:**

```
> erase all
> upload 0x0 0x80000
Start XMODEM *send* of your file in your terminal now.
```
Then trigger an XMODEM send of your image (`tio`: `Ctrl-t s`).

## Source layout

- [src/pins.h](src/pins.h) — GPIO ↔ SST39 pin mapping
- [src/sst39.h](src/sst39.h) / [src/sst39.cpp](src/sst39.cpp) — chip driver (bus cycles, program/erase/ID, DQ6 polling)
- [src/xmodem.h](src/xmodem.h) / [src/xmodem.cpp](src/xmodem.cpp) — XMODEM-CRC send/receive
- [src/main.cpp](src/main.cpp) — serial CLI, 512 KByte RAM image buffer
