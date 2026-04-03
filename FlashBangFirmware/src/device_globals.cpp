#include "device_globals.h"

#include <cstring>

DeviceState g_state = DeviceState::Init;
CommandContext g_ctx;
String g_line;
uint32_t g_chipSizeBytes = 512UL * 1024UL;
bool g_dataBusIsOutput = false;
bool g_dataBusMonitorActive = false;
uint32_t g_dataBusMonitorLastSampleMs = 0;
uint32_t g_dataBusMonitorAddr = 0;
bool g_dataBusMonitorAddrSet = false;
bool g_inspectPasteActive = false;
DriverSlot g_driverSlot;

namespace {
void setSequence(DriverSlot& slot, const char* name, const char* script) {
  if (slot.sequence_count >= MAX_SEQUENCES) return;
  SequenceSlot& s = slot.sequences[slot.sequence_count];
  strncpy(s.name, name, MAX_SEQ_NAME - 1);
  s.name[MAX_SEQ_NAME - 1] = '\0';
  strncpy(s.script, script, MAX_SEQ_SCRIPT - 1);
  s.script[MAX_SEQ_SCRIPT - 1] = '\0';
  slot.sequence_count++;
}
}  // namespace

void initDriverSlotDefaults(DriverSlot& slot) {
  memset(&slot, 0, sizeof(slot));
  slot.chip_size_bytes = 512UL * 1024UL;
  slot.sector_size_bytes = 4096;
  slot.address_bits = 19;
  slot.is_default = true;

  setSequence(slot, "ID_ENTRY",     "W5555,AA;W2AAA,55;W5555,90;D10");
  setSequence(slot, "ID_READ",      "R0000>R0;R0001>R1");
  setSequence(slot, "ID_EXIT",      "W5555,AA;W2AAA,55;W5555,F0;D10");
  setSequence(slot, "PROGRAM_BYTE", "W5555,AA;W2AAA,55;W5555,A0;W$A,$D;T$A,50000");
  setSequence(slot, "PROGRAM_RANGE","{W5555,AA;W2AAA,55;W5555,A0;W$A,$D;T$A,50000}");
  setSequence(slot, "SECTOR_ERASE", "W5555,AA;W2AAA,55;W5555,80;W5555,AA;W2AAA,55;W$A,30;T$A,50000000");
  setSequence(slot, "CHIP_ERASE",   "W5555,AA;W2AAA,55;W5555,80;W5555,AA;W2AAA,55;W5555,10;T0000,250000000");
}

const char* findSequence(const DriverSlot& slot, const char* name) {
  for (uint8_t i = 0; i < slot.sequence_count; i++) {
    if (strcmp(slot.sequences[i].name, name) == 0) {
      return slot.sequences[i].script;
    }
  }
  return nullptr;
}

void resetContext() {
  g_ctx = CommandContext{};
}
