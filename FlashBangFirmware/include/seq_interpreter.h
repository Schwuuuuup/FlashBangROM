#pragma once

#include <Arduino.h>

#include "device_types.h"

// Execute a micro-script sequence.
// addr/data are pre-set for $A/$D substitution.
// Returns SeqResult with ok flag and any read values ($R0, $R1).
SeqResult executeSequence(const char* script, uint32_t addr, uint8_t data);

// Execute a named sequence from the driver slot.
SeqResult executeNamedSequence(const DriverSlot& slot, const char* name,
                               uint32_t addr, uint8_t data);

// Execute a program_range loop sequence over a data buffer.
// The script must contain {...} loop body. $A increments per byte, $D is set per byte.
bool executeProgramRange(const char* script, uint32_t startAddr,
                         const uint8_t* buf, uint32_t len);
