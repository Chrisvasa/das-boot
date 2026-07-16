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
    { channel: 0, key: "servo-0", label: "Servo 0", min: 0, center: 9500, max: 19000, step: 100 },
    { channel: 1, key: "servo-1", label: "Servo 1", min: 0, center: 9500, max: 19000, step: 100 },
    { channel: 2, key: "servo-2", label: "Servo 2", min: 0, center: 9500, max: 19000, step: 100 },
    { channel: 3, key: "servo-3", label: "Servo 3", min: 0, center: 9500, max: 19000, step: 100 }
  ])
});
