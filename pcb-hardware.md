# PCB Hardware

Locked part selection for the power-delivery / STM32 carrier PCB. See [`hardware-plans.md`](./hardware-plans.md) for the broader architecture context.

## Locked targets (2026-08-28)

| Rail | Spec | Load | Regulator |
|---|---|---|---|
| 6V servo | 6.0V @ 5A | 4× MG90S | **TI TPS56837** (LCSC C22428366) |
| 5V logic | 5.0V @ 3A | Rpi + STM32 + headroom | **TI LMR33630** |
| 3.3V | 3.3V @ ≤1A | STM32, from 5V rail | LDO (MCP1826S-3302 or similar) |

Both rails are generated in parallel directly from the battery input — never in series. Two *different* buck ICs on purpose: the v1 board doubles as an eval of both parts, which matters more than a shared-BOM win.

The ESC/motor decision is deferred. The ESC taps the battery via a Y-split in the pack lead, **before** the PCB — motor current (up to 30A) never touches the board, so copper, fuse, and connector sizing stays in the ~6–8A range.

## Design constraints

- **Vin design window: 8.0–13.0V.** Covers 3S LiPo (12.6V charged → ~9V loaded at 3.3V/cell cutoff) *and* a 3S1P Li-ion swap, which is happy down to ~2.75V/cell (~8.25V). Both bucks are 36V-input parts, so the window costs nothing.
- **Input transient clamp: SMBJ15A** TVS at the battery input. Standoff 15V (clear of 12.6V charged), clamp ~24V — comfortably inside the 36V rating of both bucks. This is the reason the 17V-class bucks were dropped: nothing clamps between 12.6V and 17V with real margin on both sides.
- **Hot-plate assembly.** Both bucks are thermal-pad packages (QFN / PowerPAD HSOIC) — exactly what the hot plate handles well and hand-ironing doesn't.

## Battery

**Primary: 3S LiPo, 2200 mAh, ≥20C, XT60 + JST-XH balance lead** (~$20).

- Power budget ~30W average (20–50W motor + ~2W Pi Zero 2 W + servo bursts) → ~2.7A draw → roughly 40–60 min mixed running.
- ~105×34×24 mm, ~180 g — fits an 8 cm hull mounted low for self-righting.
- 2200 mAh × 20C = 44A available; we draw <10% of that. Exactly the headroom the sealed-hull LiPo safety plan wants.
- 3000 mAh if the trim budget allows; don't chase runtime beyond that, ballast and trim pay for it.

### 3S1P Li-ion swap (test phase)

Three Li-ion 18650s in series is still "3S": 12.6V charged, ~10.8V nominal. With matching **XT60 + JST-XH** termination it's a plug-in swap. Caveats:

- **Discharge floor** is lower (~2.75–3.0V/cell) — that's why the Vin window is designed to 8V, so the extra capacity is actually usable.
- **BMS packs usually have no balance connector.** Off-the-shelf protected 18650 packs often expose only two wires, which breaks per-cell ADC monitoring. Either build/buy an unprotected pack with a JST-XH pigtail (charge like a LiPo on the same balance charger), or accept pack-level-only monitoring on those packs.

Good cell if building: Molicel P28A (2.8 Ah, 30A+) — one cell covers the entire load.

## Servos — 4× MG90S @ 6V

Standard-voltage servos, no HV. The 7.4V HV rail idea from earlier planning is dead: HV digitals cost 5–10× more and buy nothing at this torque.

- Torque check: a few-cm² fin at 1 m/s sees ~1N; on a ~2 cm horn that's ~0.2 kg·cm. MG90S is 2.2 kg·cm — 10× margin.
- Current: ~0.7–1A per servo at stall → 4× simultaneous stall ≈ 3–4A worst case → **5A rail spec** with margin.
- MG996R-class was considered and rejected: 4× stall ≈ 10A would force a different regulator (board respin). If full-size servos ever appear, that's a v2.
- Ballast piston is a geared DC motor, not on this rail.

## 6V buck — TI TPS56837

8A, 4.5–28V synchronous buck, D-CAP3, VQFN-HR-10 (HotRod) 3×3 mm (~$1.30/1ku, LCSC C22428366). Datasheet: SLVSGM3B.

- **32V abs max / 28V recommended input** — the SMBJ15A input TVS (clamp ~24V worst case) fits below even the recommended max. This margin is why the WEBENCH-suggested 16V-class parts (TPS565242/47) were passed over.
- 8A covers the 4× MG90S stall case with 2× margin; a buck loafing at 40–60% of rating runs in its peak-efficiency region. Current limit is resistor-selectable via the MODE pin.
- Excellent thermals: HotRod flip-chip (no bond wires), 20.4/9.5 mΩ FETs, effective RθJA 30°C/W on a 4-layer board.
- 500/800/1200 kHz selectable via MODE resistor — use 500 kHz at 12V→6V (WEBENCH design: 95% efficiency, 12-part BOM).
- Supports 98% duty — 6V out from an 8V sagged input is fine.
- **Variants, pin-to-pin in the same footprint:** TPS56837 = Eco-mode (pulse-skipping, 45 µA Iq — best battery runtime; servos don't care about light-load ripple). **TPS56838 = FCCM** drop-in if light-load ripple ever becomes a problem. TPS56836 = Out-of-Audio.
- **Inspection caveat:** HotRod terminals are bottom-only — no side fillet to inspect after reflow. Plan on a stencil (TI's example: 0.1 mm, 89% paste coverage on the SW/PGND tabs), and route PG (power-good) to an LED or test point for electrical verification.
- Layout: AGND and PGND tie at a single point; input ceramics tight between VIN and PGND pins; thermal vias in the PGND land.

## 5V buck — TI LMR33630

3A, 3.8–36V synchronous buck, HSOIC-8 PowerPAD (~$2.50).

- 3A covers even a Pi 4B; Pi Zero 2 W needs ~1.5A, so big margin at the expected load.
- Dead-simple BOM, huge number of reference designs, WEBENCH-supported.
- Available in 400 kHz / 1.4 MHz / 2.1 MHz variants — let WEBENCH pick the frequency/inductor pairing.

## Rejected candidates (and why)

| Part | Verdict |
|---|---|
| LM61460 | Good part (6A, 36V, wettable flanks), but TPS56837 beats it on price, current headroom, and thermals |
| TPS565242 / TPS565247 | WEBENCH's cost-ranked suggestions at Vmax=14V. 16V rec / 18V abs max input: no TVS fits between 12.6V and abs max; SOT-563 has no thermal pad (eff. RθJA 58°C/W) |
| TPS565208 | Same 17V-class headroom problem; 5A in plain SOT-23-6 has no thermal path |
| LM5148 | Controller + external FETs, 19-part BOM — gate-drive layout subproject for no benefit at 5A |
| TPS54824 | Same 17V headroom problem; 8A overkill at MG90S stall currents |
| TPS54561 / TPS54360 | Workable fallbacks, but non-synchronous (external catch diode, ~5% worse efficiency, more board heat) |
| LM5145 / LM5146 + FETs | Controller + external FETs is a subproject; unnecessary at 5A |
| MP2315 | Fine cheap alternate for the 5V rail if doing JLCPCB assembly; TI docs are better for a first layout |
| Pololu modules (D36V50F5, D24V90F6) | Superseded — hot-plate capability makes chip-down viable for v1 |

## Custom KiCad library

Project-local library at the schematic root (`PCB/`), registered in `sym-lib-table` / `fp-lib-table` via `${KIPRJMOD}` — carries parts that don't exist in stock KiCad:

- `das-boot.kicad_sym` — symbols (TPS56837RPAR, with correct pin numbering and electrical types)
- `das-boot.pretty/` — footprints. `TPS5683x_VQFN-HR-10_3x3mm_RPA0010A` is built to the TI datasheet land pattern (pad positions verified against the SLVSGM3B land pattern example, incl. the VIN comb fingers), NSMD with 0.07 mm mask margin and 89% paste on the SW/PGND tabs per TI's stencil example. Shared by TPS56837/38/36.
- `das-boot.3dshapes/` — STEP + WRL models (sourced from the EasyEDA library; cosmetic only — verify alignment in the 3D viewer)

The EasyEDA/LCSC footprint for this part was checked and rejected: pads shifted 0.05–0.06 mm vs the TI land pattern, wrong SW pad length, and MODE mis-numbered as pad 12.

## Board-level concerns

- **Inline fuse per rail** from battery, sized just above expected continuous current
- **Bulk capacitor (470–1000 µF)** on the servo rail output to absorb simultaneous-start / stall transients
- **SMBJ15A TVS** at the battery input (see design constraints)
- **TVS diode + fuse at the Pi 5V input** — GPIO has zero overcurrent/overvoltage protection
- **Reverse-polarity protection**: P-channel MOSFET in series on Vbatt (check Vgs rating against 13V, clamp gate if needed)
- **JST-XH balance connector footprint** + divider network to STM32 ADC for per-cell monitoring
- **SWD header** (5 pins: SWDIO, SWCLK, NRST, GND, 3.3V)
- **Layout**: input ceramic loop (Vin cap → IC → GND) as small as physically possible for each buck; continuous ground pour under both; thermal vias under the exposed pads
- **Spares**: order 3× of each buck IC — first hot-plate boards eat a chip occasionally

## Open questions

- Brushed vs brushless motor + ESC pick — deferred, off-board either way
- 3.3V LDO final pick (MCP1826S-3302 carried over from the old schematic; anything 500 mA+ from 5V works)
- Whether the test-phase 3S1P pack keeps a balance lead (drives whether per-cell monitoring survives the swap)
- Servo/ESC connector style on the board (standard 0.1" 3-pin headers?)
