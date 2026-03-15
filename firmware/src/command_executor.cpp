#include "command_executor.h"

#include "device_config.h"
#include "protocol_io.h"
#include "sst39_ops.h"

void executeCommand(const CommandContext& ctx) {
  switch (ctx.cmd) {
    case CommandType::Help:
      sendOk("HELP",
             "ID, READ|addr|len, PROGRAM_BYTE|addr|value, SECTOR_ERASE|addr, "
             "CHIP_ERASE, WRITE_STATUS|addr|expected|timeout");
      break;

    case CommandType::Id: {
      Sst39ChipInfo info = sst39ReadId();
      String detail = String("mf=0x") + String(info.manufacturer, HEX) +
                      ",dev=0x" + String(info.device, HEX) +
                      ",name=" + info.name + ",size=" + String(info.sizeBytes);
      sendOk("ID", detail);
      break;
    }

    case CommandType::Read:
      executeRead(ctx.addr, ctx.len);
      break;

    case CommandType::ProgramByte: {
      bool ok = sst39ProgramByte(ctx.addr, ctx.value);
      if (ok) {
        sendOk("PROGRAM_BYTE", "done");
      } else {
        sendErr("E_TIMEOUT", "program timeout");
      }
      break;
    }

    case CommandType::SectorErase: {
      bool ok = sst39SectorErase(ctx.addr);
      if (ok) {
        sendOk("SECTOR_ERASE", "done");
      } else {
        sendErr("E_TIMEOUT", "sector erase timeout");
      }
      break;
    }

    case CommandType::ChipErase: {
      bool ok = sst39ChipErase();
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

    default:
      sendErr("E_UNSUPPORTED", "unsupported command");
      break;
  }
}
