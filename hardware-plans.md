# Hardware Plans

Rough notes on the electronics + mechanical decisions for the sub. Subject to change as we actually start building.

## Overall architecture

- 3S LiPo (11.1V nominal) as the single onboard battery
- Engine runs directly off the 11.1V rail via an ESC
- 7.7V buck → servos (see voltage note below)
- 5V buck → Rpi + STM32
- Rpi: networking, camera, LEDs. Talks to surface via tethered Ethernet to a WiFi buoy (see Surface buoy power section for how the buoy is powered).
- STM32: realtime control of ESC, servos, ballast piston

## Hull material

| Material | Pros | Cons | Realistic depth |
|---|---|---|---|
| 3D printed (PLA/PETG) | Fully custom geometry, integrated mounts, fast iteration | Layer lines = leak paths, weak axial strength, almost always needs an epoxy / XTC-3D coating | ~3 m, optimistically |
| PVC pipe | Cheap, plumbing-grade pressure rating, off-the-shelf caps with gaskets | Opaque (need separate camera viewport), hard to mount internals (need a sled) | ~50–70 m (sched 40) |
| Aluminum tube | Strongest, dissipates electronics heat into water | Heavy, needs machine tools for end caps, galvanic corrosion with dissimilar metals, hard to inspect inside | 100 m+ easily |
| **Clear acrylic / polycarbonate** | Transparent (camera looks straight through wall, no viewport needed), pre-made cylinders with matched O-ring end caps | More expensive, scratches, brittle if dropped | 10–30 m depending on wall thickness |

**Recommendation: clear acrylic tube.** ~$15–30 for raw tube, $30–80 for machined end caps if no lathe access. Transparent hull eliminates the hardest part (the camera viewport), and O-ring end-cap engineering is well-understood. PVC is the budget runner-up. 3D-printed hulls are a research project in waterproofing, not a sub project.

## STM32 selection

Going custom-board with a chip-down STM32 is straightforward — substantially simpler than the ESP32-S3-MINI experience because **there's no RF**: no antenna matching, no 40 MHz crystal, no keepout zones, no FCC concerns.

### Clocks: HSI is enough

Every STM32 has an internal RC oscillator (HSI) that's factory-trimmed to ±1%. For this project (PWM, UART up to ~1 Mbaud, GPIO, ADC), HSI is fine — no external crystal needed at all. Cases where you'd want an external HSE crystal:

- USB host (USB device on F4 has clock recovery against the SOF and can run from HSI)
- CAN bus
- Anything needing sub-100 ppm timing

None of those apply here.

### Minimal chip-down BOM

- 100 nF decoupling per VDD pin + one 4.7 µF bulk
- VBAT to VDD (or to a 100 nF cap and a coin cell if you want RTC backup)
- NRST to VDD via 10 kΩ + 100 nF to GND
- BOOT0 pulled to GND via 10 kΩ (with a test point or jumper to pull it high for DFU bootloader)
- SWD header: SWDIO, SWCLK, NRST, GND, 3.3V (5 pins)
- That's it. No crystal, no oscillator can.

### Chip recommendations

| Chip | Package | Why | Price (chip) |
|---|---|---|---|
| **STM32F411CEU6** | UFQFPN48 0.5mm | Sweet spot for this project. 100 MHz M4+FPU, plenty of timers/UARTs, hand-solderable. | ~$5 |
| STM32F405RGT6 | LQFP64 0.5mm | Popular in flight controllers, lots of reference layouts to crib from. Bigger than you need. | ~$8 |
| STM32G431CBU6 | UFQFPN48 | Newer, motor-control-oriented (HRTIM, op-amps, comparators). Future-proof if rolling own ESC. | ~$6 |
| STM32F103C8T6 | LQFP48 0.8mm | Cheapest, biggest pitch (easiest hand soldering), end-of-life from ST. | ~$3 |

**Pros of chip-down:**
- Smallest footprint, fits a 5–10 cm hull
- Pick exactly the package and peripherals you need
- One-PCB build

**Cons of chip-down:**
- Reflow or careful hand-soldering for QFN/UFQFPN
- One bad solder joint = brick the board
- Slower iteration than swapping a dev board

**Alternative: socket a Black Pill as a sub-module.** Castellated edges or female headers; treat it like a module. Tradeoff: ~5× the board area used by the MCU vs chip-down, but faster iteration and unbrickable. Sensible for v1; switch to chip-down for v2.

## Servos

Hobby servos use a specific pulse-width-position scheme (not duty cycle):

- 50 Hz frame (20 ms period)
- 1.0 ms pulse = one end of travel
- 1.5 ms = center
- 2.0 ms = other end
- Some extend to 0.5–2.5 ms

STM32 timer config: prescale so each tick = 1 µs, `ARR = 20000`, `CCRx = 1000–2000`. One timer drives multiple servos via separate channels.

3.3V GPIO on signal line is accepted directly by typical hobby servos — no level shifter needed.

### Voltage warning

The 7.7V servo rail is too high for standard analog servos (4.8–6V). Either:

- Spec HV digital servos (6–7.4V), or
- Drop that buck output to ~6V

### Recommendation: keep servos inside the dry hull

Run linkage rods through sealed glands out to the control surfaces. Saves ~$30/servo vs waterproof models, and "waterproof" hobby servos are typically only IP67 (splash/dust) — not rated for pressure-cycled submersion.

### Internal (dry) servo options

| Servo | Torque | Weight | Price |
|---|---|---|---|
| SG90 (plastic gear) | 1.8 kg·cm | 9 g | $2–3 |
| MG90S (metal gear) | 2.2 kg·cm | 13 g | $4–6 |
| MG996R (full size, metal) | 11 kg·cm | 55 g | $5–8 |
| Savöx SH-0255MG (mini digital) | 4 kg·cm | 16 g | $25–35 |

**Pros of cheap (SG90/MG90S):** small, light, plenty of options, easy to replace when (not if) one fails.
**Cons:** plastic gear strips under shock load; analog control loop is twitchy.

### Waterproof servo options (only if external-mount required)

| Servo | Notes | Price |
|---|---|---|
| Hitec D85MG | micro, IP67 | $30–40 |
| Savöx SW-0231MG | ~5 kg·cm, IP67 | $40–50 |
| Traxxas 2080X / 2255 | RC boat/crawler grade | $40–80 |
| Blue Bird BMS-101DMH | digital, waterproof | $30–40 |

## ESC + motor

Decision: **buy a hobby ESC**, don't build one.

- Speaks the same 1–2 ms PWM as servos, so STM32 firmware treats it identically
- Pick a **boat or car ESC** for native bidirectional throttle. Drone ESCs are unidirectional unless flashed with BLHeli in bidirectional mode.
- Rolling our own H-bridge is only worth it for a brushed motor; BLDC controller from scratch is its own project

### Power target

1 m/s on a ~1.5 L sub needs roughly 20–50 W in the water. Tiny.

### Brushed combo (~$30–50)

| Part | Example | Price |
|---|---|---|
| Motor | Mabuchi RS-540 or generic 540 | $10–15 |
| ESC | Hobbywing Quicrun 1060 (60A brushed) | $15–25 |

**Pros:** dead-simple control, cheap, ESC handles direction. Trivial to drive from STM32.
**Cons:** brushes wear, commutator sparks, doesn't love water ingress. Need a well-sealed motor or a sacrificial cheap one.

### Brushless combo (~$50–80)

| Part | Example | Price |
|---|---|---|
| Motor | 2838/2840 inrunner, ~1000 KV | $20–30 |
| ESC | Hobbywing Seaking 30A (boat ESC) | $30–50 |

**Pros:** sealed-enclosure-friendly, longer life, more efficient. Boat ESCs are designed for marine use (water cooling, anti-corrosion coating).
**Cons:** more expensive, ESC arming sequence is fussier, reverse is slightly slower to engage.

Leaning brushless for a sub build — the sealing + life advantages matter more than the cost delta.

## Buck converters

### Topology: parallel, never series

Each buck taps the battery directly and produces its own independent rail:

```
3S battery ─┬─→ 5V buck   → Rpi + STM32
            ├─→ 6V buck   → servo rail
            └─→ ESC       → motor
```

Series (one buck feeding the next) compounds losses and propagates failures.

### Sizing

| Rail | Min current | Recommended buck | Examples |
|---|---|---|---|
| 5V (Rpi 4B/CM4 + STM32) | 3A | 4–5A | Pololu D36V28F5 ($15), Hobbywing UBEC 5A ($15), MP1584 module ($3, noisier) |
| 6V or 7.4V (servos) | depends on count | 5–10A | Castle BEC Pro 10A, Hobbywing UBEC 8A |

A 4-servo rail can momentarily spike to 6–10A on simultaneous stall. Size for stall, not average.

### Practical notes

- Inline fuse per rail, sized just above expected continuous current
- Bulk capacitor (470–1000 µF) on the servo rail to absorb transients
- Verify input range covers full 3S sag: 12.6V (charged) down to ~9V (loaded near empty)

## Rpi power

Single **5V rail in, that's it.** The Rpi has onboard regulators for 3.3V/1.8V/1.1V — never supply those directly.

| Board | 5V current | Notes | Price |
|---|---|---|---|
| Pi Zero 2 W | ~1A | Smallest. Handles camera + WiFi tether fine. | $15 |
| Pi 4B | 3A peak | Power via GPIO pins 2/4 to bypass USB-C resistor quirk | $35–75 |
| CM4 | 3–5A | Needs a carrier board; 5V direct to carrier | $35–90 |

**Recommendation: Pi Zero 2 W** unless you're doing onboard CV/inference. Saves space, current, and money.

Add an inline fuse + TVS diode at the Pi's 5V input — GPIO has zero overcurrent/overvoltage protection and a buck failure will brick the Pi instantly.

## Ballast piston

Mechanism: syringe(s) driven by a small geared DC motor + leadscrew, with hardware limit switches at both ends.

Leadscrew is self-locking → piston holds position without holding torque.

### Sizing formula

Required displacement is a fraction of hull volume:

```
V_ballast ≈ (0.05 to 0.10) × V_hull
```

For a ~30 cm × 8 cm cylindrical hull:

```
V_hull = π × r² × L = π × (4)² × 30 ≈ 1508 cm³ ≈ 1.5 L
V_ballast ≈ 75–150 ml
```

So one 100 ml syringe or two 60 ml syringes.

### Force formula

Water pressure pushes back on the piston face:

```
F = P × A
P = ρ × g × h    (gauge pressure at depth h)
ρ = 1000 kg/m³, g = 9.81 m/s²
```

Worked example for a 20 mm-diameter piston (A = π × (0.01)² ≈ 3.14 × 10⁻⁴ m²) at 5 m depth:

```
P = 1000 × 9.81 × 5 ≈ 49 kPa
F = 49000 × 3.14 × 10⁻⁴ ≈ 15 N
```

Small at shallow depth but grows linearly with depth. Useful sanity check when picking motor + gear ratio.

### Other notes

- Need a water port through the hull for the syringe outlet — one more sealing point
- Limit switches must be hardware, not just firmware. A runaway piston either crushes itself or punches through an end cap.

## Rpi ↔ STM32 link

Decision: **UART**.

- Skip I2C — Rpi's I2C has had clock-stretching bugs that bite STM32 slaves
- SPI is faster but awkward for STM32-initiated telemetry (Rpi is master)
- UART at 115200–921600, DMA on STM32 side, framing with COBS + CRC16
- Debuggable with any USB-UART adapter

## Surface buoy power

Three ways to power the surface WiFi router/extender. Listed simplest first.

### Option A: Separate battery in the buoy (recommended)

Small 1S LiPo in the buoy with a USB-C charge IC (TP4056 or similar) and a buck to whatever the router wants. A 1000 mAh 1S cell weighs ~10 g and runs a Pi-class WiFi extender for several hours.

**Pros:**
- Lowest complexity, fewest custom parts
- Sub-side faults don't take down surface comms — small but real reliability win
- No tether voltage-drop math to worry about

**Cons:**
- Second battery to remember to charge
- Buoy is slightly heavier / more complex mechanically

**Cost: ~$10–20**

### Option B: Passive PoE

Just inject DC onto the unused Ethernet pairs (pins 4/5 = +, 7/8 = −) at the sub end, pull it back off at the buoy end with a passive splitter, then buck down to whatever the router needs. No negotiation, no handshake.

**Pros:**
- Single battery, single charge step
- Off-the-shelf passive injectors/splitters are $3–5
- No 48V boost needed — tether can run at 12V (battery direct) or 24V (small boost) for lower copper loss

**Cons:**
- Non-standard, will damage a non-PoE device plugged in by accident
- Adds another sealing penetration / voltage-drop sensitivity

**Cost: ~$5–15**

#### Tether voltage drop

For ~10 m of Cat5e (≈0.094 Ω/m per conductor, two conductors in loop):

```
R_loop = 2 × L × 0.094     (Ω, L in m)
V_drop = I × R_loop
P_loss = I² × R_loop
```

At 10 m, 1A: 1.88V drop, 1.88W loss. Significant at 12V tether (≈16%), tolerable at 24V (≈8%). Doubling up (both unused pairs in parallel for + and −) halves loop resistance.

### Option C: Active 802.3af/at PoE

Standard PoE: 48V on the line with full PSE↔PD handshake (detection signature → classification → power-up). Designed so non-PoE devices can't be damaged.

**Pros:**
- The standard. Off-the-shelf 802.3af extenders/APs work directly as the surface unit.
- Safest at the connector — proper isolation and negotiation

**Cons:**
- PSE-from-battery is uncommon and adds real complexity: 48V boost, PSE controller IC (TPS23861, LTC4258), isolation magnetics
- Surface side needs a PD controller + buck (e.g. Silvertel Ag9305)
- Easily $30–50 in parts and a non-trivial PCB layout subproject

**Cost: ~$30–50**

### Note on Ethernet hardware

You don't need an Ethernet switch IC anywhere in this design. Rpi has one Ethernet port, router has one WAN port — single Cat5e end-to-end. The piece sometimes called a "switch" in PoE context is actually the **injector** (the bit that combines DC + data onto the cable at one end) and the symmetric **splitter** at the other end.

### Decision

Default: **Option A** (separate buoy battery). Reconsider only if charging two packs becomes annoying in practice.

## Mechanical & physical considerations

### Camera viewport optics

A flat window in water acts as a lens: ~25% reduced FOV and noticeable barrel distortion. A dome port (hemisphere) preserves the camera's air-design FOV but is harder to make. Transparent hull dodges the issue — the cylindrical wall has predictable, software-correctable distortion. If we go opaque hull, plan a dome.

### Buoyancy and trim

- **Self-righting:** center of gravity must be *below* center of buoyancy. In practice: heavy stuff (battery) low and centered, air/foam high. Get this wrong and the sub rolls inverted at rest.
- **Slightly-positive trim:** tune so a dead sub floats. With piston ballast, target neutral when piston is mid-stroke; "dive" pulls water in, "surface" pushes it out.
- **Failsafe direction:** on power loss the piston should *retract* (positive buoyancy → sub surfaces). Either spring-return mechanically or a watchdog-driven dump routine in firmware.

Use internal trim weights once everything is built — don't try to get it perfect on paper.

### Propeller shaft seal

The single hardest mechanical part. Two approaches:

- **Stuffing box** — grease-packed sliding seal around the prop shaft. Classic, leaks a drop or two per dive, works. Cheap.
- **Magnetic coupling** — motor stays inside the dry hull, neodymium magnets on both sides of an unbroken hull wall transfer torque to the external prop shaft. Zero shaft penetration. Limits max torque but for 20–50 W it's fine. Strongly worth considering.

### Thermal management

Sealed hull = no convection. Rpi 4 will throttle. Motor + ESC dump real heat too. Plan thermal paths from hot components to the hull wall — water is a fantastic heat sink if you can reach it. Big argument for aluminum, or a thermal pad from a heatsink to an aluminum end cap in an otherwise-acrylic hull.

### LiPo safety in a sealed pressure vessel

Take this seriously. A pouch fire inside a sealed acrylic tube is bad. Mitigations:

- Stay well within the cell's current rating (we have lots of headroom)
- Per-cell monitoring via the balance lead, not just pack voltage
- Conservative discharge cutoff at 3.3 V/cell in firmware
- Physically restrain the pack so it can't short on something
- Don't charge inside the sealed hull — charge externally

### Tether

- 10 m of Cat5e drags noticeably. Thin shielded cable, possibly with a neutral-buoyancy foam jacket.
- Strain relief at the hull penetration is mandatory — never let pull force reach the connector or gland.
- The tether is **not** a recovery line. It'll detach at the gland under load. Run a separate retrieval cord.

## Budget estimate

Rough per-sub component cost. Excludes tools (soldering iron, oscilloscope, 3D printer, etc.) and PCB fab.

| Item | Low | High |
|---|---|---|
| STM32 (chip + passives) or dev board | $5 | $15 |
| Rpi (Zero 2 W → 4B/CM4) | $15 | $90 |
| Camera module | $10 | $30 |
| 4× servos (internal, dry) | $20 | $200 |
| Motor + ESC | $30 | $80 |
| 2× buck converters | $20 | $40 |
| 3S LiPo + balance charger | $30 | $60 |
| Surface buoy (WiFi router/extender + PoE) | $30 | $80 |
| Hull, end caps, seals, syringes, leadscrew, hardware | $30 | $100 |
| PCB fab + assembly (small run) | $20 | $80 |
| Tether cable (Cat5e or shielded, ~10m) | $10 | $30 |
| **Total** | **~$220** | **~$805** |

Realistic mid-build target: **$350–450**. Hidden cost lives in iterating on seals and end caps until they stop leaking.

## Open questions / TODO

- Hull diameter: 10 cm floor; 5 cm is unrealistic with current component list
- Dive failsafe behavior on comms loss (drop ballast? float up? cut motor?)
- Tether cable gland + prop shaft seal + camera viewport — the actual hard parts
- LiPo low-voltage cutoff + monitoring
- Pressure test plan (submerge with desiccant before any electronics)
- Decide on Rpi model (Zero 2 W vs 4B vs CM4) — drives carrier board design
- Decide on brushed vs brushless motor — drives sealing strategy
- STM32 board form factor: chip-down for v2; Black Pill carrier for v1 prototyping?
- Hull material final pick (acrylic tube vs PVC vs other)
- Prop drive: stuffing box vs magnetic coupling
- Software stack (video pipeline, control daemon, surface UI) — not yet planned
- Test/recovery plan (pressure test procedure, retrieval cord rigging)
