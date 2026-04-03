#include "chip_probe.h"

#include "device_config.h"
#include "device_globals.h"
#include "hal_bus.h"
#include "seq_interpreter.h"

namespace {

ChipInfo probeSst39() {
  ChipInfo info{};

  // Use sequence interpreter for ID entry/read/exit
  SeqResult rEntry = executeNamedSequence(g_driverSlot, "ID_ENTRY", 0, 0);
  if (!rEntry.ok) return info;

  SeqResult rRead = executeNamedSequence(g_driverSlot, "ID_READ", 0, 0);
  if (!rRead.ok) {
    executeNamedSequence(g_driverSlot, "ID_EXIT", 0, 0);
    return info;
  }

  info.manufacturer = rRead.r0;
  info.device = rRead.r1;

  executeNamedSequence(g_driverSlot, "ID_EXIT", 0, 0);

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

}  // namespace

ChipInfo probeChipInfo() {
  ChipInfo info = probeSst39();
  return info;
}

bool hasSupportedSst39(ChipInfo info) {
  return info.manufacturer == 0xBF &&
         (info.device == 0xB5 || info.device == 0xB6 || info.device == 0xB7);
}
