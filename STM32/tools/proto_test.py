#!/usr/bin/env python3
"""das-boot protocol tester: sends frames and verifies ACK/NACK replies.

Firmware reply rules:
  known command (SetServo 0x20) + valid CRC -> ACK   [func=0x06 len=0]
  unknown func + valid CRC                  -> NACK  [func=0x15 len=1 payload=reason]
  incoming func Ack(0x06)/Nack(0x15)        -> no reply (host shouldn't send these)
  bad CRC                                   -> no reply

Frame: [sync=0xAB][txn:u16 LE][func:u8][len:u8][payload:len][crc16:u16 LE]
CRC:   CRC-16/MODBUS over sync..payload (little-endian on the wire).

Usage: python3 proto_test.py [/dev/ttyACM0]
Exit:  0 = all pass, 1 = one or more failures.
"""
import os
import select
import sys
import time
import tty

SYNC = 0xAB
ACK, NACK, SET_SERVO = 0x06, 0x15, 0x20
REASON_INVALID_FUNC = 0x05


def crc16_modbus(data: bytes) -> int:
    crc = 0xFFFF
    for b in data:
        crc ^= b
        for _ in range(8):
            crc = (crc >> 1) ^ 0xA001 if (crc & 1) else (crc >> 1)
    return crc & 0xFFFF


def frame(txn: int, func: int, payload: bytes = b"") -> bytes:
    body = bytes([SYNC]) + txn.to_bytes(2, "little") + bytes([func, len(payload)]) + payload
    return body + crc16_modbus(body).to_bytes(2, "little")


def parse_frames(data: bytes):
    """Pull all complete, CRC-valid frames out of a buffer -> [(txn, func, payload)]."""
    out, i = [], 0
    while i < len(data):
        if data[i] != SYNC:
            i += 1
            continue
        if i + 5 > len(data):
            break
        total = data[i + 4] + 7
        if i + total > len(data):
            break
        f = data[i:i + total]
        if crc16_modbus(f[:-2]) == int.from_bytes(f[-2:], "little"):
            out.append((int.from_bytes(f[1:3], "little"), f[3], bytes(f[5:-2])))
            i += total
        else:
            i += 1
    return out


class Port:
    def __init__(self, path):
        self.fd = os.open(path, os.O_RDWR | os.O_NOCTTY)
        tty.setraw(self.fd)                 # cfmakeraw: no echo / no translation
        os.set_blocking(self.fd, False)

    def write(self, data: bytes):
        while data:
            data = data[os.write(self.fd, data):]

    def drain(self, window=0.3) -> bytes:
        buf, end = b"", time.monotonic() + window
        while True:
            remaining = end - time.monotonic()
            if remaining <= 0:
                break
            if select.select([self.fd], [], [], remaining)[0]:
                buf += os.read(self.fd, 512)
        return buf


fails = 0


def check(label, ok, detail=""):
    global fails
    tag = "PASS" if ok else "FAIL"
    print(f"  [{tag}] {label}" + (f"  ({detail})" if detail and not ok else ""))
    if not ok:
        fails += 1


def main():
    port = Port(sys.argv[1] if len(sys.argv) > 1 else "/dev/ttyACM0")
    port.drain(0.3)  # flush anything stale

    def expect_reply(label, txn, req, exp_func, exp_payload):
        port.drain(0.05)
        port.write(req)
        replies = [f for f in parse_frames(port.drain()) if f[0] == txn]
        if not replies:
            check(label, False, "no reply")
            return
        _, func, payload = replies[0]
        check(label, func == exp_func and payload == exp_payload,
              f"got func={func:#04x} payload={payload.hex()}")

    def expect_silence(label, req):
        port.drain(0.05)
        port.write(req)
        replies = parse_frames(port.drain())
        check(label, not replies, f"unexpected {[(t, hex(f)) for t, f, _ in replies]}")

    print("1. known command -> ACK")
    expect_reply("SetServo -> ACK", 1, frame(1, SET_SERVO, bytes([90])), ACK, b"")

    print("2. unknown func -> NACK(InvalidFunc)")
    expect_reply("unknown -> NACK", 2, frame(2, 0x99), NACK, bytes([REASON_INVALID_FUNC]))

    print("3. incoming ACK -> no reply")
    expect_silence("host ACK ignored", frame(3, ACK))

    print("4. incoming NACK -> no reply")
    expect_silence("host NACK ignored", frame(4, NACK))

    print("5. bad CRC -> no reply")
    bad = bytearray(frame(5, SET_SERVO, bytes([1])))
    bad[-1] ^= 0xFF
    expect_silence("bad CRC ignored", bytes(bad))

    print("6. leading garbage + valid -> ACK (resync)")
    expect_reply("resync -> ACK", 6, bytes([0, 1, 2, 3]) + frame(6, SET_SERVO), ACK, b"")

    print("7. two frames -> two ACKs (txn 7,8)")
    port.drain(0.05)
    port.write(frame(7, SET_SERVO) + frame(8, SET_SERVO))
    txns = sorted(t for t, f, _ in parse_frames(port.drain()) if f == ACK)
    check("two ACKs", txns == [7, 8], f"got {txns}")

    print("8. split frame -> ACK")
    f9 = frame(9, SET_SERVO, bytes([1, 2]))
    port.drain(0.05)
    port.write(f9[:4])
    time.sleep(0.2)
    port.write(f9[4:])
    check("split -> ACK", any(t == 9 and f == ACK for t, f, _ in parse_frames(port.drain())))

    print("9. flood 20 -> 20 ACKs")
    port.drain(0.05)
    port.write(b"".join(frame(100 + i, SET_SERVO, bytes([i])) for i in range(20)))
    acks = sorted(t for t, f, _ in parse_frames(port.drain(1.0)) if f == ACK and 100 <= t < 120)
    check("20 ACKs", acks == list(range(100, 120)), f"got {len(acks)}")

    print("10. garbage (no sync) then valid -> ACK (recovery)")
    port.write(bytes([0x00, 0x10, 0x20, 0x30]))
    port.drain(0.2)
    expect_reply("recovery -> ACK", 200, frame(200, SET_SERVO), ACK, b"")

    print(f"\n{'ALL PASS' if fails == 0 else f'{fails} FAILED'}")
    sys.exit(1 if fails else 0)


if __name__ == "__main__":
    main()
