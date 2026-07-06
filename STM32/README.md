# das-boot firmware (STM32F411)

Embassy async firmware for a WeAct "Black Pill" (STM32F411CE). Talks to a host
over USB CDC-ACM (`/dev/ttyACMx`).

## USB Command Protocol (draft v0.1)

### Transport
- USB CDC-ACM, point-to-point (no bus address).
- USB guarantees integrity (hw CRC + retransmit) → no application CRC.
- Binary, packet-framed. Multi-byte fields are **little-endian**.

### Frame (same shape both directions)
```
[ sync:u8 ][ txn:u16 ][ func:u8 ][ len:u8 ][ payload: len bytes ]
```

| Field   | Size | Meaning                                            |
|---------|------|----------------------------------------------------|
| sync    | 1    | Constant `0xAB`. Frame start / resync marker.      |
| txn     | 2    | Transaction id, host-chosen. Echoed in replies.    |
| func    | 1    | Function code (see below). Single shared namespace. |
| len     | 1    | Payload length, 0–255.                              |
| payload | len  | Function-specific data.                            |

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

### Robustness
- Resync: on bad sync or len, scan forward to the next `0xAB`.
- Device clears its RX accumulator on each new host connection.
- txn is host-owned; device only echoes it.

### Open questions
- Final func codes per subsystem (battery / servo / engine).
- Any commands need progress/streaming replies, or is ACK + terminal enough?
- Max payload size actually needed (drives buffer sizing).

## Toolchain
See [memory: embedded-debug-setup] — probe-rs 0.31 + clone ST-Link. Always
flash-and-debug (clone can't cold-attach to a sleeping core); if the chip
wedges, BOOT0 recovery + `probe-rs erase --allow-erase-all`.
