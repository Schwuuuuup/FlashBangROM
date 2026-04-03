#include "driver_ops.h"

#include "command_parser.h"
#include "device_config.h"
#include "device_globals.h"
#include "hal_bus.h"
#include "protocol_io.h"
#include "seq_interpreter.h"

namespace {
bool usePageProgramForByteWrite() {
  // W29-style page devices: sector_size_bytes is used as page size (128B).
  return g_driverSlot.sector_size_bytes > 0 &&
         g_driverSlot.sector_size_bytes <= 256 &&
         findSequence(g_driverSlot, "PROGRAM_RANGE") != nullptr;
}

bool verifyReadbackStable(uint32_t addr, uint8_t expected, uint8_t* observed) {
  // Some chips may briefly return transient status-like values directly after
  // program completion. Require two consecutive expected reads in a short
  // settle window before declaring mismatch.
  static constexpr uint32_t VERIFY_WINDOW_US = 5000;
  static constexpr uint32_t VERIFY_RETRY_DELAY_US = 20;

  const uint32_t start = micros();
  uint8_t last = readCycle(addr);
  if (observed != nullptr) {
    *observed = last;
  }

  while ((micros() - start) <= VERIFY_WINDOW_US) {
    const uint8_t a = readCycle(addr);
    const uint8_t b = readCycle(addr);
    last = b;
    if (observed != nullptr) {
      *observed = last;
    }
    if (a == expected && b == expected) {
      return true;
    }
    delayMicroseconds(VERIFY_RETRY_DELAY_US);
  }

  return false;
}
}

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

bool driverProgramByte(uint32_t addr, uint8_t value, uint8_t* observed, bool* verifyMismatch) {
  if (verifyMismatch != nullptr) {
    *verifyMismatch = false;
  }
  if (!validateRange(addr, 1)) {
    return false;
  }

  if (usePageProgramForByteWrite()) {
    const uint32_t pageSize = g_driverSlot.sector_size_bytes;
    const uint32_t pageBase = addr & ~(pageSize - 1U);
    if (!validateRange(pageBase, pageSize)) {
      return false;
    }

    uint8_t pageBuf[256];
    for (uint32_t i = 0; i < pageSize; ++i) {
      pageBuf[i] = readCycle(pageBase + i);
    }
    pageBuf[addr - pageBase] = value;

    const char* rangeScript = findSequence(g_driverSlot, "PROGRAM_RANGE");
    if (rangeScript == nullptr) {
      return false;
    }
    if (!executeProgramRange(rangeScript, pageBase, pageBuf, pageSize)) {
      return false;
    }
  } else {
    SeqResult r = executeNamedSequence(g_driverSlot, "PROGRAM_BYTE", addr, value);
    if (!r.ok) return false;
  }

  delayMicroseconds(WAIT_POST_PROGRAM_STABLE_US);
  if (!verifyReadbackStable(addr, value, observed)) {
    if (verifyMismatch != nullptr) {
      *verifyMismatch = true;
    }
    return false;
  }
  return true;
}

bool driverProgramByte(uint32_t addr, uint8_t value) {
  return driverProgramByte(addr, value, nullptr, nullptr);
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
