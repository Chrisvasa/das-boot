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
  0x10  NACK   rejected                (len = 1, payload = reason)
Commands (host → device):
  0x15  PING          liveness check              (len = 0)
  0x20  SET_SERVO     move a servo                (len = 3 or 5)
  0x21  GET_SERVO     read servo state by mask    (len = 1)
  0x22  GET_SERVO_ALL read all servos             (len = 0)
  (battery / engine commands: TBD)

NACK reasons:
  0x05  invalid_func         (unknown command code)
  0x06  invalid_payload
  0x07  invalid_payload_len  (wrong length for this func)
  0x08  invalid_servo_id     (servo index / mask bit out of range)
  0x09  invalid_servo_duty   (target out of range)
  0x10  vector_error         (reserved; device-side buffer/build failure —
                              not currently emitted: buffers are sized to fit,
                              so a build failure panics instead of NACKing)
```
Control and command codes are distinct value ranges. An *inbound* control code
(host sending ACK/NACK) is treated as a no-op — the device sends no reply.

### Per-command payloads
Multi-byte fields little-endian; frame `crc16` omitted below for brevity.

**PING** `0x15` — payload `[]`. Liveness check; device replies `ACK`. Nothing else.

**SET_SERVO** `0x20` — payload `[num:u8][target:u16]` or `[num:u8][target:u16][step:u16]`.
- `num`: servo index `0..N` (`N = 4`). `target`: duty in units of 1/20000.
- `step`: per-20 ms-tick slew increment. Omitted or `0` → jump straight to target.
- Replies `ACK` on accept, then a **deferred terminal reply** once the slew
  completes: `[AB][txn][0x20][2][current:u16]` (the final position).
- NACKs: `invalid_payload_len` (len ∉ {3,5}), `invalid_servo_id` (`num ≥ N`),
  `invalid_servo_duty` (`target ≥ 20000`).

**GET_SERVO** `0x21` — payload `[mask:u8]`. Bit *i* selects servo *i*; valid bits
`0..N`. Device builds its own reply (no `ACK`):
```
[AB][txn][0x21][len][ mask:u8 ][ per set bit, ascending: current:u16, target:u16, step:u16 ]
```
The leading `mask` echoes which servos are present so the reply is
self-describing; walk its set bits to map each 6-byte triple to a servo.
- NACKs: `invalid_payload_len` (len ≠ 1), `invalid_servo_id` (mask has a bit
  ≥ N, or mask == 0).

**GET_SERVO_ALL** `0x22` — payload `[]`. Convenience form of GET_SERVO for every
servo. Device builds its own reply (no `ACK`), echoing the **original** func code
so it's self-describing:
```
[AB][txn][0x22][len][ per servo, ascending: current:u16, target:u16, step:u16 ]
```
Unlike GET_SERVO, the reply carries **no leading mask** — it's always every
present servo (`0..N`) in ascending order, so `len` is exactly `6 * N`. No NACKs —
it takes no payload, so there's nothing to reject.

### Exchange model (per txn)
Each command handler decides its own reply behaviour — the dispatcher only sends
a default `ACK`/`NACK` for handlers that ask it to.

1. Host sends a request frame.
2. Device validates and dispatches by func code. A handler returns one of:
   - **accept, auto-ACK** → dispatcher sends `ACK` `[AB][txn][0x06][0]`
     (PING, SET_SERVO).
   - **reject** → dispatcher sends `NACK` `[AB][txn][0x10][1][reason]`.
   - **handler owns the reply** → dispatcher stays silent; the handler has
     already queued its own frame (GET_SERVO / GET_SERVO_ALL send their data
     frame directly).
   - inbound control codes and bad-CRC frames get **no reply**.
3. Some commands add a later **terminal reply** using the **original** func code
   (self-describing), so a txn can span time:
   - SET_SERVO: after the slew finishes → `[AB][txn][0x20][2][current:u16]`.
   - GET_SERVO / GET_SERVO_ALL: the data frame in step 2 is itself the terminal reply.

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
