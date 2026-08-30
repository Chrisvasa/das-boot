# PCB Hardware

Locked part selection for the power-delivery / STM32 carrier PCB. See [`hardware-plans.md`](./hardware-plans.md) for the broader architecture context.

## Locked targets (2026-08-28)

| Rail | Spec | Load | Regulator |
|---|---|---|---|
| 6V servo | 6.0V @ 5A | 4× MG90S | **TI TPS56837** (LCSC C22428366) |
| 5V logic | 5.0V @ 3A | Rpi + STM32 + headroom | **TI TPS56837** (same part) |
| 3.3V | 3.3V @ ≤1A | STM32, from 5V rail | LDO (MCP1826S-3302 or similar) |

Both rails are generated in parallel directly from the battery bus — never in series. Deliberately the **same buck IC twice**: one proven design stamped at two operating points, identical BOM except four passives, interchangeable spares. (Originally two different ICs for evaluation; WEBENCH recommending the TPS56837 for both rails settled it.)

The ESC/motor decision is deferred. The ESC taps the battery via a Y-split in the pack lead, **before** the PCB — motor current (up to 30A) never touches the board, so copper, fuse, and connector sizing stays in the ~6–8A range.

### Per-rail circuit values (from TPS5683x datasheet Table 7-2, 500 kHz)

| | FB divider | L (Isat) | C_ff | MODE | EN |
|---|---|---|---|---|---|
| 5V | 73.2k / 10k | 3.3 µH (≥6A) | 150 pF | 30.1k | floating = always on |
| 6V | 90.9k / 10k | 4.7 µH (≥8A) | 100 pF | 30.1k | MCU GPIO + 100k pulldown = **off by default** |

- Shared per rail: 2× 10 µF + 100 nF input, 2× 22 µF 25V X7R output, 100 nF BOOT, 22 nF SS, optional 49.9 Ω loop-injection resistor between VOUT and the FB divider (gain/phase measurement).
- **PG:** U1 (5V) PG → 100k to 5V, board test point only (if firmware runs, 5V is fine). U2 (6V) PG → J5 pin 14, open-drain with **no board pull-up — firmware must enable the STM32 internal pull-up** on that input (keeps the pin at 3.3V). FW contract: PG high within ~2 ms of raising 6V ENA; if it drops while enabled, kill servos.
- 6V EN logic: STM32 GPIO drives EN directly (3.3V > 1.26V max enable threshold, < 6V abs max). The 100k pulldown holds the rail off while the MCU is high-Z (boot, reset, reflash) — every enable ramps through soft-start into the servo bulk cap. L2 Isat ≥ 8A because the current-limit hiccup lets the inductor reach ~8A peak.

## Design constraints

- **Vin design window: 8.0–13.0V.** Covers 3S LiPo (12.6V charged → ~9V loaded at 3.3V/cell cutoff) *and* 3S1P Li-ion (happy down to ~2.75V/cell ≈ 8.25V). The 28V-rated bucks make the window free.
- **Input transient clamp: SMBJ15A** TVS on the common battery bus. Standoff 15V, clamp ~24V — inside the TPS56837's 32V abs max with margin. (This margin is why every 16/17V-class buck was rejected.)
- **Hot-plate assembly.** All active parts are thermal-pad packages.

## Battery input — parallel packs with ideal-diode ORing

Multiple 3S packs in parallel, each individually MCU-disconnectable (low SoC → drop the pack). Direct paralleling is forbidden (circulating current between mismatched-SoC LiPos), and a single FET can't disconnect a battery (body diode). Solution, per battery port:

**TI LM7480x-Q1 ideal-diode controller + back-to-back N-FETs** (one dual-FET SO-8/PowerPAK per port):

- DGATE FET = ideal diode (~20 mV drop): current only ever flows *out* of a pack — mismatched SoC is safe, packs share naturally as they converge. **Replaces the reverse-polarity P-FET** (this stage *is* the reverse protection).
- HGATE FET = series disconnect, driven via EN from the MCU.
- **EN pulls UP to its own pack (100k) = default ON** — opposite polarity to the servo rail. If ports defaulted off, no pack could power the MCU that enables them (the disconnect FET's body diode blocks battery→bus). MCU disconnects a port via a 2N7002 (drain→EN, source→GND, gate→J5 with 100k pulldown); GPIO high = pack off.
- **Firmware rule: never disconnect the last live pack.** MCU dies → pull-ups re-enable → reboot → re-disconnect = brownout oscillation with servos attached. Below last-pack cutoff the action is surface-and-shutdown, not disconnect.
- Pack sensing per port via the LM7480x **SW ladder** (SW → 75k → MON tap → 16.2k → OV tap → 8.87k → GND): monitor ratio ≈ 1/4 to the ADC, OV trip ≈ 13.9V (rejects an accidental 4S pack), and the ladder auto-disconnects when the port is disabled (zero standby drain — but a disabled pack reads 0V, by design).
- **No on-board balance header in v1.** The ideal diodes share load *between* packs; cell balance *inside* each pack is the external balance charger's job. Ops rule: balance-charge every pack after every session — the charger is the per-cell safety check. Per-cell monitoring deferred to v2 (needs ADC channels the J5 ribbon doesn't have).
- v1: 3 ports, XT30 each (2× CSD18540Q5B common-drain, SMAJ15A per port). Identical blocks.

### Bus bulk capacitance

**100–220 µF aluminum electrolytic (35V, low-impedance) on the common battery bus**, in addition to the per-buck ceramics:

- Main reason: **hot-plug damping.** Ceramics + battery-lead inductance ring on connect — worst case ~2× overshoot. The electrolytic's ESR damps the ring; the SMBJ15A catches what's left.
- Pack-switchover hold-up is nearly free: an already-conducting port never opens, and even a ~10 µs gap at 8A only dips 0.36V on 220 µF.
- Re-enable inrush into the bus caps is slew-limited by the LM7480x's controlled HGATE turn-on — no extra capacitance needed for that.

## Battery packs

**Primary: 3S LiPo, 2200 mAh, ≥20C, XT30/XT60 + JST-XH balance lead** (~$20 each).

- ~30W average draw → ~40–60 min per pack; parallel ports scale runtime linearly.
- 2200 mAh × 20C = 44A available per pack; we draw <10%. Mount low for self-righting.

### 3S1P Li-ion swap (test phase)

Three 18650s in series = still "3S" (12.6V charged, ~10.8V nominal); with matching connectors it's plug-in. Vin window already covers the lower ~2.75V/cell floor. Caveat: BMS-protected packs usually omit the balance connector → pack-level-only monitoring (which the per-port dividers provide anyway). Good cell if building: Molicel P28A.

## Buck converter — TPS56837 (both rails)

8A, 4.5–28V synchronous buck, D-CAP3, Eco-mode, VQFN-HR-10 (HotRod) 3×3 mm (~$1.30/1ku). Datasheet: SLVSGM3B.

- 32V abs max input → TVS fits; 8A → 2× margin at servo stall; effective RθJA 30°C/W (4-layer); 98% duty OK from 8V sag; 500 kHz via MODE 30.1k.
- **Variants, pin-to-pin:** TPS56838 = FCCM drop-in if light-load ripple ever matters; TPS56836 = Out-of-Audio. Eco-mode's 45 µA Iq is the right default on battery.
- **Inspection:** HotRod terminals are bottom-only — stencil (0.1 mm, 89% paste on SW/PGND tabs per TI), verify via PG. Route PG to LED/test point.
- Layout: AGND–PGND tie at one point; input ceramics tight between VIN and PGND; thermal vias in the PGND land.

## Rejected candidates (and why)

| Part | Verdict |
|---|---|
| LMR33630 | Fine part, was the 5V pick; dropped for BOM commonality once TPS56837 covered both rails |
| LM61460 | Good part (6A, 36V, wettable flanks), but TPS56837 beats it on price, current headroom, and thermals |
| TPS565242 / TPS565247 | WEBENCH cost-ranked picks at Vmax=14V. 16V rec / 18V abs max: no TVS fits; SOT-563 has no thermal pad |
| TPS565208 | Same 17V-class headroom problem; 5A in SOT-23-6 has no thermal path |
| LM5148 / TPS40305 | Controllers + external FETs: gate-drive layout subproject, 19-part BOM, no benefit at 5A |

## Connectors

| Function | Symbol | Footprint (stock KiCad) |
|---|---|---|
| Battery ports (3×) | Conn_01x02 | `AMASS_XT30PW-M_1x02_P2.50mm_Horizontal` (board = male; battery = female; packs need XT60→XT30 pigtails) |
| Servos (4×) | Conn_01x03 | `PinHeader_1x03_P2.54mm_Vertical` — JR pinout: 1 = signal, 2 = +6V, 3 = GND; identical orientation + silk labels |
| MCU / control (J5) | Conn_02x08 | `PinHeader_2x08_P2.54mm_Vertical` — NB: symbol is Top_Bottom-numbered, footprint pads are odd/even; print the real pinout from layout before crimping |

J5 pinout: 1/3/5 = BAT1/2/3 enable (high = disconnect), 2/4/6 = BAT1/2/3 monitor, 7/15/16 = GND, 8 = 5V, 9–12 = servo signals S1–S4, 13 = 6V ENA (high = servo rail on), 14 = 6V PG (open-drain, enable internal pull-up).

Each servo header gets a local 220 µF 16V polymer cap placed at the connector, so stall transients close their loop locally instead of at the buck.

## Custom KiCad library

Project-local library at the schematic root (`PCB/`), registered in `sym-lib-table` / `fp-lib-table` via `${KIPRJMOD}` — carries parts that don't exist in stock KiCad:

- `das-boot.kicad_sym` — symbols: TPS56837RPAR, LM74800QDRRRQ1 (correct pin numbering and electrical types)
- `das-boot.pretty/` — footprints, built to the TI datasheet land patterns and render-verified:
  - `TPS5683x_VQFN-HR-10_3x3mm_RPA0010A` (SLVSGM3B; incl. VIN comb fingers, 89% paste on SW/PGND tabs)
  - `LM7480x_WSON-12_3x3mm_DRR0012E` (SNOSD95C; **pad 13 / RTN: solder but leave electrically floating — isolated island, no GND vias**)
- `das-boot.3dshapes/` — STEP + WRL (EasyEDA-sourced, cosmetic; check alignment in 3D viewer)

The EasyEDA/LCSC TPS56837 footprint was checked and rejected: pads shifted 0.05–0.06 mm, wrong SW pad length, MODE mis-numbered as pad 12. Stock KiCad's generic WSON-12 was rejected for the LM7480x: wrong EP size (1.5 mm vs 1.3 mm wide).

## Board-level concerns

- **Inline fuse per rail** from battery bus, sized just above expected continuous current
- **220 µF 16V low-ESR polymer at each servo header** (4× ≈ 880 µF total — inside the 6V soft-start inrush budget of ~1.2 mF; populate only fitted servos, DNP the rest)
- **SMBJ15A TVS + 100–220 µF electrolytic** on the common battery bus (see bus bulk capacitance)
- **TVS diode + fuse at the Pi 5V input** — GPIO has zero protection
- Reverse-polarity protection: **covered by the ideal-diode input stage** (P-FET no longer needed)
- **Per-port pack divider → STM32 ADC**; JST-XH for the primary pack
- **SWD header** (SWDIO, SWCLK, NRST, GND, 3.3V)
- **Layout**: minimal input ceramic loops at each buck; continuous ground pour; thermal vias under pads
- **Spares**: 3× of each IC — first hot-plate boards eat a chip occasionally

## Open questions

- Brushed vs brushless motor + ESC pick — deferred, off-board either way
- 3.3V LDO final pick (anything 500 mA+ from 5V works)
- LM7480x variant + dual-FET part selection for the battery ports
- Whether port 2 also gets a balance connector (per-cell sense on both packs vs pack-level only)
- MCU on this board (chip-down STM32F411) vs Black Pill carrier for v1 — drives the control connector pinout
