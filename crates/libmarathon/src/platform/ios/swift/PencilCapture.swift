import UIKit

@_cdecl("swift_attach_pencil_capture")
func swiftAttachPencilCapture(_ viewPtr: UnsafeMutableRawPointer) {
    DispatchQueue.main.async {
        let view = Unmanaged<UIView>.fromOpaque(viewPtr).takeUnretainedValue()
        let recognizer = PencilGestureRecognizer()
        recognizer.cancelsTouchesInView = false
        recognizer.delaysTouchesEnded = false
        view.addGestureRecognizer(recognizer)
        print("[Swift] Pencil capture attached")
    }
}

class PencilGestureRecognizer: UIGestureRecognizer {
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        state = .began
        send(touches, event: event, phase: 0)
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
        state = .changed
        send(touches, event: event, phase: 1)
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        state = .ended
        send(touches, event: event, phase: 2)
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
        state = .cancelled
        send(touches, event: event, phase: 2)
    }

    private func send(_ touches: Set<UITouch>, event: UIEvent?, phase: UInt8) {
        for touch in touches where touch.type == .pencil {
            for t in event?.coalescedTouches(for: touch) ?? [touch] {
                let loc = t.preciseLocation(in: view)
                pencil_point_received(RawPencilPoint(
                    x: Float(loc.x),
                    y: Float(loc.y),
                    force: Float(t.force),
                    altitude: Float(t.altitudeAngle),
                    azimuth: Float(t.azimuthAngle(in: view)),
                    timestamp: t.timestamp,
                    phase: phase
                ))
            }
        }
    }
}
