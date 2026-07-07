# RX parser design (firmware side)

Notes for implementing the UART receive path. Protocol frame is defined in
[README.md](README.md). This is the firmware-internal parsing strategy.

## Pipeline
```
UART ─► RingBufferedUartRx (DMA)  ─►  drain chunks  ─►  linear parse buffer  ─►  state machine
        true ring, O(1) capture       read()             contiguous [u8], easy to scan/CRC
```
- **DMA ring**: soaks up bytes in the background via interrupt/DMA. Sized for
  drain latency. Overrun = bytes lost = desync (see below).
- **Parse buffer**: a plain `[u8; CAP]` + `len` (NOT a ring). Contiguous, so
  scanning and CRC-over-a-span are trivial (no wraparound math). `CAP = 2 × max frame`.
  - `fill(src)`  — append drained bytes to the end.
  - `view()`     — `&buf[..len]`, parse over this by index.
  - `consume(n)` — drop n bytes from the front (`copy_within` compaction).

## State machine — decision table (each pass does exactly ONE)
Sync byte = `0xAB`. Frame = `[sync][txn:2][func][len][payload:len][crc16:2]`,
total = `6 + len`.

| # | Condition                                         | Action                                    |
|---|---------------------------------------------------|-------------------------------------------|
| 1 | No `0xAB` anywhere in buffer                       | Discard whole buffer                      |
| 2 | `0xAB` at offset k > 0                             | Compact: drop `[0..k]`, sync → front      |
| 3 | Sync at front, < 4 header bytes present            | **Wait for more** (do not consume)        |
| 4 | Header present, `len` > MAX_PAYLOAD                | `consume(1)`, rescan (false sync)         |
| 5 | Header ok, but < `6 + len` bytes present           | **Wait for more** (do not consume)        |
| 6 | Full frame present, CRC ok                         | Dispatch → ACK/NACK/data, `consume(6+len)`|
| 7 | Full frame present, CRC bad                        | `consume(1)`, rescan (never trust bad len)|

Notes:
- **Sanity-check `len` (step 4) BEFORE the complete-vs-incomplete decision**, or a
  corrupt len byte makes you wait forever for a frame that never arrives.
- **Never trust a bad frame's `len`** to skip ahead — advance 1 byte and rescan.
  A real sync can hide inside a bad frame's claimed payload; byte-by-byte scan
  finds it.
- `0xAB` can legitimately appear in payload/txn/crc. Sync is only a *hint*;
  **CRC is the real alignment check.**

## Edge cases
- **Incomplete frame (steps 3/5)** is the only non-consuming branch. Keep the
  partial, `fill()` more on the next drain, re-attempt. Step 2's compaction frees
  tail space so a late-arriving sync still has room for its remainder.
- **Stall watchdog**: if holding a partial with no new bytes for N ms →
  `consume(1)` past the sync + resync. (Framing is by `len`; timeout is only
  a safety against a half-sent frame wedging the parser.)
- **DMA ring overrun**: bytes were lost → desynced by definition. Reset parse
  state, clear the parse buffer, resync.
- **New host connection**: clear parse buffer + state (drop stale partials).

## Replies
- **CRC fail → send NOTHING.** txn/func are untrustworthy on a corrupt frame;
  you can't address a reply. Host times out on that txn and retransmits.
- **NACK only for CRC-valid frames** that fail semantics (bad func, queue full,
  …) — those have a trustworthy txn to reply to.

## The no-deadlock invariant
Every pass either **consumes ≥ 1 byte** (1,2,4,6,7) or is **legitimately waiting
for more data** (3,5, bounded by the watchdog). So the parser always makes
forward progress and recovers from garbage as fast as byte-by-byte rescanning
allows — the property we want for a link that drives servos/engine.

## Efficiency
Only compact when needed (junk before a sync in step 2; after a good frame in
step 6). Parse as far as possible into `view()` before moving bytes. Negligible
cost at expected message rates, but that's the instinct. If memmoves ever matter,
switch to read/write cursors and compact only when `write` hits `CAP`.
