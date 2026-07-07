# das-boot firmware (STM32F411)

Embassy async firmware for a WeAct "Black Pill" (STM32F411CE). Talks to the host
(Raspberry Pi) over **UART on the GPIO header** in production; USB CDC-ACM
(`/dev/ttyACMx`) is kept as a bench/dev console.

## Command Protocol (draft v0.2)

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
- Rust: `crc` crate `Crc::<u16>::new(&crc::CRC_16_MODBUS)`. Host (Python):
  `crcmod`/`libscrc` MODBUS — must match byte-for-byte on both ends.

### Function codes (one enum, must be unique)
```
Control (replies only):
  0x06  ACK    accepted into queue     (len = 0)
  0x15  NACK   rejected                (len = 1, payload = reason)
Commands / queries (0x10+):
  0x20  GET_BATTERY
  0x21  SET_SERVO
  0x22  ...

NACK reasons: 0x01 queue_full  0x02 bad_func  0x03 bad_length  0x04 internal
```

### Exchange model (per txn)
1. Host sends a request frame.
2. Device replies immediately with the queue result:
   - `ACK`  `[AB][txn][0x06][0]` — queued, processing
   - `NACK` `[AB][txn][0x15][1][reason]` — not queued (terminal)
3. On completion, device sends exactly ONE terminal reply using the **original**
   func code (self-describing):
   - data: `[AB][txn][<orig func>][len][payload]`
   - done: `[AB][txn][<orig func>][0]` — completed, no data

Every txn ends with exactly one terminal reply (data / done / NACK).
(Frame examples above omit the trailing `crc16` for brevity — it's always present.)

### Robustness
- CRC fail → **drop silently, no reply** (txn/func can't be trusted). Then resync.
- Resync: scan forward from `last_sync + 1` to the next `0xAB` (must advance past
  the byte already tried, or you re-lock the same bad sync forever). A `0xAB` can
  occur inside a payload — CRC is the real alignment check, not the sync byte.
- NACK is only sent for frames that **pass CRC** but fail semantics (bad func,
  queue full, …) — those have a trustworthy txn to address the reply to.
- Mid-frame stall watchdog: if bytes stop arriving mid-frame for N ms, drop +
  resync. (Framing itself is by `len`, not timing — the timeout is only a safety.)
- Device clears its RX state on each new host connection.
- txn is host-owned; device only echoes it.

### Open questions
- Final func codes per subsystem (battery / servo / engine).
- Any commands need progress/streaming replies, or is ACK + terminal enough?
- Max payload size actually needed (drives buffer sizing).

## Toolchain
See [memory: embedded-debug-setup] — probe-rs 0.31 + clone ST-Link. Always
flash-and-debug (clone can't cold-attach to a sleeping core); if the chip
wedges, BOOT0 recovery + `probe-rs erase --allow-erase-all`.
