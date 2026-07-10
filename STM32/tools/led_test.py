#!/usr/bin/env python3
"""Drive the LED on PB0 (servo index 2 = TIM3 Ch3) to watch the slew ramp.

Sends SET_SERVO moves with large targets + small steps so the brightness ramp
is visible on an LED (a real servo pulse of ~1500 is too dim to see). Prints the
immediate ACK and the deferred "done" reply (with final position) for each move.

Usage: python3 led_test.py [/dev/ttyACM0]
"""
import sys
import time

from proto import ACK, SET_SERVO, Port, frame, parse_frames, servo_payload

LED = 2  # SERVOS[2] -> Ch3 -> PB0


def main():
    port = Port(sys.argv[1] if len(sys.argv) > 1 else "/dev/ttyACM0")
    port.drain(0.3)
    txn = 1

    def move(target, step, note):
        nonlocal txn
        # ramp time ≈ (distance / step) * 20 ms; generous window + margin.
        window = (target // max(step, 1)) * 0.02 + 1.0 if step else 0.5
        print(f"\n-> {note}: target={target} step={step}  (~{window:.1f}s)")
        port.write(frame(txn, SET_SERVO, servo_payload(LED, target, step)))
        for t, f, p in parse_frames(port.drain(window)):
            if t != txn:
                continue
            if f == ACK:
                print("   ACK (accepted)")
            elif f == SET_SERVO:
                pos = int.from_bytes(p, "little") if len(p) == 2 else None
                print(f"   DONE at {pos}")
        txn += 1

    move(10000, 100, "fade up (slow)")       # ~2 s ramp
    time.sleep(0.5)
    move(0, 100, "fade down (slow)")         # ~2 s ramp
    time.sleep(0.5)
    move(15000, 500, "fade up (fast)")       # ~0.6 s ramp
    time.sleep(0.5)
    move(0, 0, "snap off (step=0)")          # instant

    print("\ndone")


if __name__ == "__main__":
    main()
