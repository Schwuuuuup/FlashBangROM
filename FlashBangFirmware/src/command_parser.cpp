#include "command_parser.h"

#include <cstdlib>

#include "device_globals.h"

String g_rawLine;

bool parseHex32(const String& text, uint32_t& out) {
  String normalized = text;
  normalized.trim();
  char* endPtr = nullptr;
  unsigned long parsed = strtoul(normalized.c_str(), &endPtr, 16);
  if (endPtr == nullptr || *endPtr != '\0') {
    return false;
  }
  out = static_cast<uint32_t>(parsed);
  return true;
}

bool parseDec32(const String& text, uint32_t& out) {
  String normalized = text;
  normalized.trim();
  char* endPtr = nullptr;
  unsigned long parsed = strtoul(normalized.c_str(), &endPtr, 10);
  if (endPtr == nullptr || *endPtr != '\0') {
    return false;
  }
  out = static_cast<uint32_t>(parsed);
  return true;
}

bool parseLine(const String& line, CommandContext& ctx) {
  String trimmed = line;
  trimmed.trim();

  if (trimmed.length() == 0) {
    return false;
  }

  // Save raw line for commands that need case-sensitive input
  g_rawLine = trimmed;

  // Lowercase first char → custom sequence dispatch (bit 5 check)
  char first = trimmed.charAt(0);
  if (first >= 'a' && first <= 'z') {
    ctx.cmd = CommandType::CustomSequence;
    return true;
  }

  // Normalize to uppercase for built-in command matching
  String normalized = trimmed;
  normalized.toUpperCase();

  if (normalized == "?") {
    ctx.cmd = CommandType::Help;
    return true;
  }
  if (normalized == "HELLO" || normalized.startsWith("HELLO|")) {
    ctx.cmd = CommandType::Hello;
    return true;
  }
  if (normalized == "ID") {
    ctx.cmd = CommandType::Id;
    return true;
  }
  if (normalized == "CHIP_ERASE") {
    ctx.cmd = CommandType::ChipErase;
    return true;
  }
  if (normalized == "INSPECT") {
    ctx.cmd = CommandType::Inspect;
    return true;
  }
  if (normalized == "DRIVER_RESET") {
    ctx.cmd = CommandType::DriverReset;
    return true;
  }
  if (normalized == "DATA_BUS_MONITOR_START") {
    ctx.cmd = CommandType::DataBusMonitorStart;
    return true;
  }
  if (normalized == "DATA_BUS_MONITOR_STOP") {
    ctx.cmd = CommandType::DataBusMonitorStop;
    return true;
  }

  if (normalized.startsWith("SET_A")) {
    if (normalized.length() != 10) {
      return false;
    }
    uint32_t addr = 0;
    if (!parseHex32(normalized.substring(5), addr)) {
      return false;
    }
    if (addr > 0x7FFFFUL) {
      return false;
    }
    ctx.addr = addr;
    ctx.cmd = CommandType::SetAddress;
    return true;
  }

  int p1 = normalized.indexOf('|');
  if (p1 < 0) {
    return false;
  }
  String op = normalized.substring(0, p1);
  op.trim();

  if (op == "SEQUENCE") {
    // SEQUENCE|name|script — need raw line for case-sensitive name + script
    ctx.cmd = CommandType::Sequence;
    return true;
  }

  if (op == "PARAMETER") {
    // PARAMETER|key|value — need raw line for case-sensitive key
    ctx.cmd = CommandType::Parameter;
    return true;
  }

  if (op == "READ") {
    int p2 = normalized.indexOf('|', p1 + 1);
    if (p2 < 0) {
      return false;
    }
    String addrText = normalized.substring(p1 + 1, p2);
    String lenText = normalized.substring(p2 + 1);
    if (!parseHex32(addrText, ctx.addr)) {
      return false;
    }
    if (!parseDec32(lenText, ctx.len)) {
      return false;
    }
    ctx.cmd = CommandType::Read;
    return true;
  }

  if (op == "PROGRAM_BYTE") {
    int p2 = normalized.indexOf('|', p1 + 1);
    uint32_t value = 0;
    if (p2 < 0) {
      return false;
    }
    String addrText = normalized.substring(p1 + 1, p2);
    String valueText = normalized.substring(p2 + 1);
    if (!parseHex32(addrText, ctx.addr)) {
      return false;
    }
    if (!parseHex32(valueText, value)) {
      return false;
    }
    ctx.value = static_cast<uint8_t>(value & 0xFF);
    ctx.cmd = CommandType::ProgramByte;
    return true;
  }

  if (op == "SECTOR_ERASE") {
    String addrText = normalized.substring(p1 + 1);
    if (!parseHex32(addrText, ctx.addr)) {
      return false;
    }
    ctx.cmd = CommandType::SectorErase;
    return true;
  }

  if (op == "WRITE_STATUS") {
    int p2 = normalized.indexOf('|', p1 + 1);
    int p3 = normalized.indexOf('|', p2 + 1);
    uint32_t expected = 0;
    if (p2 < 0 || p3 < 0) {
      return false;
    }
    String addrText = normalized.substring(p1 + 1, p2);
    String expectedText = normalized.substring(p2 + 1, p3);
    String timeoutText = normalized.substring(p3 + 1);
    if (!parseHex32(addrText, ctx.addr)) {
      return false;
    }
    if (!parseHex32(expectedText, expected)) {
      return false;
    }
    if (!parseDec32(timeoutText, ctx.timeoutMs)) {
      return false;
    }
    ctx.value = static_cast<uint8_t>(expected & 0xFF);
    ctx.cmd = CommandType::WriteStatus;
    return true;
  }

  if (op == "ADDR_BUS_TEST") {
    String bank = normalized.substring(p1 + 1);
    bank.trim();
    if (bank == "A0_7" || bank == "LOW8") {
      ctx.bank = 0;
    } else if (bank == "A8_15" || bank == "HIGH8") {
      ctx.bank = 1;
    } else if (bank == "A16_18" || bank == "HIGH3") {
      ctx.bank = 2;
    } else {
      return false;
    }
    ctx.cmd = CommandType::AddrBusTest;
    return true;
  }

  return false;
}

bool validateRange(uint32_t addr, uint32_t len) {
  if (len == 0) {
    return false;
  }
  if (addr >= g_chipSizeBytes) {
    return false;
  }
  if (len > g_chipSizeBytes) {
    return false;
  }
  if (addr > (g_chipSizeBytes - len)) {
    return false;
  }
  return true;
}
