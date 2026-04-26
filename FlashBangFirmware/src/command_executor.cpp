#include "command_executor.h"

#include <cstdlib>
#include <cstring>

#include "chip_probe.h"
#include "command_parser.h"
#include "device_config.h"
#include "device_globals.h"
#include "hal_bus.h"
#include "protocol_io.h"
#include "seq_interpreter.h"
#include "driver_ops.h"
#include "version_info.h"

namespace {
static constexpr uint8_t SHORT_ERR_RING_MAX = 8;
String g_shortErrRing[SHORT_ERR_RING_MAX];
uint8_t g_shortErrCount = 0;
uint32_t g_shortWriteOkCount = 0;
uint32_t g_shortReadOkCount = 0;

void pushShortError(const String& line) {
  if (g_shortErrCount < SHORT_ERR_RING_MAX) {
    g_shortErrRing[g_shortErrCount++] = line;
    return;
  }
  for (uint8_t i = 1; i < SHORT_ERR_RING_MAX; ++i) {
    g_shortErrRing[i - 1] = g_shortErrRing[i];
  }
  g_shortErrRing[SHORT_ERR_RING_MAX - 1] = line;
}

void reportError(const char* code, const char* message, bool shortForm) {
  if (shortForm) {
    pushShortError(String("ERR|") + code + "|" + message);
    return;
  }
  sendErr(code, message);
}

bool decodeHexNibble(char c, uint8_t& out) {
  if (c >= '0' && c <= '9') {
    out = static_cast<uint8_t>(c - '0');
    return true;
  }
  if (c >= 'A' && c <= 'F') {
    out = static_cast<uint8_t>(10 + (c - 'A'));
    return true;
  }
  if (c >= 'a' && c <= 'f') {
    out = static_cast<uint8_t>(10 + (c - 'a'));
    return true;
  }
  return false;
}

uint32_t makeAddrForBank(uint8_t bank, uint8_t pattern) {
  if (bank == 0) {
    return static_cast<uint32_t>(pattern);
  }
  if (bank == 1) {
    return static_cast<uint32_t>(pattern) << 8;
  }
  return static_cast<uint32_t>(pattern & 0x07) << 16;
}

uint8_t expectedForBank(uint8_t bank, uint8_t pattern) {
  if (bank == 2) {
    return static_cast<uint8_t>(pattern & 0x07);
  }
  return pattern;
}

bool sampleMatches(uint8_t bank, uint8_t observed, uint8_t expected) {
  if (bank == 2) {
    return (observed & 0x07) == (expected & 0x07);
  }
  return observed == expected;
}

const char* bankName(uint8_t bank) {
  if (bank == 0) {
    return "A0_7";
  }
  if (bank == 1) {
    return "A8_15";
  }
  return "A16_18";
}

bool isBuiltinSequenceUpper(const String& name) {
  return name == "ID_ENTRY" ||
         name == "ID_READ" ||
         name == "ID_EXIT" ||
         name == "PROGRAM_BYTE" ||
         name == "PROGRAM_RANGE" ||
         name == "CHIP_ERASE";
}

bool isBuiltinSequenceLowerLegacy(const String& name) {
  return name == "id_entry" ||
         name == "id_read" ||
         name == "id_exit" ||
         name == "program_byte" ||
         name == "program_range" ||
         name == "chip_erase";
}

bool isCustomSequenceName(const String& name) {
  if (name.length() == 0) {
    return false;
  }
  char first = name.charAt(0);
  if (first < 'a' || first > 'z') {
    return false;
  }
  for (uint16_t i = 0; i < name.length(); ++i) {
    char c = name.charAt(i);
    bool ok = (c >= 'a' && c <= 'z') ||
              (c >= '0' && c <= '9') ||
              c == '_';
    if (!ok) {
      return false;
    }
  }
  return true;
}

bool isBuiltinParameterName(const String& key) {
  return key == "CHIP_SIZE" || key == "SECTOR_SIZE" || key == "ADDR_BITS";
}

bool isCustomParameterName(const String& key) {
  if (key.length() == 0 || key.length() >= MAX_PARAM_NAME) {
    return false;
  }
  char first = key.charAt(0);
  if (first < 'a' || first > 'z') {
    return false;
  }
  for (uint16_t i = 0; i < key.length(); ++i) {
    char c = key.charAt(i);
    bool ok = (c >= 'a' && c <= 'z') ||
              (c >= '0' && c <= '9') ||
              c == '_';
    if (!ok) {
      return false;
    }
  }
  return true;
}

bool parseHexArg(const String& text, uint32_t& out) {
  String normalized = text;
  normalized.trim();
  if (normalized.length() == 0) {
    return false;
  }
  char* endPtr = nullptr;
  unsigned long parsed = strtoul(normalized.c_str(), &endPtr, 16);
  if (endPtr == nullptr || *endPtr != '\0') {
    return false;
  }
  out = static_cast<uint32_t>(parsed);
  return true;
}

void executeAddrBusTest(uint8_t bank) {
  chipSelect(false);
  outputEnable(false);
  setDataBusMode(false);

  uint8_t mismatches = 0;
  uint32_t firstAddr = 0;
  uint8_t firstExpected = 0;
  uint8_t firstObserved = 0;

  const uint16_t limit = (bank == 2) ? 8 : 256;
  for (uint16_t p = 0; p < limit; ++p) {
    const uint8_t pattern = static_cast<uint8_t>(p & 0xFF);
    const uint32_t addr = makeAddrForBank(bank, pattern);
    const uint8_t expected = expectedForBank(bank, pattern);

    busSetAddress(addr);
    delayMicroseconds(2);
    const uint8_t observed = busReadData();

    if (!sampleMatches(bank, observed, expected)) {
      if (mismatches == 0) {
        firstAddr = addr;
        firstExpected = expected;
        firstObserved = observed;
      }
      ++mismatches;
    }
  }

  if (mismatches == 0) {
    sendOk("ADDR_BUS_TEST", String("bank=") + bankName(bank) + ",mismatches=0");
    return;
  }

  String detail = String("bank=") + bankName(bank) +
                  ",mismatches=" + String(mismatches) +
                  ",first_addr=0x" + String(firstAddr, HEX) +
                  ",expected=0x" + String(firstExpected, HEX) +
                  ",observed=0x" + String(firstObserved, HEX);
  sendErr("E_HW", detail.c_str());
}
}  // namespace

void executeCommand(const CommandContext& ctx) {
  switch (ctx.cmd) {
    case CommandType::Help:
      Serial.println("# HELP - FlashBang Firmware Commands");
      Serial.println("# ? | HELP                                   : show this help text");
      Serial.println("# HELLO[|host|proto]                         : return firmware/protocol/capabilities");
      Serial.println("# ID                                         : read chip ID using configured ID sequences");
      Serial.println("# READ|<addr-hex>|<len-dec>                  : read bytes from chip");
      Serial.println("# PROGRAM_BYTE|<addr-hex>|<value-hex>        : program one byte");
      Serial.println("# PROGRAM_RANGE|<addr-hex>|<hex-bytes>       : program byte range (auto-increment)");
      Serial.println("# SECTOR_ERASE|<addr-hex>                    : erase sector containing address");
      Serial.println("# CHIP_ERASE                                 : erase complete chip");
      Serial.println("# WRITE_STATUS|<addr-hex>|<exp-hex>|<ms-dec> : poll write completion status");
      Serial.println("# SEQUENCE|<name>|<script>                   : set/replace driver sequence");
      Serial.println("# PARAMETER|<key>|<value>                    : set built-in/custom driver parameter");
      Serial.println("# INSPECT                                    : dump active driver config as # lines");
      Serial.println("# DRIVER_RESET                               : restore built-in default driver");
      Serial.println("# DATA_BUS_MONITOR_START                     : start live data bus monitor stream");
      Serial.println("# DATA_BUS_MONITOR_STOP                      : stop live data bus monitor stream");
      Serial.println("# SET_A<addr-hex5>                           : set monitor address (00000..7FFFF)");
      Serial.println("# ADDR_BUS_TEST|A0_7|A8_15|A16_18            : run address bus walk test");
      Serial.println("# !                                          : flush shorthand errors + counters");
      Serial.println("# >AAAAADD                                   : shorthand write byte");
      Serial.println("# <AAAAALLLL                                 : shorthand read range");
      Serial.println("# :AAAAALLLL<hex-bytes>                      : shorthand write range");
      break;

    case CommandType::Flush: {
      for (uint8_t i = 0; i < g_shortErrCount; ++i) {
        Serial.println(g_shortErrRing[i]);
      }
      const uint32_t errCount = g_shortErrCount;
      g_shortErrCount = 0;

      String detail = String("w=") + String(g_shortWriteOkCount) +
                      ",r=" + String(g_shortReadOkCount) +
                      ",e=" + String(errCount);
      sendOk("!", detail);
      g_shortWriteOkCount = 0;
      g_shortReadOkCount = 0;
      break;
    }

    case CommandType::Hello:
      Serial.print("HELLO|");
      Serial.print(firmwareVersionText());
      Serial.print("|");
      Serial.print(firmwareProtocolVersion());
      Serial.println("|driver-upload");
      break;

    case CommandType::Id: {
      ChipInfo info = probeChipInfo();
      String detail = String("mf=0x") + String(info.manufacturer, HEX) +
                      ",dev=0x" + String(info.device, HEX);
      sendOk("ID", detail);
      break;
    }

    case CommandType::Read:
      executeRead(ctx.addr, ctx.len, !ctx.shortForm);
      if (ctx.shortForm) {
        g_shortReadOkCount++;
      }
      break;

    case CommandType::ProgramByte: {
      uint8_t observed = 0;
      bool verifyMismatch = false;
      bool ok = driverProgramByte(ctx.addr, ctx.value, &observed, &verifyMismatch);
      if (ok) {
        if (ctx.shortForm) {
          g_shortWriteOkCount++;
        } else {
          sendOk("PROGRAM_BYTE", "done");
        }
      } else if (verifyMismatch) {
        char detail[72];
        snprintf(detail, sizeof(detail),
                 "verify mismatch addr=0x%05lX expected=0x%02X observed=0x%02X",
                 static_cast<unsigned long>(ctx.addr),
                 static_cast<unsigned>(ctx.value),
                 static_cast<unsigned>(observed));
        reportError("E_VERIFY", detail, ctx.shortForm);
      } else {
        reportError("E_TIMEOUT", "program timeout", ctx.shortForm);
      }
      break;
    }

    case CommandType::ProgramRange: {
      String payload;
      if (ctx.shortForm) {
        // :AAAAALLLL<hex-bytes>
        if (g_rawLine.length() < 10) {
          reportError("E_PARSE", "program range malformed", true);
          break;
        }
        payload = g_rawLine.substring(10);
      } else {
        int p1 = g_rawLine.indexOf('|');
        int p2 = (p1 >= 0) ? g_rawLine.indexOf('|', p1 + 1) : -1;
        if (p1 < 0 || p2 < 0) {
          reportError("E_PARSE", "program range malformed", false);
          break;
        }
        payload = g_rawLine.substring(p2 + 1);
      }

      payload.trim();
      if (payload.length() == 0 || (payload.length() % 2) != 0) {
        reportError("E_PARAM", "program range payload invalid", ctx.shortForm);
        break;
      }

      const uint32_t byteLen = static_cast<uint32_t>(payload.length() / 2);
      if (ctx.shortForm && ctx.len != byteLen) {
        reportError("E_PARAM", "program range length mismatch", true);
        break;
      }
      if (!validateRange(ctx.addr, byteLen)) {
        reportError("E_RANGE", "program range out of bounds", ctx.shortForm);
        break;
      }

      static constexpr uint32_t MAX_PROGRAM_RANGE_BYTES = 96;
      if (byteLen > MAX_PROGRAM_RANGE_BYTES) {
        reportError("E_PARAM", "program range payload too large", ctx.shortForm);
        break;
      }

      uint8_t buf[MAX_PROGRAM_RANGE_BYTES];
      bool decodeOk = true;
      for (uint32_t i = 0; i < byteLen; ++i) {
        uint8_t hi = 0;
        uint8_t lo = 0;
        if (!decodeHexNibble(payload.charAt(i * 2), hi) ||
            !decodeHexNibble(payload.charAt(i * 2 + 1), lo)) {
          decodeOk = false;
          break;
        }
        buf[i] = static_cast<uint8_t>((hi << 4) | lo);
      }
      if (!decodeOk) {
        reportError("E_PARAM", "program range payload not hex", ctx.shortForm);
        break;
      }

      bool ok = false;
      const char* rangeScript = findSequence(g_driverSlot, "PROGRAM_RANGE");
      if (rangeScript != nullptr) {
        // Winbond-style page programming must not cross a page boundary
        // within one program-range transaction.
        const uint32_t pageSize = g_driverSlot.sector_size_bytes;
        const bool pageSplitNeeded = pageSize > 0 && pageSize <= 256;

        if (!pageSplitNeeded) {
          ok = executeProgramRange(rangeScript, ctx.addr, buf, byteLen);
        } else {
          ok = true;
          uint32_t offset = 0;
          while (offset < byteLen) {
            const uint32_t curAddr = ctx.addr + offset;
            const uint32_t pageOffset = curAddr % pageSize;
            const uint32_t pageRemain = pageSize - pageOffset;
            const uint32_t chunkLen = (byteLen - offset) < pageRemain
                                          ? (byteLen - offset)
                                          : pageRemain;
            if (!executeProgramRange(rangeScript, curAddr, buf + offset, chunkLen)) {
              ok = false;
              break;
            }
            offset += chunkLen;
          }
        }
      } else {
        ok = true;
        for (uint32_t i = 0; i < byteLen; ++i) {
          uint8_t observed = 0;
          bool verifyMismatch = false;
          if (!driverProgramByte(ctx.addr + i, buf[i], &observed, &verifyMismatch)) {
            ok = false;
            break;
          }
        }
      }

      if (ok) {
        if (ctx.shortForm) {
          g_shortWriteOkCount++;
        } else {
          sendOk("PROGRAM_RANGE", "done");
        }
      } else {
        reportError("E_TIMEOUT", "program range failed", ctx.shortForm);
      }
      break;
    }

    case CommandType::SectorErase: {
      bool ok = driverSectorErase(ctx.addr);
      if (ok) {
        sendOk("SECTOR_ERASE", "done");
      } else {
        sendErr("E_TIMEOUT", "sector erase timeout");
      }
      break;
    }

    case CommandType::ChipErase: {
      bool ok = driverChipErase();
      if (ok) {
        sendOk("CHIP_ERASE", "done");
      } else {
        sendErr("E_TIMEOUT", "chip erase timeout");
      }
      break;
    }

    case CommandType::WriteStatus: {
      uint32_t timeoutUs =
          ctx.timeoutMs > 0 ? (ctx.timeoutMs * 1000UL) : TIMEOUT_BYTE_PROGRAM_US;
      bool ok = waitDq7DoneProgram(ctx.addr, ctx.value, timeoutUs);
      if (ok) {
        sendOk("WRITE_STATUS", "stable");
      } else {
        sendErr("E_TIMEOUT", "status timeout");
      }
      break;
    }

    case CommandType::DataBusMonitorStart:
      g_dataBusMonitorActive = true;
      g_dataBusMonitorLastSampleMs = 0;
      sendOk("DATA_BUS_MONITOR_START", "streaming");
      break;

    case CommandType::DataBusMonitorStop:
      g_dataBusMonitorActive = false;
      sendOk("DATA_BUS_MONITOR_STOP", "stopped");
      break;

    case CommandType::SetAddress: {
      g_dataBusMonitorAddr = ctx.addr & 0x7FFFFUL;
      g_dataBusMonitorAddrSet = true;
      busSetAddress(g_dataBusMonitorAddr);

      char detail[24];
      snprintf(detail, sizeof(detail), "addr=0x%05lX",
               static_cast<unsigned long>(g_dataBusMonitorAddr));
      sendOk("SET_A", detail);
      break;
    }

    case CommandType::AddrBusTest:
      executeAddrBusTest(ctx.bank);
      break;

    case CommandType::Sequence: {
      // Parse from raw line: SEQUENCE|name|script
      int p1 = g_rawLine.indexOf('|');
      if (p1 < 0) { sendErr("E_PARSE", "missing pipe"); break; }
      int p2 = g_rawLine.indexOf('|', p1 + 1);
      if (p2 < 0) { sendErr("E_PARSE", "missing script"); break; }
      String name = g_rawLine.substring(p1 + 1, p2);
      String script = g_rawLine.substring(p2 + 1);
      name.trim();
      script.trim();
      if (name.length() == 0 || name.length() >= MAX_SEQ_NAME) {
        if (g_inspectPasteActive) {
          break;
        }
        sendErr("E_PARAM", "name too long or empty");
        break;
      }
      if (isBuiltinSequenceLowerLegacy(name)) {
        if (g_inspectPasteActive) {
          break;
        }
        String detail = String("built-in sequence names must be uppercase: '") +
                        name + "'";
        sendErr("E_PARAM", detail.c_str());
        break;
      }
      if (!isBuiltinSequenceUpper(name) && !isCustomSequenceName(name)) {
        if (g_inspectPasteActive) {
          break;
        }
        String detail = String("bad sequence name len=") + String(name.length()) +
                        ": '" + name + "'";
        sendErr("E_PARAM", detail.c_str());
        break;
      }
      if (script.length() == 0 || script.length() >= MAX_SEQ_SCRIPT) {
        if (g_inspectPasteActive) {
          break;
        }
        sendErr("E_PARAM", "script too long or empty");
        break;
      }
      // Find existing slot or add new
      bool found = false;
      for (uint8_t i = 0; i < g_driverSlot.sequence_count; i++) {
        if (strcmp(g_driverSlot.sequences[i].name, name.c_str()) == 0) {
          strncpy(g_driverSlot.sequences[i].script, script.c_str(), MAX_SEQ_SCRIPT - 1);
          g_driverSlot.sequences[i].script[MAX_SEQ_SCRIPT - 1] = '\0';
          found = true;
          break;
        }
      }
      if (!found) {
        if (g_driverSlot.sequence_count >= MAX_SEQUENCES) {
          sendErr("E_FULL", "max sequences reached");
          break;
        }
        SequenceSlot& s = g_driverSlot.sequences[g_driverSlot.sequence_count];
        strncpy(s.name, name.c_str(), MAX_SEQ_NAME - 1);
        s.name[MAX_SEQ_NAME - 1] = '\0';
        strncpy(s.script, script.c_str(), MAX_SEQ_SCRIPT - 1);
        s.script[MAX_SEQ_SCRIPT - 1] = '\0';
        g_driverSlot.sequence_count++;
      }
      g_driverSlot.is_default = false;
      if (!g_inspectPasteActive) {
        sendOk("SEQUENCE", name);
      }
      break;
    }

    case CommandType::Parameter: {
      // Parse from raw line: PARAMETER|key|value
      int p1 = g_rawLine.indexOf('|');
      if (p1 < 0) { sendErr("E_PARSE", "missing pipe"); break; }
      int p2 = g_rawLine.indexOf('|', p1 + 1);
      if (p2 < 0) { sendErr("E_PARSE", "missing value"); break; }
      String key = g_rawLine.substring(p1 + 1, p2);
      String valStr = g_rawLine.substring(p2 + 1);
      key.trim();
      valStr.trim();
      int commentPos = valStr.indexOf('#');
      if (commentPos >= 0) {
        valStr = valStr.substring(0, commentPos);
        valStr.trim();
      }
      if (key.length() == 0) {
        if (g_inspectPasteActive) {
          break;
        }
        sendErr("E_PARAM", "empty key");
        break;
      }

      // Treat visibly corrupted keys as malformed input instead of semantic param errors.
      String keyUp = key;
      keyUp.toUpperCase();
      if (!isBuiltinParameterName(keyUp) && !isCustomParameterName(key)) {
        if (g_inspectPasteActive) {
          break;
        }
        sendErr("E_PARSE", "malformed command");
        break;
      }

      // Parse value as hex (consistent with protocol)
      uint32_t val = 0;
      if (!parseHex32(valStr, val)) {
        if (g_inspectPasteActive) {
          break;
        }
        sendErr("E_PARAM", "bad value");
        break;
      }

      char firstChar = key.charAt(0);
      if (firstChar >= 'A' && firstChar <= 'Z') {
        // Built-in parameter (UPPERCASE)
        if (keyUp == "CHIP_SIZE") {
          g_driverSlot.chip_size_bytes = val;
          g_chipSizeBytes = val;
        } else if (keyUp == "SECTOR_SIZE") {
          g_driverSlot.sector_size_bytes = val;
        } else if (keyUp == "ADDR_BITS") {
          g_driverSlot.address_bits = static_cast<uint8_t>(val);
        } else {
          if (g_inspectPasteActive) {
            break;
          }
          sendErr("E_PARAM", "unknown built-in param");
          break;
        }
      } else {
        // Custom parameter (lowercase) → $0..$7
        if (key.length() >= MAX_PARAM_NAME) {
          if (g_inspectPasteActive) {
            break;
          }
          sendErr("E_PARAM", "key too long");
          break;
        }
        // Find existing or add new
        bool found = false;
        for (uint8_t i = 0; i < g_driverSlot.custom_param_count; i++) {
          if (strcmp(g_driverSlot.custom_params[i].name, key.c_str()) == 0) {
            g_driverSlot.custom_params[i].value = val;
            found = true;
            break;
          }
        }
        if (!found) {
          if (g_driverSlot.custom_param_count >= MAX_CUSTOM_PARAMS) {
            if (g_inspectPasteActive) {
              break;
            }
            sendErr("E_FULL", "max custom params reached");
            break;
          }
          CustomParam& cp = g_driverSlot.custom_params[g_driverSlot.custom_param_count];
          strncpy(cp.name, key.c_str(), MAX_PARAM_NAME - 1);
          cp.name[MAX_PARAM_NAME - 1] = '\0';
          cp.value = val;
          g_driverSlot.custom_param_count++;
        }
      }
      g_driverSlot.is_default = false;
      if (!g_inspectPasteActive) {
        sendOk("PARAMETER", key);
      }
      break;
    }

    case CommandType::Inspect: {
      // Output all state in re-inputtable syntax
      Serial.println("# INSPECT BEGIN");
      // Built-in parameters
      char buf[80];
      snprintf(buf, sizeof(buf), "PARAMETER|CHIP_SIZE|%lX",
               static_cast<unsigned long>(g_driverSlot.chip_size_bytes));
      Serial.println(buf);
      snprintf(buf, sizeof(buf), "PARAMETER|SECTOR_SIZE|%lX",
               static_cast<unsigned long>(g_driverSlot.sector_size_bytes));
      Serial.println(buf);
      snprintf(buf, sizeof(buf), "PARAMETER|ADDR_BITS|%X",
               static_cast<unsigned>(g_driverSlot.address_bits));
      Serial.println(buf);
      // Custom parameters
      for (uint8_t i = 0; i < g_driverSlot.custom_param_count; i++) {
        snprintf(buf, sizeof(buf), "PARAMETER|%s|%lX",
                 g_driverSlot.custom_params[i].name,
                 static_cast<unsigned long>(g_driverSlot.custom_params[i].value));
        Serial.print(buf);
        Serial.print("  # $");
        Serial.println(i);
      }
      // Sequences
      for (uint8_t i = 0; i < g_driverSlot.sequence_count; i++) {
        Serial.print("SEQUENCE|");
        Serial.print(g_driverSlot.sequences[i].name);
        Serial.print("|");
        Serial.println(g_driverSlot.sequences[i].script);
      }
      Serial.print("# INSPECT END default=");
      Serial.println(g_driverSlot.is_default ? "true" : "false");
      sendOk("INSPECT", "done");
      break;
    }

    case CommandType::DriverReset:
      initDriverSlotDefaults(g_driverSlot);
      g_chipSizeBytes = g_driverSlot.chip_size_bytes;
      sendOk("DRIVER_RESET", "defaults restored");
      break;

    case CommandType::CustomSequence: {
      String line = g_rawLine;
      line.trim();

      int p1 = line.indexOf('|');
      int p2 = (p1 >= 0) ? line.indexOf('|', p1 + 1) : -1;
      int p3 = (p2 >= 0) ? line.indexOf('|', p2 + 1) : -1;
      if (p3 >= 0) {
        sendErr("E_PARSE", "too many custom args");
        break;
      }

      String name = (p1 >= 0) ? line.substring(0, p1) : line;
      name.trim();

      uint32_t seqAddr = 0;
      uint32_t seqData = 0;
      if (p1 >= 0) {
        String addrText = (p2 >= 0) ? line.substring(p1 + 1, p2) : line.substring(p1 + 1);
        if (!parseHexArg(addrText, seqAddr)) {
          sendErr("E_PARAM", "bad custom addr");
          break;
        }
      }
      if (p2 >= 0) {
        String dataText = line.substring(p2 + 1);
        if (!parseHexArg(dataText, seqData)) {
          sendErr("E_PARAM", "bad custom data");
          break;
        }
      }

      const char* script = findSequence(g_driverSlot, name.c_str());
      if (script == nullptr) {
        sendErr("E_SEQ", "unknown sequence");
        break;
      }
      SeqResult r = executeSequence(script, seqAddr, static_cast<uint8_t>(seqData & 0xFF));
      if (r.ok) {
        sendOk(name.c_str(), "done");
      } else {
        sendErr("E_TIMEOUT", "sequence failed");
      }
      break;
    }

    default:
      sendErr("E_UNSUPPORTED", "unsupported command");
      break;
  }
}
