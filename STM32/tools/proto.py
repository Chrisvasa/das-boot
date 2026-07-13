"""Shared das-boot protocol helpers for host-side tools.

Frame: [sync=0xAB][txn:u16 LE][func:u8][len:u8][payload:len][crc16:u16 LE]
CRC:   CRC-16/MODBUS over sync..payload, little-endian on the wire.
"""
import os
import select
import time
import tty

SYNC = 0xAB

# control codes (device -> host replies)
ACK = 0x06
NACK = 0x10

# command codes (host -> device)
PING = 0x15
SET_SERVO = 0x20
GET_SERVO = 0x21
GET_SERVO_ALL = 0x22

# NACK reason codes
INVALID_FUNC = 0x05
INVALID_PAYLOAD = 0x06
INVALID_PAYLOAD_LEN = 0x07
INVALID_SERVO_ID = 0x08
INVALID_SERVO_DUTY = 0x09
VECTOR_ERROR = 0x10


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


def servo_payload(num: int, target: int, step: int | None = None) -> bytes:
    """SET_SERVO payload: [num][target:u16 LE] (+ [step:u16 LE] if given)."""
    p = bytes([num]) + target.to_bytes(2, "little")
    if step is not None:
        p += step.to_bytes(2, "little")
    return p


def parse_servo_status(payload: bytes) -> dict[int, tuple[int, int, int]]:
    """Decode a GET_SERVO reply -> {servo_index: (current, target, step)}.

    Payload: [mask:u8][ per set bit, ascending: current:u16, target:u16, step:u16 ].
    The leading mask says which servos are present; walk its set bits in order.
    """
    if not payload:
        return {}
    mask, off, out = payload[0], 1, {}
    for i in range(8):
        if mask & (1 << i):
            cur = int.from_bytes(payload[off:off + 2], "little")
            tgt = int.from_bytes(payload[off + 2:off + 4], "little")
            stp = int.from_bytes(payload[off + 4:off + 6], "little")
            out[i] = (cur, tgt, stp)
            off += 6
    return out


def parse_frames(data: bytes):
    """All complete, CRC-valid frames in a buffer -> [(txn, func, payload)]."""
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
