#include "driver_ops.h"

#include "command_parser.h"
#include "device_config.h"
#include "device_globals.h"
#include "hal_bus.h"
#include "protocol_io.h"
#include "seq_interpreter.h"

bool waitToggleDone(uint32_t addr, uint32_t timeoutUs) {
  const uint32_t start = micros();
  while ((micros() - start) <= timeoutUs) {
    uint8_t a = readCycle(addr);
    uint8_t b = readCycle(addr);
    bool toggling = ((a ^ b) & 0x40) != 0;
    if (!toggling) {
      uint8_t c = readCycle(addr);
      uint8_t d = readCycle(addr);
      bool stillToggling = ((c ^ d) & 0x40) != 0;
      if (!stillToggling) {
        return true;
      }
    }
    delayMicroseconds(WAIT_POLL_INTERVAL_US);
  }
  return false;
}

bool waitDq7DoneProgram(uint32_t addr, uint8_t expected, uint32_t timeoutUs) {
  const uint32_t start = micros();
  const uint8_t expectedDq7 = expected & 0x80;
  while ((micros() - start) <= timeoutUs) {
    uint8_t a = readCycle(addr) & 0x80;
    if (a == expectedDq7) {
      uint8_t b = readCycle(addr) & 0x80;
      if (b == expectedDq7) {
        return true;
      }
    }
    delayMicroseconds(WAIT_POLL_INTERVAL_US);
  }
  return false;
}

bool driverProgramByte(uint32_t addr, uint8_t value) {
  if (!validateRange(addr, 1)) {
    return false;
  }
  SeqResult r = executeNamedSequence(g_driverSlot, "PROGRAM_BYTE", addr, value);
  if (!r.ok) return false;
  delayMicroseconds(WAIT_POST_PROGRAM_STABLE_US);
  return true;
}

bool driverSectorErase(uint32_t addr) {
  uint32_t sectorBase = addr & 0xFFFFF000UL;
  if (sectorBase >= g_chipSizeBytes) {
    return false;
  }
  SeqResult r = executeNamedSequence(g_driverSlot, "SECTOR_ERASE", sectorBase, 0);
  return r.ok;
}

bool driverChipErase() {
  SeqResult r = executeNamedSequence(g_driverSlot, "CHIP_ERASE", 0, 0);
  return r.ok;
}

void executeRead(uint32_t addr, uint32_t len) {
  if (!validateRange(addr, len)) {
    sendErr("E_RANGE", "read range out of bounds");
    return;
  }

  static constexpr uint32_t CHUNK = 32;
  uint8_t buf[CHUNK];
  uint32_t offset = 0;
  while (offset < len) {
    uint32_t n = (len - offset) > CHUNK ? CHUNK : (len - offset);
    for (uint32_t i = 0; i < n; ++i) {
      buf[i] = readCycle(addr + offset + i);
    }
    sendDataFrameHex(addr + offset, buf, n);
    offset += n;
  }
  sendOk("READ", "done");
}
