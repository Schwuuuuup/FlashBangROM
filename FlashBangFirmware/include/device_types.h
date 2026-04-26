#pragma once

#include <Arduino.h>

enum class DeviceState {
  Init,
  Idle,
  Parse,
  Execute,
  Error,
};

enum class CommandType {
  None,
  Help,
  Flush,
  Hello,
  Id,
  Read,
  ProgramByte,
  ProgramRange,
  SectorErase,
  ChipErase,
  WriteStatus,
  DataBusMonitorStart,
  DataBusMonitorStop,
  SetAddress,
  AddrBusTest,
  Sequence,
  Parameter,
  Inspect,
  DriverReset,
  CustomSequence,
};

struct CommandContext {
  CommandType cmd = CommandType::None;
  uint32_t addr = 0;
  uint32_t len = 0;
  uint8_t value = 0;
  uint32_t timeoutMs = 0;
  uint8_t bank = 0;
  bool shortForm = false;
};

struct ChipInfo {
  uint8_t manufacturer = 0x00;
  uint8_t device = 0x00;
  uint32_t sizeBytes = 0;
  const char* name = "unknown";
  const char* driverId = "unknown";
};

// --- DriverSlot: dynamic driver storage in RAM ---

static constexpr uint8_t MAX_SEQUENCES = 12;
static constexpr uint8_t MAX_SEQ_NAME = 20;
static constexpr uint8_t MAX_SEQ_SCRIPT = 96;
static constexpr uint8_t MAX_CUSTOM_PARAMS = 8;
static constexpr uint8_t MAX_PARAM_NAME = 32;

struct SequenceSlot {
  char name[MAX_SEQ_NAME];
  char script[MAX_SEQ_SCRIPT];
};

struct CustomParam {
  char name[MAX_PARAM_NAME];
  uint32_t value;
};

struct DriverSlot {
  SequenceSlot sequences[MAX_SEQUENCES];
  uint8_t sequence_count;

  // Built-in parameters (UPPERCASE keys)
  uint32_t chip_size_bytes;
  uint32_t sector_size_bytes;
  uint8_t  address_bits;

  // Custom parameters (lowercase keys, referenced as $0..$7 in sequences)
  CustomParam custom_params[MAX_CUSTOM_PARAMS];
  uint8_t custom_param_count;

  bool is_default;
};

// Result of sequence execution
struct SeqResult {
  bool ok;
  uint8_t r0;
  uint8_t r1;
};

void initDriverSlotDefaults(DriverSlot& slot);
const char* findSequence(const DriverSlot& slot, const char* name);
