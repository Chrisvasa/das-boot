# Frontend + Backend

Rough notes on the surface-side stack. Covers the backend server running on the Raspberry Pi and the browser-based dashboard used by pilots/viewers.

This README is only about the frontend/backend part of the project. STM32 firmware, low-level motor/servo control, and the camera pipeline are handled separately.

Subject to change as we build.

## Scope

This part of the project covers:

* Backend server running on the Raspberry Pi
* Static file serving for the dashboard
* WebSocket handling between browser clients and backend
* Pilot token logic: only one active pilot at a time
* Receiving control input from the dashboard
* Forwarding high-level control input to the STM32
* Receiving telemetry/status from the STM32
* Broadcasting telemetry/status to all connected clients
* Browser dashboard for video, telemetry, controls, and pilot status

Out of scope for this README:

* STM32 firmware internals
* Final STM32 USB/serial protocol details
* PWM generation for ESC/servos
* Ballast control implementation
* Sensor sampling/filtering
* Camera/video pipeline internals

The frontend/backend layer acts as the bridge between users on the surface network and the STM32 inside the submarine.

## Overall architecture

```text
RPi (inside hull)
  ├── Camera/video process → stream endpoint
  │
  ├── Backend server
  │     ├── Static file serving → dashboard HTML/CSS/JS
  │     ├── WebSocket → telemetry/status out, input in
  │     ├── Pilot token logic
  │     ├── Control input handling
  │     └── STM32 link wrapper
  │
  └── USB CDC / serial link → STM32

        |
        | Ethernet tether
        |

Buoy (WiFi router)
  └── WiFi → Laptop / Phone / Tablet
              ├── Pilot
              └── Viewers
```

No cloud, no authentication, no internet dependency. Entirely local network.

Multiple clients can connect at the same time. Everyone can watch telemetry/video, but only one client may control the submarine at a time.

## Backend — server on RPi

### C# with ASP.NET Core Minimal API — preferred for v1

Preferred for v1 as long as the backend stays lightweight:

- Static file serving
- Raw WebSockets
- Pilot token state
- STM32 link wrapper
- Telemetry broadcasting
- Optional simple file logging

The backend must not handle video encoding, transcoding, or proxying. The camera/video stream is served by a separate process and displayed directly by the frontend.

Use:

- Minimal API
- Raw WebSockets
- BackgroundService for STM32 link/control loop
- System.Text.Json
- No SignalR
- No MVC
- No database
- No video processing in backend

Native AOT can be tested later if memory usage or startup time becomes a problem.

### Rust — fallback if resource budget becomes tight

Rust is a good alternative if the Pi Zero 2 W memory/CPU budget becomes too tight, or if sharing protocol definitions with the STM32 side becomes useful.

Expected advantages:

- Lower runtime overhead
- Small native binary
- Good fit for async WebSocket + serial handling
- Easier to keep resource usage predictable

Downside:

- More implementation work if the frontend/backend developer is faster in C#

## Backend components

Regardless of language, the backend should be split into clear responsibilities.

| Component             | Responsibility                                                         |
| --------------------- | ---------------------------------------------------------------------- |
| WebSocket server      | Manages browser connections, receives messages, sends telemetry/status |
| Static file server    | Serves dashboard files                                                 |
| Pilot manager         | Tracks who currently has control                                       |
| Control state         | Stores the latest valid input from the current pilot                   |
| STM32 link            | Wraps the USB/serial link to the STM32                                 |
| Telemetry broadcaster | Sends telemetry/status to all connected clients                        |
| Logger                | Optional telemetry/input logging for post-test analysis                |

The WebSocket layer should not directly write raw frames to the STM32. STM32 communication should go through one backend component, for example `Stm32Link`.

## STM32 link from backend perspective

The STM32 protocol is handled separately by the firmware side.

From the backend perspective, the STM32 link should expose high-level methods/events such as:

```text
SendControl(input)
Arm()
Disarm()
GetStatus()
OnTelemetryReceived(...)
OnFaultReceived(...)
```

The rest of the backend should not care about the binary protocol details, such as:

* Sync bytes
* Transaction IDs
* ACK/NACK frames
* Payload layout
* Function codes
* Parser state

Those details belong inside the STM32 link wrapper.

## Control input handling

Frontend sends high-level control input to the backend.

Example:

```json
{
  "type": "input",
  "seq": 1842,
  "throttle": 0.7,
  "yaw": -0.3,
  "pitch": 0.1,
  "ballast": 0.0
}
```

Values are normalized:

```text
-1.0 = full negative
 0.0 = neutral
+1.0 = full positive
```

The backend should:

* Check that the sender is the current pilot
* Clamp values to valid ranges
* Optionally apply deadzones/smoothing
* Store the latest valid input
* Forward input to the STM32 through `Stm32Link`

Live control input should not build up as a queue. The backend should care about the latest input, not old input.

Preferred model:

```text
Browser sends input regularly
Backend stores latest valid input
Backend forwards latest input to STM32
Old input is overwritten
```

This avoids the submarine reacting to stale pilot input.

## Pilot token logic

Multiple clients can watch simultaneously, but only one can control at a time.

The server holds the pilot token and rejects control input from anyone who does not currently own it.

```text
[Viewer A] ──watch──────┐
[Viewer B] ──watch──────┤
[Pilot]  ───control─────┘
                         ↓
                   Backend server
                         ↓
                       STM32
```

Rules:

* Any client may request control.
* Whoever requests control receives the token.
* The previous pilot loses control immediately.
* Only the current pilot's input is accepted.
* If the pilot disconnects, control is released after a short timeout.
* Server broadcasts pilot status to all connected clients.
* The frontend should not assume it has control until the server confirms it.

Suggested timeout:

```text
Pilot disconnect timeout: 1–3 seconds
```

## WebSocket protocol draft

Frontend sends JSON messages to the backend. Backend sends JSON messages to clients.

The browser never talks directly to the STM32.

### Client → server: request control

```json
{
  "type": "request_control"
}
```

### Client → server: release control

```json
{
  "type": "release_control"
}
```

### Client → server: control input

Only accepted from the current pilot.

```json
{
  "type": "input",
  "seq": 1842,
  "throttle": 0.7,
  "yaw": -0.3,
  "pitch": 0.1,
  "ballast": 0.0
}
```

### Server → client: telemetry

Broadcast to all connected clients.

```json
{
  "type": "telemetry",
  "depth": 2.1,
  "pitch": -3.2,
  "roll": 0.8,
  "battery_v": 11.4,
  "piston_pct": 50,
  "temp_c": 8.3
}
```

### Server → all clients: pilot status

```json
{
  "type": "pilot_status",
  "pilot_id": "client-7",
  "pilot_label": "192.168.1.5",
  "you_have_control": true
}
```

`you_have_control` is useful because every connected browser needs to know whether *that specific client* currently owns the token.

### Server → all clients: link status

```json
{
  "type": "link_status",
  "stm32": "connected",
  "armed": false
}
```

### Server → all clients: fault/status message

```json
{
  "type": "fault",
  "source": "stm32",
  "code": "LOW_BATTERY",
  "message": "Battery below warning threshold"
}
```

## Frontend — dashboard

Vanilla HTML, CSS, and JS. No frameworks, no build step, no dependencies.

A single dashboard served statically by the backend.

```text
Open URL → dashboard works
```

Frontend only displays data and sends input. The backend decides whether that input is accepted.

| Component     | Choice                                 | Reason                           |
| ------------- | -------------------------------------- | -------------------------------- |
| Language      | Vanilla JS                             | No build step, runs anywhere     |
| Framework     | None                                   | Overkill for this dashboard      |
| Communication | `WebSocket`                            | Built into browsers              |
| Video         | `<img src="…/stream.mjpg">` or similar | Simple, no client library needed |
| Control input | Gamepad / keyboard / touch / gyro      | User-selectable                  |
| Build step    | None                                   | Copy files and run               |

## Frontend responsibilities

* Display the video stream
* Render incoming telemetry
* Render current pilot/control status
* Show backend/STM32 connection status
* Send input messages to the backend
* Provide a "Take control" button
* Allow switching control mode at runtime
* Show clear warning/error states

The frontend should clearly show whether the user is:

```text
Watching only
Requesting control
Currently pilot
Disconnected
Control lost
```

## Frontend UI states

Suggested visible states:

```text
DISCONNECTED
CONNECTED / WATCHING
REQUESTING CONTROL
PILOT ACTIVE
CONTROL LOST
STM32 LINK LOST
LOW BATTERY
FAULT ACTIVE
```

Important indicators:

* WebSocket connected/disconnected
* STM32 connected/disconnected
* Current pilot
* Whether this browser has control
* Armed/disarmed state
* Battery voltage
* Depth
* Pitch/roll
* Ballast/piston position
* Fault/warning messages

The pilot should always know whether input is actually being accepted.

## Control modes

Selected in the UI and switchable at runtime.

Recommended priority:

```text
1. Gamepad
2. Keyboard
3. Touch joystick
4. Gyro
```

### Gamepad

Best primary control method for laptop/tablet use.

Xbox/PS controller via USB or Bluetooth. Most precise for longer sessions.

```js
function loop() {
  const gp = navigator.getGamepads()[0];

  if (gp) {
    sendInput({
      throttle: -gp.axes[1], // left stick Y, inverted if needed
      yaw:       gp.axes[0], // left stick X
      pitch:    -gp.axes[3], // right stick Y, inverted if needed
      ballast:   0
    });
  }

  requestAnimationFrame(loop);
}
```

Need to test final mapping with the actual controller.

Open questions:

* Xbox controller mapping
* PS controller mapping
* Generic controller mapping
* Trigger/button use for ballast
* Deadzone amount
* Axis inversion

### Keyboard

Useful fallback on laptop.

Possible mapping:

| Key           | Action                |
| ------------- | --------------------- |
| W/S           | Throttle forward/back |
| A/D           | Yaw left/right        |
| Arrow up/down | Pitch                 |
| Q/E           | Ballast up/down       |
| Space         | Neutral / stop        |
| R             | Request control       |
| Esc           | Release control       |

Keyboard should be treated as fallback, not the main long-term control method.

### Touch joystick

Works on phones/tablets without extra hardware.

Pros:

* Works on almost any device
* No controller needed

Cons:

* Fingers cover the screen
* Less precise than gamepad
* Harder during longer sessions

Suggested layout:

```text
Left virtual stick:
  throttle + yaw

Right virtual stick:
  pitch + ballast
```

### Gyroscope

Experimental/mobile mode.

Uses browser device orientation where available. Tilt phone to steer.

Pros:

* Hands do not cover virtual sticks
* Fun to test

Cons:

* Can be tiring
* Browser permissions may be annoying
* Calibration/drift needs UI handling
* Not ideal as primary control

Gyro should reset its reference orientation when activated.

## Video

The video stream is handled separately by the camera/video part of the project.

Frontend only needs a stream URL.

Example:

```html
<img id="video" src="http://sub.local:8080/stream.mjpg" alt="Submarine video stream">
```

The backend does not need to parse or understand the video stream.

Possible future frontend overlay:

* Depth
* Battery
* Timestamp
* Armed/disarmed status
* Recording/logging indicator
* Low battery warning

## Logging

Telemetry logging on the RPi is optional but useful for post-test debugging.

Simple formats:

```text
JSONL: one JSON object per line
CSV: easier to inspect in spreadsheet tools
```

Suggested log fields:

```text
timestamp
pilot_id
input.throttle
input.yaw
input.pitch
input.ballast
battery_v
depth
pitch
roll
piston_pct
temp_c
armed
stm32_link_status
fault_code
```

Example JSONL row:

```json
{"t":123.456,"pilot":"client-7","throttle":0.2,"yaw":-0.1,"pitch":0.0,"battery_v":11.4,"depth":2.1,"piston_pct":50,"fault":null}
```

## Runtime flow

Typical control flow:

```text
1. Browser opens dashboard URL
2. Browser loads HTML/CSS/JS
3. Browser opens WebSocket to backend
4. Backend sends current status
5. User clicks "Take control"
6. Backend assigns pilot token
7. Browser starts sending input
8. Backend stores latest valid input
9. Backend forwards control input to STM32 through Stm32Link
10. STM32 handles low-level control
11. STM32 sends telemetry/status back
12. Backend broadcasts telemetry/status to all clients
13. Frontend updates UI
```

## MVP build order

1. Static dashboard loads from RPi
2. WebSocket connects and shows connection status
3. Fake telemetry from backend to frontend
4. Pilot token: take/release control
5. Gamepad/keyboard input visible in backend logs
6. Latest-input handling instead of queued input
7. STM32 link wrapper added
8. STM32 dummy telemetry appears in dashboard
9. Backend forwards normalized control input to STM32
10. Basic warning/status UI
11. Optional telemetry logging
12. Add video stream element
13. Add overlay polish

## Open questions / TODO

* Final RPi model: Zero 2 W or 4B?
* STM32 link path: stable `/dev/ttyACMx` or udev alias?
* Pilot token disconnect timeout duration
* Gamepad mapping: Xbox, PS, or generic first?
* Keyboard fallback keys
* Touch joystick layout
* Gyro calibration/reset behavior
* What should frontend show on lost WebSocket connection?
* What should frontend show on lost STM32 connection?
* Should backend require explicit arm/disarm before forwarding throttle?
* Telemetry logging format: JSONL or CSV?
* Video stream URL/path
* Video overlay: depth, battery, timestamp, armed state?
* Final WebSocket protocol shape
* Final backend-facing interface to the STM32 link
