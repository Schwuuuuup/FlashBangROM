// SST39SF040 low-level driver for the RP2350B.
//
// Implements the parallel-bus protocol described in
// Resources/ExternalDocs/SST39SF_Protocol_fuer_KI.txt:
//   - byte read / byte program (with SDP unlock)
//   - sector erase / chip erase
//   - software product ID read
//   - toggle-bit (DQ6) end-of-write polling
#pragma once

#include <stdint.h>

namespace sst39 {

// Geometry of the SST39SF040.
static const uint32_t kChipSize = 512UL * 1024UL;  // 512 KByte
static const uint32_t kSectorSize = 0x1000UL;      // 4 KByte
static const uint32_t kSectorCount = kChipSize / kSectorSize;  // 128

// Expected software product IDs.
static const uint8_t kManufacturerSST = 0xBF;
static const uint8_t kDeviceSF040 = 0xB7;

// Configure GPIOs and bring the chip into a defined read/idle state.
void begin();

// Single byte read from the chip (chip must be in read/idle mode).
uint8_t readByte(uint32_t addr);

// Read a contiguous range into buf. buf must hold at least len bytes.
void readRange(uint32_t addr, uint8_t* buf, uint32_t len);

// Set an extra settle delay (microseconds) applied after every WE#-controlled
// write cycle (program, erase, ID, reset). 0 disables it (default). Useful
// for chips/boards that need more margin than the SST39SF040 default timing.
void setWriteDelayUs(uint32_t us);

// Current extra write delay in microseconds (see setWriteDelayUs()).
uint32_t writeDelayUs();

// Wait until DQ6 no longer toggles. Use this before starting a new write
// sequence when a previous program/erase operation may still be active.
bool waitReady(uint32_t addr);

// Program a single byte. The target cell must already be erased (0xFF).
// Waits for an idle chip first and returns true when programming completes.
bool programByte(uint32_t addr, uint8_t data);

// Program a range from buf. Bytes equal to 0xFF are skipped (no-op on an
// erased cell). Returns the number of bytes that failed to program.
uint32_t programRange(uint32_t addr, const uint8_t* buf, uint32_t len);

// Erase the 4 KByte sector that contains addr. Returns true only after the
// complete sector has been read back as 0xFF.
bool eraseSector(uint32_t addr);

// Erase the whole chip (all bytes -> 0xFF). Returns true on success.
bool eraseChip();

// Read manufacturer and device ID via the software product-ID sequence.
void readId(uint8_t* manufacturer, uint8_t* device);

}  // namespace sst39
