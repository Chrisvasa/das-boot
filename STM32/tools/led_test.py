#!/usr/bin/env python3
"""Breathe all 4 LEDs at different tempos, event-driven off the done reply.

Each LED ping-pongs between off (0) and MAX. The firmware answers a SET_SERVO
with an immediate ACK (accepted) and then a *deferred* done reply
(func=SET_SERVO, payload=final position) once the slew finishes. We ignore the
ACK and use the done reply as the cue to fire the opposite sweep -- so every LED
self-clocks at its own speed and they drift out of phase on their own.

One-way sweep times: LED0 ~1s, LED1 ~2s, LED2 ~3s, LED3 ~4s (set by step size).

Ctrl+C once : snap every LED off (step=0 -> instant) and wait for the ACKs.
Ctrl+C twice: force exit.

Usage: python3 led_test.py [/dev/ttyACM0]
"""
import sys
import time

from proto import (
    ACK, SET_SERVO, FrameReader, Port, frame, servo_payload,
)

NUM_LEDS = 4
MAX = 19000                       # < DUTY_DENOM (20000); firmware rejects >=
TICK_MS = 20                      # firmware PWM tick period
SWEEP_SECS = [1.0, 2.0, 3.0, 4.0]  # one-way (on->off) time per LED


def step_for(secs):
    """Slew step that sweeps 0..MAX in `secs`, given the 20 ms firmware tick."""
    ticks = max(1, round(secs * 1000 / TICK_MS))
    return max(1, round(MAX / ticks))


def main():
    port = Port(sys.argv[1] if len(sys.argv) > 1 else "/dev/ttyACM0")
    port.drain(0.3)               # flush stale
    reader = FrameReader()

    steps = [step_for(s) for s in SWEEP_SECS]
    target = [MAX] * NUM_LEDS      # each LED's active goal; all start sweeping up
    pending = {}                   # txn -> led index (one outstanding per LED)
    txn = 1

    def send(led):
        nonlocal txn
        t = txn
        txn = txn % 60000 + 1      # keep clear of the shutdown range below
        pending[t] = led
        port.write(frame(t, SET_SERVO, servo_payload(led, target[led], steps[led])))

    def snap_off():
        """Instant off for every LED (step=0); wait ~1s for an ACK per LED."""
        want = {60001 + led: led for led in range(NUM_LEDS)}
        for t, led in want.items():
            port.write(frame(t, SET_SERVO, servo_payload(led, 0, 0)))
        got, deadline = set(), time.monotonic() + 1.0
        while want.keys() - got and time.monotonic() < deadline:
            for t, f, _ in reader.push(port.drain(0.05)):
                if f == ACK and t in want:
                    got.add(t)
        return [want[t] for t in want.keys() - got]   # LEDs that never ACKed

    # kick off: every LED sweeps up; they diverge in phase as they report back.
    for led in range(NUM_LEDS):
        send(led)
    print("breathing 4 LEDs... Ctrl+C to snap off, twice to force quit")

    try:
        while True:
            for t, f, _ in reader.push(port.drain(0.05)):
                # done reply for an outstanding sweep -> reverse that LED.
                if f == SET_SERVO and t in pending:
                    led = pending.pop(t)
                    target[led] = MAX - target[led]   # flip 0 <-> MAX
                    send(led)
    except KeyboardInterrupt:
        print("\nsnapping off...")
        try:
            missing = snap_off()
            print("all LEDs off" if not missing
                  else f"timed out, no ACK for LEDs {missing}")
        except KeyboardInterrupt:
            print("\nforce exit")
            sys.exit(130)


if __name__ == "__main__":
    main()
