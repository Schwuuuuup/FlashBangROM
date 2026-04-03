#include "sst39_ops.h"

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

bool sst39ProgramByte(uint32_t addr, uint8_t value) {
  if (!validateRange(addr, 1)) {
    return false;
  }
  SeqResult r = executeNamedSequence(g_driverSlot, "PROGRAM_BYTE", addr, value);
  if (!r.ok) return false;
  delayMicroseconds(WAIT_POST_PROGRAM_STABLE_US);
  return true;
}

bool sst39SectorErase(uint32_t addr) {
  uint32_t sectorBase = addr & 0xFFFFF000UL;
  if (sectorBase >= g_chipSizeBytes) {
    return false;
  }
  SeqResult r = executeNamedSequence(g_driverSlot, "SECTOR_ERASE", sectorBase, 0);
  return r.ok;
}

bool sst39ChipErase() {
  SeqResult r = executeNamedSequence(g_driverSlot, "CHIP_ERASE", 0, 0);
  return r.ok;
}

Sst39ChipInfo sst39ReadId() {
  Sst39ChipInfo info;
  SeqResult rEntry = executeNamedSequence(g_driverSlot, "ID_ENTRY", 0, 0);
  if (!rEntry.ok) return info;

  SeqResult rRead = executeNamedSequence(g_driverSlot, "ID_READ", 0, 0);
  info.manufacturer = rRead.r0;
  info.device = rRead.r1;

  executeNamedSequence(g_driverSlot, "ID_EXIT", 0, 0);

  if (info.manufacturer == 0xBF && info.device == 0xB5) {
    info.name = "SST39SF010A";
    info.sizeBytes = 128UL * 1024UL;
  } else if (info.manufacturer == 0xBF && info.device == 0xB6) {
    info.name = "SST39SF020A";
    info.sizeBytes = 256UL * 1024UL;
  } else if (info.manufacturer == 0xBF && info.device == 0xB7) {
    info.name = "SST39SF040";
    info.sizeBytes = 512UL * 1024UL;
  } else {
    info.name = "unknown";
    info.sizeBytes = 0;
  }

  if (info.sizeBytes > 0) {
    g_chipSizeBytes = info.sizeBytes;
  }
  return info;
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
