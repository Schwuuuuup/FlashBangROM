#pragma once

#include <Arduino.h>

#include "device_types.h"

extern DeviceState g_state;
extern CommandContext g_ctx;
extern String g_line;
extern uint32_t g_chipSizeBytes;
extern bool g_dataBusIsOutput;

void resetContext();
