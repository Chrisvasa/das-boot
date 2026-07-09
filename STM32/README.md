# das-boot firmware (STM32F411)

Embassy async firmware for a WeAct "Black Pill" (STM32F411CE). Talks to the host
(Raspberry Pi) over **UART on the GPIO header** in production; USB CDC-ACM
(`/dev/ttyACMx`) is kept as a bench/dev console.

## Command Protocol (draft v0.3)

### Transport
- Primary: 3.3 V UART, point-to-point (STM TX↔Pi RX, RX↔TX, GND). No bus address.
- Dev: USB CDC-ACM (same frames), for laptop testing.
- UART has **no link-layer integrity** → application **CRC16 is required**.
  (Over USB it's redundant but harmless; keep it so one parser serves both.)
- Binary, packet-framed. Multi-byte fields are **little-endian**.

### Frame (same shape both directions)
```
[ sync:u8 ][ txn:u16 ][ func:u8 ][ len:u8 ][ payload: len bytes ][ crc16:u16 ]
```

| Field   | Size | Meaning                                            |
|---------|------|----------------------------------------------------|
| sync    | 1    | Constant `0xAB`. Frame start / resync marker.      |
| txn     | 2    | Transaction id, host-chosen. Echoed in replies.    |
| func    | 1    | Function code (see below). Single shared namespace. |
| len     | 1    | Payload length, 0–255.                              |
| payload | len  | Function-specific data.                            |
| crc16   | 2    | CRC-16/MODBUS over `sync..=last payload byte`.      |

### CRC16
- Algorithm: **CRC-16/MODBUS** (poly `0x8005`, init `0xFFFF`, refin/refout, xorout `0x0000`).
- Coverage: the **entire message including the sync byte** — every byte from
  `sync` through the last payload byte (i.e. all bytes before the crc field).
- On the wire: little-endian (low byte first), consistent with `txn`.
- Device: `crc16` crate, `crc16::State::<crc16::MODBUS>::calculate(...)`. Host: any
  CRC-16/MODBUS impl (the `tools/proto_test.py` tester implements it inline).
  Must match byte-for-byte on both ends.

### Function codes
```
Control (device → host replies):
  0x06  ACK    command accepted        (len = 0)
  0x15  NACK   rejected                (len = 1, payload = reason)
Commands (host → device):
  0x20  SET_SERVO
  (battery / engine commands: TBD)

NACK reasons:
  0x05  invalid_func      (unknown command code)
  0x06  invalid_payload
```
Control and command codes are distinct value ranges. An *inbound* control code
(host sending ACK/NACK) is treated as a no-op — the device sends no reply.

### Exchange model (per txn)
**Implemented now:**
1. Host sends a request frame.
2. Device replies immediately with the acceptance result:
   - `ACK`  `[AB][txn][0x06][0]` — command accepted
   - `NACK` `[AB][txn][0x15][1][reason]` — rejected (e.g. unknown func)
   - inbound control codes and bad-CRC frames get **no reply**.

**Planned (needs command dispatch):**
3. On completion, device sends one terminal reply using the **original** func
   code (self-describing):
   - data: `[AB][txn][<orig func>][len][payload]`
   - done: `[AB][txn][<orig func>][0]`

   so each txn ends with exactly one terminal reply (data / done / NACK).

(Frame examples omit the trailing `crc16` for brevity — it's always present.)

### Robustness
- CRC fail → **drop silently, no reply** (txn/func can't be trusted). Then resync.
- Resync: scan forward from `last_sync + 1` to the next `0xAB` (must advance past
  the byte already tried, or you re-lock the same bad sync forever). A `0xAB` can
  occur inside a payload — CRC is the real alignment check, not the sync byte.
- NACK is only sent for frames that **pass CRC** but fail semantics (bad func,
  queue full, …) — those have a trustworthy txn to address the reply to.
- txn is host-owned; device only echoes it.
- _(planned)_ Mid-frame stall watchdog: drop + resync if a frame stalls mid-way.
- _(planned)_ Clear RX state on each new host connection.

### Open questions
- Final func codes per subsystem (battery / servo / engine).
- Any commands need progress/streaming replies, or is ACK + terminal enough?
- Max payload size actually needed (drives buffer sizing).

## Implementation
- Transport is a build feature: `usb` (default, CDC-ACM) or `uart`
  (`--no-default-features --features uart`). Both expose the same `Reader`/`Writer`
  to a transport-agnostic parser — see `src/transport.rs`, `src/parser.rs`.
- Parser design notes: `RX_PARSER.md`.
- Protocol tester (sends frames, asserts ACK/NACK replies): `tools/proto_test.py`.

## Toolchain
See [memory: embedded-debug-setup] — probe-rs 0.31 + clone ST-Link. Always
flash-and-debug (clone can't cold-attach to a sleeping core); if the chip
wedges, BOOT0 recovery + `probe-rs erase --allow-erase-all`.
