#pragma once

#include <Arduino.h>

#include "device_types.h"

extern DeviceState g_state;
extern CommandContext g_ctx;
extern String g_line;
extern uint32_t g_chipSizeBytes;
extern bool g_dataBusIsOutput;
extern bool g_dataBusMonitorActive;
extern uint32_t g_dataBusMonitorLastSampleMs;
extern uint32_t g_dataBusMonitorAddr;
extern bool g_dataBusMonitorAddrSet;

void resetContext();
