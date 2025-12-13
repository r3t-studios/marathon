/**
 * C header for Rust-Swift interop
 *
 * This defines the interface between Rust and Swift.
 * Both sides include this header to ensure they agree on data types.
 */

#ifndef PENCIL_BRIDGE_H
#define PENCIL_BRIDGE_H

#include <stdint.h>

/**
 * Raw pencil data from iOS UITouch
 *
 * This struct uses C types that both Rust and Swift understand.
 * The memory layout must match exactly on both sides.
 */
typedef struct {
    float x;          // Screen X in points
    float y;          // Screen Y in points
    float force;      // Pressure (0.0 - 4.0)
    float altitude;   // Angle from screen (radians)
    float azimuth;    // Rotation angle (radians)
    double timestamp; // iOS system timestamp
    uint8_t phase;    // 0=began, 1=moved, 2=ended
} RawPencilPoint;

/**
 * Called from Swift when a pencil point is captured
 *
 * This is implemented in Rust (pencil_bridge.rs)
 */
void pencil_point_received(RawPencilPoint point);

/**
 * Attach pencil capture to a UIView
 *
 * This is implemented in Swift (PencilCapture.swift)
 */
void swift_attach_pencil_capture(void* view);

#endif
