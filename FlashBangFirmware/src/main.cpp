#include <Arduino.h>

#include "command_executor.h"
#include "command_parser.h"
#include "device_config.h"
#include "device_globals.h"
#include "hal_bus.h"
#include "protocol_io.h"
#include "version_info.h"

namespace {
static constexpr uint32_t DATA_BUS_MONITOR_INTERVAL_MS = 100;

String dataBusToBinary(uint8_t value) {
  String bits;
  bits.reserve(8);
  for (int bit = 7; bit >= 0; --bit) {
    bits += ((value >> bit) & 0x01) ? '1' : '0';
  }
  return bits;
}

}  // namespace

void setup() {
  // Preload inactive level for active-low controls before enabling outputs.
  digitalWrite(PIN_WE, HIGH);
  digitalWrite(PIN_OE, HIGH);
  digitalWrite(PIN_CE, HIGH);

  pinMode(PIN_WE, OUTPUT);
  pinMode(PIN_OE, OUTPUT);
  pinMode(PIN_CE, OUTPUT);

  digitalWrite(PIN_WE, HIGH);
  digitalWrite(PIN_OE, HIGH);
  digitalWrite(PIN_CE, HIGH);

  initAddressBusPins();
  busSetAddress(0x00000);

  setDataBusMode(false);

  Serial.begin(115200);
  while (!Serial) {
  }
  delayMicroseconds(WAIT_POWER_UP_US);

  g_state = DeviceState::Idle;
  Serial.print("HELLO|");
  Serial.print(firmwareVersionText());
  Serial.println("|0.1|sst39-core,data-hex");
}

void loop() {
  switch (g_state) {
    case DeviceState::Init:
      g_state = DeviceState::Idle;
      break;

    case DeviceState::Idle:
      if (Serial.available()) {
        char c = static_cast<char>(Serial.read());
        if (c == '\n' || c == '\r') {
          if (g_line.length() > 0) {
            g_state = DeviceState::Parse;
          }
        } else {
          g_line += c;
        }
      } else if (g_dataBusMonitorActive) {
        const uint32_t now = millis();
        if (g_dataBusMonitorLastSampleMs == 0 ||
            (now - g_dataBusMonitorLastSampleMs) >= DATA_BUS_MONITOR_INTERVAL_MS) {
          if (g_dataBusMonitorAddrSet) {
            busSetAddress(g_dataBusMonitorAddr & 0x7FFFFUL);
          }
          const uint8_t sample = busReadData();
          String detail = dataBusToBinary(sample);
          if (g_dataBusMonitorAddrSet) {
            char addrHex[8];
            snprintf(addrHex, sizeof(addrHex), "%05lX",
                     static_cast<unsigned long>(g_dataBusMonitorAddr & 0x7FFFFUL));
            detail = String("A=") + addrHex + ",D=" + detail;
          }
          sendStatus("DATA_BUS", "SAMPLE", 0, detail);
          g_dataBusMonitorLastSampleMs = now;
        }
      }
      break;

    case DeviceState::Parse: {
      String line = g_line;
      resetContext();
      if (!parseLine(line, g_ctx)) {
        sendErr("E_PARSE", "malformed command");
        g_state = DeviceState::Error;
      } else {
        g_state = DeviceState::Execute;
      }
      break;
    }

    case DeviceState::Execute:
      executeCommand(g_ctx);
      g_line = "";
      g_state = DeviceState::Idle;
      break;

    case DeviceState::Error:
      g_line = "";
      resetContext();
      g_state = DeviceState::Idle;
      break;
  }
}
