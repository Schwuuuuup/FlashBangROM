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
  Id,
  Read,
  ProgramByte,
  SectorErase,
  ChipErase,
  WriteStatus,
};

struct CommandContext {
  CommandType cmd = CommandType::None;
  uint32_t addr = 0;
  uint32_t len = 0;
  uint8_t value = 0;
  uint32_t timeoutMs = 0;
};

struct Sst39ChipInfo {
  uint8_t manufacturer = 0x00;
  uint8_t device = 0x00;
  uint32_t sizeBytes = 0;
  const char* name = "unknown";
};
