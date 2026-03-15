#include <Arduino.h>

#include "command_executor.h"
#include "command_parser.h"
#include "device_config.h"
#include "device_globals.h"
#include "hal_bus.h"
#include "protocol_io.h"
#include "version_info.h"

void setup() {
  pinMode(PIN_WE, OUTPUT);
  pinMode(PIN_OE, OUTPUT);
  pinMode(PIN_CE, OUTPUT);

  digitalWrite(PIN_WE, HIGH);
  digitalWrite(PIN_OE, HIGH);
  digitalWrite(PIN_CE, HIGH);

  for (int pin = PA0; pin <= PA10; ++pin) {
    pinMode(pin, OUTPUT);
    digitalWrite(pin, LOW);
  }
  for (int pin = PB0; pin <= PB7; ++pin) {
    pinMode(pin, OUTPUT);
    digitalWrite(pin, LOW);
  }

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
