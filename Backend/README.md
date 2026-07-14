# DasBoot.Api

A small ASP.NET Core Minimal API that owns a long-lived UART connection to the
STM32 and bridges browser requests to the binary protocol. The camera stream is
intentionally outside this process.

## Architecture

```text
Browser / frontend
       |
       | HTTP + JSON
       v
Minimal API endpoints
       |
       v
IStm32Link
       |
       +--> bounded Channel<OutboundCommand> --> one serial writer
       |
       +<-- one serial reader --> incremental CRC parser
                              --> ACK/NACK transaction matching
                              --> raw telemetry cache
       |
       v
configured serial device <--> STM32 USB CDC or USART1
```

Only `Stm32LinkService` touches `SerialPort`. HTTP request threads never read or
write UART directly. Multiple requests can enqueue commands concurrently, but a
single consumer writes complete frames in order.

## Why this is AOT-friendly

- `WebApplication.CreateSlimBuilder()`
- Minimal APIs; no MVC/controllers
- `System.Text.Json` source generation for every HTTP DTO
- No reflection-based configuration binding
- No dynamic assembly loading, runtime code generation, EF Core, Swagger, or
  third-party mediator/serializer packages
- A bounded command queue prevents unbounded memory growth
- Workstation/server GC is disabled in favor of lower idle memory

## Current protocol mapping

The frame layout is:

```text
[sync:u8][txn:u16 LE][func:u8][len:u8][payload][crc16:u16 LE]
```

CRC is CRC-16/MODBUS over every byte before the CRC, including the sync byte.
The maximum frame size is 262 bytes: 255 payload bytes + 7 bytes overhead.

Current firmware codes:

- `0x06`: ACK
- `0x10`: NACK
- `0x15`: PING
- `0x20`: SET_SERVO
- `0x21`: GET_SERVO
- `0x22`: GET_SERVO_ALL (firmware stub)

`PING` has an empty payload and returns an ACK with the same transaction id. The
API exposes it through `POST /api/stm32/ping`; no pilot lease is required.

### Servo payload

The firmware accepts:

```text
[servo_num:u8][target:u16 little-endian]
```

or the optional slew form:

```text
[servo_num:u8][target:u16 little-endian][step:u16 little-endian]
```

The dashboard currently sends the three-byte form. Firmware has four servo slots,
so valid channel ids are `0..3`. With the current 50 Hz, 20,000-unit PWM period,
targets such as `500..2500` correspond to normal servo pulse widths.

`SET_SERVO` first returns ACK/NACK and later sends a terminal `0x20` frame when
the movement finishes. The current API completes the HTTP request on the immediate
ACK. Terminal servo replies can be added to the transaction model later.

### Proposed telemetry convention

Telemetry function codes and payloads are not defined yet. This sample reserves
`txn = 0` for unsolicited device telemetry. Host-created commands use nonzero
transaction IDs. The latest such frame is exposed as raw bytes from:

```text
GET /api/telemetry/latest
```

Once telemetry packets are specified, decode each function code into typed
immutable snapshots inside `TelemetryStore` and expose typed endpoints. Do not
make endpoints read the serial port themselves.

## Endpoints

- `GET /health/live` — process is alive
- `GET /health/stm32` — UART port state; `responsive` after a valid frame was received recently
- `GET /api/status` — counters, timestamps, link state, last error
- `GET /api/telemetry/latest` — latest raw unsolicited telemetry frame, or 204
- `POST /api/stm32/ping` — sends PING over the configured serial device and measures ACK round-trip time
- `POST /api/control/servo` — sends one SET_SERVO command and waits for ACK/NACK

Example:

```bash
curl -X POST http://raspberrypi.local:5080/api/control/servo \
  -H 'Content-Type: application/json' \
  -d '{"channel":0,"pulseWidthMicroseconds":1500}'
```

## Raspberry Pi UART setup

Use the Pi's 3.3 V UART, cross TX/RX, and connect ground:

```text
Pi TX  -> STM32 RX (PA10)
Pi RX  <- STM32 TX (PA9)
Pi GND -- STM32 GND
```

Never feed 5 V logic into either side.

On Raspberry Pi OS, enable the serial hardware and disable the login shell on
that UART. `/dev/serial0` is preferred because it follows the active primary
UART. Ensure the service user can open it:

```bash
sudo usermod -aG dialout dasboot
```

Log out/reboot after changing group membership.

Verify that the firmware and API use the same baud rate. This project defaults
to 115200, 8 data bits, no parity, one stop bit, and no flow control.

## Fedora laptop USB demo

Build the STM32 firmware with its default `usb` transport and connect it to the
laptop. Confirm that Linux created a CDC ACM device:

```bash
ls -l /dev/ttyACM*
ls -l /dev/serial/by-id/
```

The helper script prefers a stable `/dev/serial/by-id/...` link that resolves to
a `ttyACM` device, then falls back to `/dev/ttyACM0`:

```bash
./run-fedora-demo.sh
```

A device can also be supplied explicitly:

```bash
./run-fedora-demo.sh /dev/ttyACM1
```

If access is denied, inspect the device group and add your user to that group,
then log out and back in. On many Fedora systems the group is `dialout`:

```bash
ls -l /dev/ttyACM0
sudo usermod -aG dialout "$USER"
```

Open `http://localhost:5080/` and press **Pinga STM32**. A successful response
proves the complete path: browser → HTTP API → USB CDC → STM32 → ACK → browser.

Manual startup is also supported:

```bash
DOTNET_ENVIRONMENT=Development \
Stm32__PortName=/dev/ttyACM0 \
dotnet run
```

Use `DasBoot.Api.http` or curl to exercise the endpoints:

```bash
curl -X POST http://localhost:5080/api/stm32/ping
```

## Publish Native AOT for a 64-bit Raspberry Pi OS

Install the native toolchain on Debian/Raspberry Pi OS:

```bash
sudo apt-get update
sudo apt-get install -y clang zlib1g-dev
```

Publish:

```bash
dotnet publish -c Release -r linux-arm64 --self-contained true
```

Output:

```text
bin/Release/net10.0/linux-arm64/publish/
```

Treat every trim/AOT warning as a release blocker. The project promotes the two
most common dynamic-code warnings (`IL2026`, `IL3050`) to errors, but the publish
output must still be reviewed for warnings from all dependencies.

## Install as a service

Create the service user and install the binary:

```bash
sudo useradd --system --home /nonexistent --shell /usr/sbin/nologin dasboot
sudo usermod -aG dialout dasboot
sudo mkdir -p /opt/das-boot-api
sudo cp -a bin/Release/net10.0/linux-arm64/publish/. /opt/das-boot-api/
sudo cp systemd/das-boot-api.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now das-boot-api
```

Inspect logs:

```bash
journalctl -u das-boot-api -f
```

## Production notes

- Keep the frontend and API on the same origin through a small reverse proxy if
  possible. That avoids adding permissive CORS rules.
- HTTP is intentional on the local Pi process. Terminate TLS elsewhere if the
  control interface is exposed beyond the trusted local network.
- The pilot lease arbitrates control but is not user authentication. Add authentication
  and TLS before using an untrusted network.
- Do not retry servo commands blindly after a timeout unless the command is
  explicitly designed to be idempotent.
- The UART device can remain open even when the STM32 is powered off. Therefore
  `PortOpen` and `DeviceResponsive` are separate status fields.

## Dashboard och pilotlås

Projektet serverar nu en vanilla frontend direkt från `wwwroot`:

- mobil-först layout där videon prioriteras,
- touchvänliga servoreglage,
- rå telemetri, seriell länkstatus och ett USB/UART-pingtest,
- konfigurerbar separat videoström,
- server-side pilot lease så endast en klient får styra åt gången.

Öppna `http://<raspberry-pi>:5080/` efter att tjänsten startats. Kameraadressen och
servokanalerna ställs in i `wwwroot/config.js`.

Pilotlåset använder följande endpoints:

```text
GET  /api/pilot/status
POST /api/pilot/claim
POST /api/pilot/heartbeat
POST /api/pilot/release
```

En giltig token måste skickas i `X-Pilot-Token` till `POST /api/control/servo`.
Standardleasen är 15 sekunder med heartbeat var femte sekund och konfigureras under
`Pilot` i `appsettings.json`.

Pilotlåset är kontrollarbitrering, inte ett komplett säkerhetssystem. STM32-firmware
bör fortfarande ha ett eget timeout/watchdog som neutraliserar motorer och servon om
kontrollkommandon uteblir.
