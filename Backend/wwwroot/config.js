// Adjust these values to match the separate camera-stream process.
// Keep the stream behind the same origin/reverse proxy when possible.
window.DAS_BOOT_CONFIG = Object.freeze({
  streamMode: "mjpeg", // "mjpeg" or "video"
  streamUrl: "",       // Example: "/camera/stream"
  apiBaseUrl: "",
  telemetryPollMilliseconds: 1000,
  statusPollMilliseconds: 2000,
  pilotStatusPollMilliseconds: 3000,
  servoSendIntervalMilliseconds: 90,
  servos: Object.freeze([
    { channel: 0, key: "rudder", label: "Roder", min: 500, center: 1500, max: 2500 },
    { channel: 1, key: "dive", label: "Dykroder", min: 500, center: 1500, max: 2500 },
    { channel: 2, key: "camera", label: "Kameratilt", min: 500, center: 1500, max: 2500 }
  ])
});
