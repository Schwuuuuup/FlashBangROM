#include "device_globals.h"

DeviceState g_state = DeviceState::Init;
CommandContext g_ctx;
String g_line;
uint32_t g_chipSizeBytes = 512UL * 1024UL;
bool g_dataBusIsOutput = false;
bool g_dataBusMonitorActive = false;
uint32_t g_dataBusMonitorLastSampleMs = 0;
uint32_t g_dataBusMonitorAddr = 0;
bool g_dataBusMonitorAddrSet = false;

void resetContext() {
  g_ctx = CommandContext{};
}
