#pragma once

#include <Arduino.h>

#include "device_types.h"

// Runs an ID probe using the currently loaded driver sequences.
ChipInfo probeChipInfo();
