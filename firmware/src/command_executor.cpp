#include "command_executor.h"

#include "chip_probe.h"
#include "device_config.h"
#include "device_globals.h"
#include "hal_bus.h"
#include "protocol_io.h"
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
            "DATA_BUS_MONITOR_START, DATA_BUS_MONITOR_STOP, "
            "SET_A#####, "
            "ADDR_BUS_TEST|A0_7|A8_15|A16_18");
      break;

    case CommandType::Hello:
      Serial.print("HELLO|");
      Serial.print(firmwareVersionText());
      Serial.println("|0.1|sst39-core,data-hex");
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

    default:
      sendErr("E_UNSUPPORTED", "unsupported command");
      break;
  }
}
