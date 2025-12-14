#ifndef PENCIL_BRIDGE_H
#define PENCIL_BRIDGE_H

#include <stdint.h>

// Raw Apple Pencil data point
typedef struct {
    float x;           // Screen x coordinate
    float y;           // Screen y coordinate
    float force;       // Pressure (0.0 - 1.0)
    float altitude;    // Altitude angle in radians
    float azimuth;     // Azimuth angle in radians (relative to screen)
    double timestamp;  // Event timestamp
    uint8_t phase;     // 0 = began, 1 = moved, 2 = ended, 3 = cancelled
} RawPencilPoint;

// Swift-implemented functions
void swift_attach_pencil_capture(void* ui_view);
void swift_detach_pencil_capture(void* ui_view);

// Rust-implemented function (called from Swift)
void rust_push_pencil_point(RawPencilPoint point);

#endif // PENCIL_BRIDGE_H
