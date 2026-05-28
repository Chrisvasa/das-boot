# PCB Hardware

Concrete part options for the STM32 carrier board / power-delivery PCB. See [`hardware-plans.md`](./hardware-plans.md) for the broader architecture context.

## Design constraints

- **Vin range:** 3S LiPo sag from ~12.6V (charged) down to ~8.5V (loaded near empty). Allow headroom to ~16V for transients.
- **Rails needed:** 5V @ ≥3A (Rpi + STM32), 6V @ 5–10A (servos), 11.1V (motor via ESC).
- **Module vs chip-down:** module = drop-in board with through-hole pins, easiest first design. Chip-down = buck IC + L + caps on the PCB, smaller and cheaper at scale. v1 module, v2 chip-down is a sensible progression.

## 5V buck — Rpi 4B + STM32 (target ~4–5A)

### Modules

| Part | Iout | Vin | Form factor | Price |
|---|---|---|---|---|
| **Pololu D36V50F5** | 5A | 6–50V | TH pins, 25×17 mm | ~$25 |
| Pololu D36V28F5 | 3.2A | 6–50V | TH pins, 17×11 mm | ~$15 |
| Murata OKI-78SR-5 | 1.5A | 7–36V | TO-220-style, drop-in for 7805 | ~$10 |
| MP1584 module (generic) | ~2.5A real | 4.5–28V | Tiny PCB | ~$2 |

D36V50F5 is the safe default. MP1584 modules are fine for prototyping but noisy and the 3A spec is optimistic.

### Chip-down

| Part | Iout | Vin | Notes |
|---|---|---|---|
| **TI LMR33630** | 3A | 3.8–36V | HSOP-8, integrated FETs, very simple BOM. WEBENCH generates inductor/caps. |
| TI TPS54360 | 3.5A | 4.5–60V | Older, well-documented, lots of reference designs |
| MPS MP2315 | 3A | 4.5–24V | Cheap, small, common in Chinese designs |
| TI LM5145 | controller | up to 75V | External FETs — overkill, but use this if you want >5A |

For Pi 4B + STM32 the **LMR33630** is the sweet spot — switches at 1–2 MHz so the inductor is tiny, and TI's tools generate the whole BOM.

## 6V buck — servos (target ~5–10A)

Size for stall, not average. 4 servos in unison can spike to 6–10A momentarily.

### Modules

| Part | Iout | Vout | Vin | Price |
|---|---|---|---|---|
| **Pololu D24V90F6** | 9A | 6V | 4.5–22V | ~$30 |
| Pololu D36V50F6 | 4.5A | 6V | 6–50V | ~$25 |
| Hobbywing UBEC 8A | 8A | 5/6V switchable | 2–6S LiPo | ~$15 |
| Castle BEC 2.0 14A | 14A | adjustable | 2–12S | ~$35 |

D24V90F6 if you want one part to never sweat. Castle BEC is what serious RC people use but it's a finished hobby module, not really board-mountable.

### Chip-down

| Part | Iout | Vin | Notes |
|---|---|---|---|
| **TI TPS54824** | 8A | 4.5–17V | Just covers 3S charged; tight margin |
| TI LM5146 / LM5145 | controller | 5.5–75V | External FETs, scale to whatever current you want |
| TI TPS54561 | 5A | 4.5–60V | Easy, well-supported |
| MPS MP4560 | 5A | 4.7–55V | Cheap, single chip |

For chip-down 6V at high current, lean **LM5146 controller + external FETs** — gives margin and the FETs handle the heat, not the IC.

## ESC — main motor

ESC sits external to the PCB; the board just provides battery + control signal. Including here so the part choice can be locked alongside the rest.

### Brushed (simpler firmware story)

| ESC | Continuous | Input | Bidir | Price |
|---|---|---|---|---|
| **Hobbywing Quicrun 1060** | 60A | 2–3S | yes (car ESC) | ~$25 |
| Cytron MD30C | 30A | 5–30V | yes | ~$25 |
| Pololu G2 18v25 | 25A | up to 30V | yes | ~$30 |
| BTS7960 module (generic) | ~20A real | 5.5–27V | yes | ~$8 |

Quicrun 1060 is the standard cheap-and-cheerful pick. BTS7960 modules are tempting at $8 but the heatsinks are undersized and the clone FETs are inconsistent.

### Brushless (boat/marine — best fit for a sub)

| ESC | Continuous | Input | Notes | Price |
|---|---|---|---|---|
| **Hobbywing Seaking 30A V3** | 30A | 2–3S | water-cooled passages, anti-corrosion coating, native bidir | ~$40 |
| Hobbywing Seaking 60A V3 | 60A | 2–6S | bigger sibling | ~$70 |
| Flycolor WinDragon 30A | 30A | 2–3S | cheaper Seaking alternative | ~$30 |
| Flier Boat ESC 50A | 50A | 2–6S | popular in DIY ROV builds | ~$40 |

**Seaking 30A V3** is the default in hobby submarine builds. 30A is overkill for 1 m/s but smaller marine ESCs don't really exist; not paying much for the headroom.

## v1 board BOM (modules, ~$95)

The "order today, start laying out tomorrow" combo:

- 5V: **Pololu D36V50F5** ($25)
- 6V: **Pololu D24V90F6** ($30)
- ESC: **Hobbywing Seaking 30A V3** ($40), external to the PCB

Validates the architecture without burning weeks on chip-down regulator design. Once the system runs, v2 can replace the modules with chip-down designs (LMR33630 for 5V, LM5146-controller for 6V) and shrink the board significantly.

## Board-level concerns (not part selection but related)

- **Inline fuses per rail** from battery, sized just above expected continuous current
- **Bulk capacitor (470–1000 µF)** on the servo rail to absorb stall transients
- **TVS diode + fuse** at the Pi 5V input (GPIO has no overcurrent/overvoltage protection)
- **Voltage divider to STM32 ADC** for battery monitoring (per-cell via balance lead is better than pack-only)
- **Reverse-polarity protection** on Vbatt input (P-channel MOSFET in series, or a Schottky if you accept the drop)
- **Ground plane** continuous under all switching regulators; keep switch-node loops small
- **SWD header** for STM32 programming/debug (5 pins: SWDIO, SWCLK, NRST, GND, 3.3V)

## Open part-selection questions

- Brushed vs brushless motor — decide before finalizing the ESC line
- Final servo voltage (6V vs 7.4V HV) — affects whether D24V90F6 fixed-6V is the right module
- Number of servos — drives the 6V rail sizing
