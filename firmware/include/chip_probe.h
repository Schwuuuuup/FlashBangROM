#pragma once

#include <Arduino.h>

#include "device_types.h"

// Runs a probe chain across known chip-specific ID entry/exit sequences.
ChipInfo probeChipInfo();

// True when the detected chip is in the SST39 family supported by current write/erase ops.
bool hasSupportedSst39(ChipInfo info);
