#include "command_executor.h"

#include <cstring>

#include "chip_probe.h"
#include "command_parser.h"
#include "device_config.h"
#include "device_globals.h"
#include "hal_bus.h"
#include "protocol_io.h"
#include "seq_interpreter.h"
#include "sst39_ops.h"
#include "version_info.h"

namespace {
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
         name == "SECTOR_ERASE" ||
         name == "CHIP_ERASE";
}

bool isBuiltinSequenceLowerLegacy(const String& name) {
  return name == "id_entry" ||
         name == "id_read" ||
         name == "id_exit" ||
         name == "program_byte" ||
         name == "program_range" ||
         name == "sector_erase" ||
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
      sendOk("HELP",
             "HELLO|host|proto, ID, READ|addr|len, PROGRAM_BYTE|addr|value, "
            "SECTOR_ERASE|addr, "
            "CHIP_ERASE, WRITE_STATUS|addr|expected|timeout, "
            "SEQUENCE|name|script, PARAMETER|key|value, "
            "INSPECT, DRIVER_RESET, "
            "DATA_BUS_MONITOR_START, DATA_BUS_MONITOR_STOP, "
            "SET_A#####, "
            "ADDR_BUS_TEST|A0_7|A8_15|A16_18");
      break;

    case CommandType::Hello:
      Serial.print("HELLO|");
      Serial.print(firmwareVersionText());
      Serial.println("|0.3|driver-upload");
      break;

    case CommandType::Id: {
      ChipInfo info = probeChipInfo();
      String detail = String("mf=0x") + String(info.manufacturer, HEX) +
                      ",dev=0x" + String(info.device, HEX) +
                      ",name=" + info.name + ",size=" + String(info.sizeBytes) +
                      ",driver=" + info.driverId;
      sendOk("ID", detail);
      break;
    }

    case CommandType::Read:
      executeRead(ctx.addr, ctx.len);
      break;

    case CommandType::ProgramByte: {
      ChipInfo info = probeChipInfo();
      if (!hasSupportedSst39(info)) {
        sendErr("E_HW", "chip not detected");
        break;
      }
      bool ok = sst39ProgramByte(ctx.addr, ctx.value);
      if (ok) {
        sendOk("PROGRAM_BYTE", "done");
      } else {
        sendErr("E_TIMEOUT", "program timeout");
      }
      break;
    }

    case CommandType::SectorErase: {
      ChipInfo info = probeChipInfo();
      if (!hasSupportedSst39(info)) {
        sendErr("E_HW", "chip not detected");
        break;
      }
      bool ok = sst39SectorErase(ctx.addr);
      if (ok) {
        sendOk("SECTOR_ERASE", "done");
      } else {
        sendErr("E_TIMEOUT", "sector erase timeout");
      }
      break;
    }

    case CommandType::ChipErase: {
      ChipInfo info = probeChipInfo();
      if (!hasSupportedSst39(info)) {
        sendErr("E_HW", "chip not detected");
        break;
      }
      bool ok = sst39ChipErase();
      if (ok) {
        sendOk("CHIP_ERASE", "done");
      } else {
        sendErr("E_TIMEOUT", "chip erase timeout");
      }
      break;
    }

    case CommandType::WriteStatus: {
      ChipInfo info = probeChipInfo();
      if (!hasSupportedSst39(info)) {
        sendErr("E_HW", "chip not detected");
        break;
      }
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
      // g_rawLine contains the lowercase command name
      const char* script = findSequence(g_driverSlot, g_rawLine.c_str());
      if (script == nullptr) {
        sendErr("E_SEQ", "unknown sequence");
        break;
      }
      SeqResult r = executeSequence(script, 0, 0);
      if (r.ok) {
        sendOk(g_rawLine.c_str(), "done");
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
