#include "chip_probe.h"

#include "device_config.h"
#include "hal_bus.h"
#include "sst39_ops.h"

namespace {
static constexpr uint32_t SST_ID_TIMEOUT_US = 50000UL;

inline void writeRawWithCeLow(uint32_t addr, uint8_t data) {
  busSetAddress(addr);
  busSetData(data);
  writeEnablePulse();
}

ChipInfo probeSst39() {
  ChipInfo info{};
  // Legacy behavior: hold CE active for full ID transaction.
  outputEnable(false);
  chipSelect(true);
  setDataBusMode(true);

  writeRawWithCeLow(0x5555, 0xAA);
  writeRawWithCeLow(0x2AAA, 0x55);
  writeRawWithCeLow(0x5555, 0x90);

  setDataBusMode(false);
  busSetAddress(0x0000);
  outputEnable(true);

  info.manufacturer = 0x00;
  const uint32_t start = micros();
  while ((micros() - start) <= SST_ID_TIMEOUT_US) {
    info.manufacturer = busReadData();
    if (info.manufacturer == 0xBF) {
      break;
    }
  }

  // Device ID is read from 0x0001 directly after mf read, no extra toggles.
  busSetAddress(0x0001);
  info.device = busReadData();

  outputEnable(false);

  setDataBusMode(true);
  writeRawWithCeLow(0x5555, 0xAA);
  writeRawWithCeLow(0x2AAA, 0x55);
  writeRawWithCeLow(0x5555, 0xF0);
  delayMicroseconds(WAIT_ID_MODE_US);

  chipSelect(false);
  outputEnable(false);

  if (info.manufacturer == 0xBF && info.device == 0xB5) {
    info.name = "SST39SF010A";
    info.sizeBytes = 128UL * 1024UL;
    info.driverId = "sst39-core";
  } else if (info.manufacturer == 0xBF && info.device == 0xB6) {
    info.name = "SST39SF020A";
    info.sizeBytes = 256UL * 1024UL;
    info.driverId = "sst39-core";
  } else if (info.manufacturer == 0xBF && info.device == 0xB7) {
    info.name = "SST39SF040";
    info.sizeBytes = 512UL * 1024UL;
    info.driverId = "sst39-core";
  }
  return info;
}

bool isKnown(const ChipInfo& info) { return info.sizeBytes > 0; }
}  // namespace

ChipInfo probeChipInfo() {
  ChipInfo info = probeSst39();
  return info;
}

bool hasSupportedSst39(ChipInfo info) {
  return info.manufacturer == 0xBF &&
         (info.device == 0xB5 || info.device == 0xB6 || info.device == 0xB7);
}
