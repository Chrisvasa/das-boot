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
/dev/serial0 <--> STM32 USART1
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

Implemented firmware function codes currently known to this API:

- `0x06`: ACK
- `0x15`: NACK
- `0x20`: SET_SERVO

### Important: provisional servo payload

The firmware README names `SET_SERVO`, but does not yet define its payload. This
sample proposes:

```text
[channel:u8][pulse_width_us:u16 little-endian]
```

Change `Stm32/ServoPayload.cs` when the Rust command dispatcher gets its final
payload definition. The current Rust parser only recognizes the function code
and sends ACK/NACK; it does not yet actuate a servo or validate this payload.

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

## Develop

```bash
dotnet restore
dotnet run
```

Use `DasBoot.Api.http` or curl to exercise the endpoints.

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
- Add authentication and a single-pilot/lease mechanism before allowing control
  from an untrusted or multi-user network.
- Do not retry servo commands blindly after a timeout unless the command is
  explicitly designed to be idempotent.
- The UART device can remain open even when the STM32 is powered off. Therefore
  `PortOpen` and `DeviceResponsive` are separate status fields.

## Dashboard och pilotlås

Projektet serverar nu en vanilla frontend direkt från `wwwroot`:

- mobil-först layout där videon prioriteras,
- touchvänliga servoreglage,
- rå telemetri och UART/STM32-status,
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
