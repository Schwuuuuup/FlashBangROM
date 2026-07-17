// Minimal streaming XMODEM implementation over the USB-CDC serial link.
//
// Data is streamed via callbacks so no large RAM buffer is required:
//   - send    : device -> PC   (terminal "receive file")
//               pulls each byte from `src` on demand (CRC-16 or checksum,
//               auto-detected from the receiver's first poll byte).
//   - receive : PC -> device   (terminal "send file"), CRC-16 mode.
//               pushes each received byte to `sink` before ACKing its block.
//
// 128-byte blocks. The final block is padded with 0x1A (SUB) on send; on
// receive only the first `len` bytes are delivered and padding is discarded.
#pragma once

#include <stdint.h>

namespace xmodem {

// Returns the byte at logical position `index` (0..len-1).
typedef uint8_t (*ByteSource)(uint32_t index);

// Consumes the byte at logical position `index` (0..len-1).
typedef void (*ByteSink)(uint32_t index, uint8_t value);

// Send exactly len bytes pulled from src. Returns true when fully ACKed.
bool send(uint32_t len, ByteSource src);

// Receive up to len bytes and hand them to sink. Extra padding beyond len is
// accepted and discarded. Returns true on a clean EOT.
bool receive(uint32_t len, ByteSink sink);

}  // namespace xmodem
