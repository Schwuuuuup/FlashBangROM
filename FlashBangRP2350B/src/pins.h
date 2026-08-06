// Pin mapping for the FlashBangRP2350B board.
//
// RP2350B (48-GPIO package) wired directly to an SST39SF040 (512 KByte).
// The mapping mirrors FlashBangRP2350B/Docs/PinConnections.md 1:1.
//
// NOTE: A18 is on GPIO25, which also drives the on-board LED through a 5K6
//       resistor. This is intentional/experimental (see PinConnections.md).
#pragma once

#include <stdint.h>

// Address lines A0..A18 -> RP2350B GPIO number. Index == address bit.
static const uint8_t kAddrGpio[19] = {
    47,  // A0
    45,  // A1
    43,  // A2
    41,  // A3
    39,  // A4
    37,  // A5
    35,  // A6
    33,  // A7
    14,  // A8
    12,  // A9
    6,   // A10
    10,  // A11
    31,  // A12
    16,  // A13
    18,  // A14
    29,  // A15
    27,  // A16
    20,  // A17
    25,  // A18
};

// Data lines D0..D7 -> RP2350B GPIO number. Index == data bit.
// NOTE: Docs/PinConnections.md lists D0/D2 swapped relative to this. The
// mapping below was confirmed correct by physical continuity measurement;
// the doc has the error, not this file.
static const uint8_t kDataGpio[8] = {
    7,   // D0
    9,   // D1
    11,  // D2
    5,   // D3
    3,   // D4
    1,   // D5
    0,   // D6
    2,   // D7
};

// Control lines (all active LOW on the chip).
static const uint8_t kPinCE = 4;   // Chip Enable  (CE#)
static const uint8_t kPinOE = 8;   // Output Enable(OE#)
static const uint8_t kPinWE = 22;  // Write Enable (WE#)
