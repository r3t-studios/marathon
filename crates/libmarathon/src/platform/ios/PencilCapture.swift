import UIKit

// Import the C bridge header
// This declares rust_push_pencil_point() which we'll call
// Note: In build.rs we'll use -import-objc-header to make this available

/// Custom gesture recognizer that captures Apple Pencil input
class PencilGestureRecognizer: UIGestureRecognizer {
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        for touch in touches {
            if touch.type == .pencil || touch.type == .stylus {
                handlePencilTouch(touch, phase: 0) // began
            }
        }
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        for touch in touches {
            if touch.type == .pencil || touch.type == .stylus {
                handlePencilTouch(touch, phase: 1) // moved
            }
        }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        for touch in touches {
            if touch.type == .pencil || touch.type == .stylus {
                handlePencilTouch(touch, phase: 2) // ended
            }
        }
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        for touch in touches {
            if touch.type == .pencil || touch.type == .stylus {
                handlePencilTouch(touch, phase: 3) // cancelled
            }
        }
    }

    private func handlePencilTouch(_ touch: UITouch, phase: UInt8) {
        let location = touch.location(in: view)

        // Construct the raw pencil point
        var point = RawPencilPoint(
            x: Float(location.x),
            y: Float(location.y),
            force: Float(touch.force / touch.maximumPossibleForce),
            altitude: Float(touch.altitudeAngle),
            azimuth: Float(touch.azimuthAngle(in: view)),
            timestamp: touch.timestamp,
            phase: phase
        )

        // Call into Rust FFI
        rust_push_pencil_point(point)
    }

    // Allow simultaneous recognition with other gestures
    override func canPrevent(_ preventedGestureRecognizer: UIGestureRecognizer) -> Bool {
        return false
    }

    override func canBePrevented(by preventingGestureRecognizer: UIGestureRecognizer) -> Bool {
        return false
    }
}

// Static reference to the gesture recognizer (so we can detach later)
private var pencilGestureRecognizer: PencilGestureRecognizer?

/// Attach pencil capture to a UIView
@_cdecl("swift_attach_pencil_capture")
public func swiftAttachPencilCapture(_ uiView: UnsafeMutableRawPointer) {
    guard let view = Unmanaged<UIView>.fromOpaque(uiView).takeUnretainedValue() as UIView? else {
        print("⚠️ Failed to get UIView from pointer")
        return
    }

    // Create and attach the gesture recognizer
    let recognizer = PencilGestureRecognizer()
    recognizer.allowedTouchTypes = [NSNumber(value: UITouch.TouchType.pencil.rawValue)]
    recognizer.cancelsTouchesInView = false
    recognizer.delaysTouchesBegan = false
    recognizer.delaysTouchesEnded = false

    view.addGestureRecognizer(recognizer)
    pencilGestureRecognizer = recognizer

    print("✏️ Apple Pencil capture attached to view")
}

/// Detach pencil capture from a UIView
@_cdecl("swift_detach_pencil_capture")
public func swiftDetachPencilCapture(_ uiView: UnsafeMutableRawPointer) {
    guard let view = Unmanaged<UIView>.fromOpaque(uiView).takeUnretainedValue() as UIView? else {
        return
    }

    if let recognizer = pencilGestureRecognizer {
        view.removeGestureRecognizer(recognizer)
        pencilGestureRecognizer = nil
        print("✏️ Apple Pencil capture detached")
    }
}
