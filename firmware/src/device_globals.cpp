#include "device_globals.h"

DeviceState g_state = DeviceState::Init;
CommandContext g_ctx;
String g_line;
uint32_t g_chipSizeBytes = 512UL * 1024UL;
bool g_dataBusIsOutput = false;

void resetContext() {
  g_ctx = CommandContext{};
}
