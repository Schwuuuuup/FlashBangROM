#include "chip_probe.h"

#include "device_config.h"
#include "device_globals.h"
#include "hal_bus.h"
#include "seq_interpreter.h"

namespace {

ChipInfo probeByLoadedDriver() {
  ChipInfo info{};

  // Use currently loaded ID sequences without family-specific mapping.
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
  return info;
}

}  // namespace

ChipInfo probeChipInfo() {
  return probeByLoadedDriver();
}
