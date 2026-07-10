#!/usr/bin/env python3
"""das-boot protocol tester: sends frames and asserts ACK/NACK replies.

Mechanism tests use GET_SERVO (ACK only, no side effects). SET_SERVO tests
exercise the command validation (length / servo id / duty range). A valid
SET_SERVO also emits a deferred "done" reply (func=SET_SERVO) once the slew
finishes — the ACK assertions filter by (txn, func) so that doesn't interfere.

Usage: python3 proto_test.py [/dev/ttyACM0]
Exit:  0 = all pass, 1 = one or more failures.
"""
import sys
import time

from proto import (
    ACK, GET_SERVO, INVALID_FUNC, INVALID_PAYLOAD_LEN, INVALID_SERVO_DUTY,
    INVALID_SERVO_ID, NACK, SET_SERVO, Port, frame, parse_frames, servo_payload,
)

fails = 0


def check(label, ok, detail=""):
    global fails
    print(f"  [{'PASS' if ok else 'FAIL'}] {label}" + (f"  ({detail})" if detail and not ok else ""))
    if not ok:
        fails += 1


def main():
    port = Port(sys.argv[1] if len(sys.argv) > 1 else "/dev/ttyACM0")
    port.drain(0.3)  # flush stale

    def reply(txn, func, req, window=0.3):
        """Send req, return the (txn, func)-matching reply payload, or None."""
        port.drain(0.05)
        port.write(req)
        for t, f, p in parse_frames(port.drain(window)):
            if t == txn and f == func:
                return p
        return None

    def expect_ack(label, txn, req):
        check(label, reply(txn, ACK, req) is not None, "no ACK")

    def expect_nack(label, txn, req, exp_reason):
        p = reply(txn, NACK, req)
        check(label, p == bytes([exp_reason]), f"got {p.hex() if p else 'none'}")

    def expect_silence(label, req):
        port.drain(0.05)
        port.write(req)
        got = parse_frames(port.drain())
        check(label, not got, f"unexpected {[(t, hex(f)) for t, f, _ in got]}")

    # --- framing / mechanism (GET_SERVO = clean single ACK) ---
    print("1. known command -> ACK")
    expect_ack("GetServo -> ACK", 1, frame(1, GET_SERVO))

    print("2. unknown func -> NACK(InvalidFunc)")
    expect_nack("unknown -> NACK", 2, frame(2, 0x99), INVALID_FUNC)

    print("3. incoming ACK -> no reply")
    expect_silence("host ACK ignored", frame(3, ACK))

    print("4. incoming NACK -> no reply")
    expect_silence("host NACK ignored", frame(4, NACK))

    print("5. bad CRC -> no reply")
    bad = bytearray(frame(5, GET_SERVO))
    bad[-1] ^= 0xFF
    expect_silence("bad CRC ignored", bytes(bad))

    print("6. leading garbage + valid -> ACK (resync)")
    expect_ack("resync -> ACK", 6, bytes([0, 1, 2, 3]) + frame(6, GET_SERVO))

    print("7. two frames -> two ACKs")
    port.drain(0.05)
    port.write(frame(7, GET_SERVO) + frame(8, GET_SERVO))
    txns = sorted(t for t, f, _ in parse_frames(port.drain()) if f == ACK)
    check("two ACKs", txns == [7, 8], f"got {txns}")

    print("8. split frame -> ACK")
    f9 = frame(9, GET_SERVO)
    port.drain(0.05)
    port.write(f9[:4])
    time.sleep(0.2)
    port.write(f9[4:])
    check("split -> ACK", any(t == 9 and f == ACK for t, f, _ in parse_frames(port.drain())))

    print("9. flood 20 -> 20 ACKs")
    port.drain(0.05)
    port.write(b"".join(frame(100 + i, GET_SERVO) for i in range(20)))
    acks = sorted(t for t, f, _ in parse_frames(port.drain(1.0)) if f == ACK and 100 <= t < 120)
    check("20 ACKs", acks == list(range(100, 120)), f"got {len(acks)}")

    print("10. garbage (no sync) then valid -> ACK (recovery)")
    port.write(bytes([0x00, 0x10, 0x20, 0x30]))
    port.drain(0.2)
    expect_ack("recovery -> ACK", 200, frame(200, GET_SERVO))

    # --- SET_SERVO command validation ---
    print("11. SetServo 3-byte valid -> ACK")
    expect_ack("setservo(3) -> ACK", 300, frame(300, SET_SERVO, servo_payload(0, 1500)))

    print("12. SetServo 5-byte valid -> ACK")
    expect_ack("setservo(5) -> ACK", 301, frame(301, SET_SERVO, servo_payload(0, 1500, 50)))

    print("13. SetServo bad length -> NACK(InvalidPayloadLen)")
    expect_nack("bad len -> NACK", 302, frame(302, SET_SERVO, bytes([0])), INVALID_PAYLOAD_LEN)

    print("14. SetServo bad servo id -> NACK(InvalidServoID)")
    expect_nack("bad id -> NACK", 303, frame(303, SET_SERVO, servo_payload(9, 1500)), INVALID_SERVO_ID)

    print("15. SetServo out-of-range duty -> NACK(InvalidServoDuty)")
    expect_nack("bad duty -> NACK", 304, frame(304, SET_SERVO, servo_payload(0, 25000)), INVALID_SERVO_DUTY)

    print(f"\n{'ALL PASS' if fails == 0 else f'{fails} FAILED'}")
    sys.exit(1 if fails else 0)


if __name__ == "__main__":
    main()
