# Frontend + Backend

Rough notes on the surface-side stack. Covers the backend server running on the RPi and the browser-based dashboard. Subject to change as we build.

## Scope

- Backend server: WebSocket handling, pilot token logic, serial link to STM32
- Dashboard: telemetry display, control input, video view, pilot status

## Overall architecture

```
RPi (inside hull)
  ├── Processes video → streams (camera person's concern)
  ├── Talks to STM32 via UART(?)
  └── Backend server
        ├── WebSocket → telemetry out, commands in
        └── Static file serving → dashboard HTML/CSS/JS
        |
        | Ethernet tether
        |
Buoy (WiFi router)
  └── WiFi → Laptop / Phone / Tablet (multiple viewers simultaneously)
```

No cloud, no authentication, no internet. Entirely local network.

## Backend — server on RPi

Language not yet decided. Two candidates:

**C# with ASP.NET Core (Minimal API) — under consideration**

AoT compilation (.NET 8+) produces a native binary with no runtime dependencies, making it viable even on a Pi Zero 2 W. Lower memory usage and faster startup than JIT. Limitations: SignalR does not fully support AoT — vanilla WebSockets used instead. Reflection avoided in favour of source generators with `System.Text.Json`. Cross-compilation to ARM64 via `dotnet publish -r linux-arm64` works out of the box.

**C++ — alternative**

Native from the start, minimal memory footprint, no runtime dependencies. More control but more to build by hand (HTTP server, WebSocket handling). Libraries like `Boost.Beast` or `libwebsockets` cover what's needed.

Regardless of language, the requirements are the same:

| Component | Responsibility |
|---|---|
| WebSocket server | Manages connections, broadcasts telemetry, receives input |
| Static file serving | Serves dashboard files |
| Serial link | UART → STM32, forwards commands |
| Pilot token | Determines who is allowed to send commands |

The video stream is served by the camera person as a separate HTTP endpoint. The backend server does not need to know about it.

### Pilot token logic

Multiple clients can watch simultaneously, but only one can control at a time. The server holds a token and rejects commands from anyone without it.

```
[Viewer A] ──watch──┐
[Viewer B] ──watch──┤──→ Backend server ──→ STM32
[Pilot]  ───control──┘   (token verified per command)
```

- Whoever requests control receives the token — the previous pilot loses it immediately
- If the pilot disconnects, the token is released after X seconds (timeout TBD)
- Server broadcasts pilot status to all connected clients via WebSocket

### WebSocket protocol (draft)

Frontend sends raw input — the server decides what to do with it.

```json
// Client → server: control input
{ "type": "input", "throttle": 0.7, "yaw": -0.3, "pitch": 0.1 }

// Client → server: request control
{ "type": "request_control" }

// Server → client: telemetry
{ "type": "telemetry", "depth": 2.1, "pitch": -3.2, "roll": 0.8, "battery_v": 11.4, "piston_pct": 50, "temp_c": 8.3 }

// Server → all clients: pilot status
{ "type": "pilot_status", "pilot": "192.168.1.5", "has_control": true }
```

## Frontend — dashboard

Vanilla HTML, CSS, and JS. No frameworks, no build step, no dependencies. A single HTML file served statically by the backend. Open a URL, it works.

Frontend only displays and sends raw input — no logic, no state beyond rendering. The server is authoritative.

| Component | Choice | Reason |
|---|---|---|
| Language | Vanilla JS | No build step, runs anywhere |
| Framework | None | Overkill for a dashboard |
| Communication | `WebSocket` (built-in) | — |
| Video | `<img src="…/stream.mjpg">` | One tag, no libraries needed |
| Control input | Touch / Gyro / Gamepad | User picks in UI, switchable at runtime |

### Frontend responsibilities

- Display the video stream (`<img>` tag pointing at the camera endpoint)
- Render incoming telemetry from WebSocket
- Send raw control input to the server
- Show pilot status and a "Take control" button

The frontend never knows whether it is actually piloting or just watching — that is the server's concern.

### Control modes

Selected in the UI, switchable at runtime. Gyro resets its reference on activation.

**Touch joystick** — two virtual sticks on screen. Precise, works on any device. Fingers cover part of the screen.

**Gyroscope** — `DeviceOrientation` API, built into mobile browsers. Tilt the phone to steer. Hands don't obscure the screen but can be tiring over longer sessions.

**Gamepad** — Xbox/PS controller via USB or Bluetooth. Gamepad API is built into all modern browsers, no drivers needed. Most precise for longer sessions.

```js
function loop() {
  const gp = navigator.getGamepads()[0];
  if (gp) {
    sendInput({
      throttle: gp.axes[1],  // left stick Y
      yaw:      gp.axes[0],  // left stick X
      pitch:    gp.axes[3],  // right stick Y
    });
  }
  requestAnimationFrame(loop);
}
```

Keyboard as an additional fallback.

## File structure (proposed)

```
surface/
├── index.html        # Dashboard — single file, no build step
├── style.css
├── main.js           # WebSocket client, control modes, rendering
└── server/
    ├── src/
    │   ├── main.*    # Server, routes, WebSocket handling
    │   ├── pilot.*   # Pilot token logic
    │   └── serial.*  # UART ↔ STM32
    └── (.csproj / CMakeLists.txt depending on language choice)
```

## Open questions / TODO

- **Backend language — C# AoT or C++?**
- Pilot token timeout on disconnect — how many seconds?
- Gamepad mapping — which controller? (Xbox / PS / generic)
- Keyboard fallback — which keys?
- Frontend behaviour on lost WebSocket connection — what does the user see?
- Telemetry logging to file on RPi for post-dive analysis
- Video overlay in frontend: depth, battery, timestamp?
- Gyro calibration — how to reset reference smoothly in the UI?
- Finalise WebSocket protocol with firmware person (what does STM32 send up?)
