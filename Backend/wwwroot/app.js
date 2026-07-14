"use strict";

const config = window.DAS_BOOT_CONFIG ?? {};
const apiBaseUrl = String(config.apiBaseUrl ?? "").replace(/\/$/, "");

const state = {
  link: {
    portOpen: false,
    deviceResponsive: false,
    lastReceiveUtc: null,
    crcErrors: 0,
    portName: ""
  },
  pilot: {
    token: sessionStorage.getItem("dasBoot.pilotToken"),
    displayName: localStorage.getItem("dasBoot.pilotName") || "",
    heartbeatIntervalMilliseconds: 5000,
    heartbeatTimer: 0,
    occupiedBy: null,
    expiresAtUtc: null
  },
  clientId: getOrCreateClientId(),
  controls: new Map()
};

const elements = {
  connectionPill: document.querySelector("#connectionPill"),
  connectionLabel: document.querySelector("#connectionLabel"),
  pilotButton: document.querySelector("#pilotButton"),
  pilotButtonLabel: document.querySelector("#pilotButtonLabel"),
  pilotBanner: document.querySelector("#pilotBanner"),
  pilotBannerText: document.querySelector("#pilotBannerText"),
  controlLock: document.querySelector("#controlLock"),
  servoControls: document.querySelector("#servoControls"),
  servoTemplate: document.querySelector("#servoTemplate"),
  streamStage: document.querySelector("#streamStage"),
  streamPlaceholder: document.querySelector("#streamPlaceholder"),
  mjpegStream: document.querySelector("#mjpegStream"),
  videoStream: document.querySelector("#videoStream"),
  liveBadge: document.querySelector("#liveBadge"),
  liveBadgeText: document.querySelector("#liveBadgeText"),
  fullscreenButton: document.querySelector("#fullscreenButton"),
  pilotDialog: document.querySelector("#pilotDialog"),
  pilotForm: document.querySelector("#pilotForm"),
  pilotName: document.querySelector("#pilotName"),
  pilotDialogError: document.querySelector("#pilotDialogError"),
  claimPilotButton: document.querySelector("#claimPilotButton"),
  cancelPilotButton: document.querySelector("#cancelPilotButton"),
  refreshButton: document.querySelector("#refreshButton"),
  uartStatus: document.querySelector("#uartStatus"),
  stm32Status: document.querySelector("#stm32Status"),
  serialPortName: document.querySelector("#serialPortName"),
  pingPanel: document.querySelector("#pingPanel"),
  pingButton: document.querySelector("#pingButton"),
  pingResult: document.querySelector("#pingResult"),
  lastReceive: document.querySelector("#lastReceive"),
  crcErrors: document.querySelector("#crcErrors"),
  rawFunction: document.querySelector("#rawFunction"),
  rawPayload: document.querySelector("#rawPayload"),
  rawReceived: document.querySelector("#rawReceived"),
  batteryValue: document.querySelector("#batteryValue"),
  depthValue: document.querySelector("#depthValue"),
  temperatureValue: document.querySelector("#temperatureValue"),
  toastRegion: document.querySelector("#toastRegion")
};

class ApiError extends Error {
  constructor(message, status, code = null, body = null) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.body = body;
  }
}

async function apiRequest(path, options = {}) {
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), options.timeout ?? 3500);
  const headers = new Headers(options.headers ?? {});

  if (options.body !== undefined && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }

  try {
    const response = await fetch(`${apiBaseUrl}${path}`, {
      method: options.method ?? "GET",
      headers,
      body: options.body === undefined ? undefined : JSON.stringify(options.body),
      cache: "no-store",
      credentials: "same-origin",
      keepalive: options.keepalive ?? false,
      signal: controller.signal
    });

    if (response.status === 204) {
      return null;
    }

    const contentType = response.headers.get("content-type") ?? "";
    const body = contentType.includes("application/json")
      ? await response.json()
      : await response.text();

    if (!response.ok) {
      const message = typeof body === "object" && body?.message
        ? body.message
        : `API-anropet misslyckades (${response.status}).`;
      throw new ApiError(message, response.status, body?.code ?? null, body);
    }

    return body;
  } catch (error) {
    if (error?.name === "AbortError") {
      throw new ApiError("API-anropet tog för lång tid.", 0, "request_timeout");
    }
    throw error;
  } finally {
    window.clearTimeout(timeout);
  }
}

function getOrCreateClientId() {
  const key = "dasBoot.clientId";
  let clientId = localStorage.getItem(key);

  if (!clientId) {
    clientId = crypto.randomUUID?.()
      ?? `client-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
    localStorage.setItem(key, clientId);
  }

  return clientId;
}

function hasPilotControl() {
  return Boolean(state.pilot.token);
}

function controlsAvailable() {
  return hasPilotControl() && state.link.portOpen;
}

function buildServoControls() {
  const servos = Array.isArray(config.servos) ? config.servos : [];

  for (const servo of servos) {
    const fragment = elements.servoTemplate.content.cloneNode(true);
    const root = fragment.querySelector(".servo-control");
    const label = fragment.querySelector("label");
    const output = fragment.querySelector("output");
    const input = fragment.querySelector("input");
    const channelLabel = fragment.querySelector(".channel-label");
    const centerButton = fragment.querySelector(".center-button");

    const inputId = `servo-${servo.key}`;
    label.htmlFor = inputId;
    label.textContent = servo.label;
    input.id = inputId;
    input.min = String(servo.min);
    input.max = String(servo.max);
    input.value = String(servo.center);
    input.dataset.channel = String(servo.channel);
    input.dataset.center = String(servo.center);
    output.htmlFor = inputId;
    output.value = `${servo.center} µs`;
    channelLabel.textContent = `CH ${servo.channel}`;
    updateRangeProgress(input);

    const sender = createServoSender(servo.channel);
    state.controls.set(servo.channel, { input, output, centerButton, sender, servo });

    input.addEventListener("input", () => {
      output.value = `${input.value} µs`;
      updateRangeProgress(input);
      sender.schedule(Number(input.value));
    });

    input.addEventListener("change", () => {
      sender.schedule(Number(input.value), true);
    });

    centerButton.addEventListener("click", () => {
      input.value = String(servo.center);
      output.value = `${servo.center} µs`;
      updateRangeProgress(input);
      sender.schedule(servo.center, true);
    });

    elements.servoControls.append(root);
  }
}

function createServoSender(channel) {
  const minimumInterval = Number(config.servoSendIntervalMilliseconds ?? 90);
  let pendingValue = null;
  let timer = 0;
  let inFlight = false;
  let lastSentAt = 0;

  async function flush() {
    timer = 0;

    if (inFlight || pendingValue === null || !controlsAvailable()) {
      return;
    }

    const value = pendingValue;
    pendingValue = null;
    inFlight = true;

    try {
      await sendServo(channel, value);
      lastSentAt = performance.now();
    } catch (error) {
      handleControlError(error);
    } finally {
      inFlight = false;
      if (pendingValue !== null && controlsAvailable()) {
        schedule(pendingValue);
      }
    }
  }

  function schedule(value, immediate = false) {
    pendingValue = value;

    if (!controlsAvailable() || inFlight || timer) {
      return;
    }

    const elapsed = performance.now() - lastSentAt;
    const delay = immediate ? 0 : Math.max(0, minimumInterval - elapsed);
    timer = window.setTimeout(flush, delay);
  }

  return { schedule };
}

async function sendServo(channel, pulseWidthMicroseconds) {
  return apiRequest("/api/control/servo", {
    method: "POST",
    headers: { "X-Pilot-Token": state.pilot.token },
    body: { channel, pulseWidthMicroseconds },
    timeout: 2200
  });
}

function handleControlError(error) {
  if (error instanceof ApiError && (error.status === 403 || error.code === "pilot_required")) {
    losePilotLease("Pilotlåset har gått förlorat.");
    return;
  }

  if (error instanceof ApiError && error.code === "command_queue_full") {
    showToast("Kommandokön är full. Senaste värdet försöks igen.", "error");
    return;
  }

  showToast(error?.message ?? "Styrkommandot kunde inte skickas.", "error");
}

function updateRangeProgress(input) {
  const min = Number(input.min);
  const max = Number(input.max);
  const value = Number(input.value);
  const progress = ((value - min) / (max - min)) * 100;
  input.style.setProperty("--range-progress", `${progress}%`);
}

function updateControlsState() {
  const enabled = controlsAvailable();

  for (const control of state.controls.values()) {
    control.input.disabled = !enabled;
    control.centerButton.disabled = !enabled;
  }

  elements.controlLock.dataset.active = String(enabled);
  elements.controlLock.lastChild.textContent = enabled ? " Aktiv" : " Låst";
}

async function refreshLinkStatus({ quiet = true } = {}) {
  try {
    const status = await apiRequest("/api/status");
    state.link = status;
    renderLinkStatus();
  } catch (error) {
    state.link.portOpen = false;
    state.link.deviceResponsive = false;
    renderLinkStatus();
    if (!quiet) {
      showToast(error?.message ?? "Kunde inte läsa systemstatus.", "error");
    }
  }
}

function renderLinkStatus() {
  const { portOpen, deviceResponsive, lastReceiveUtc, crcErrors } = state.link;

  if (portOpen && deviceResponsive) {
    elements.connectionPill.dataset.state = "online";
    elements.connectionLabel.textContent = "Ansluten";
  } else if (portOpen) {
    elements.connectionPill.dataset.state = "degraded";
    elements.connectionLabel.textContent = "Port öppen";
  } else {
    elements.connectionPill.dataset.state = "offline";
    elements.connectionLabel.textContent = "Frånkopplad";
  }

  elements.uartStatus.textContent = portOpen ? "Öppen" : "Frånkopplad";
  elements.stm32Status.textContent = deviceResponsive ? "Svarar" : "Inget aktuellt svar";
  elements.serialPortName.textContent = state.link.portName || "--";
  elements.serialPortName.title = state.link.portName || "";
  elements.pingButton.disabled = !portOpen;
  elements.lastReceive.textContent = lastReceiveUtc ? formatRelativeTime(lastReceiveUtc) : "--";
  elements.crcErrors.textContent = String(crcErrors ?? 0);
  updateControlsState();
}

async function pingStm32() {
  elements.pingButton.disabled = true;
  elements.pingButton.textContent = "Pingar…";
  elements.pingPanel.dataset.state = "pending";
  elements.pingResult.textContent = "Väntar på ACK";

  try {
    const result = await apiRequest("/api/stm32/ping", {
      method: "POST",
      timeout: 3000
    });

    elements.pingPanel.dataset.state = "success";
    elements.pingResult.textContent = `${Number(result.roundTripMilliseconds).toFixed(2)} ms · txn ${result.transactionId}`;
    showToast("STM32 svarade på ping.");
    await refreshLinkStatus();
  } catch (error) {
    elements.pingPanel.dataset.state = "error";

    if (error instanceof ApiError && error.code === "stm32_timeout") {
      elements.pingResult.textContent = "Timeout – inget ACK";
    } else if (error instanceof ApiError && error.code === "stm32_unavailable") {
      elements.pingResult.textContent = "Serieporten är inte öppen";
    } else {
      elements.pingResult.textContent = error?.message ?? "Ping misslyckades";
    }

    showToast(elements.pingResult.textContent, "error");
  } finally {
    elements.pingButton.textContent = "Pinga STM32";
    elements.pingButton.disabled = !state.link.portOpen;
  }
}

async function refreshTelemetry() {
  try {
    const telemetry = await apiRequest("/api/telemetry/latest", { timeout: 2400 });
    if (!telemetry) {
      return;
    }

    const bytes = payloadToBytes(telemetry.payload);
    elements.rawFunction.textContent = `0x${Number(telemetry.function).toString(16).padStart(2, "0").toUpperCase()}`;
    elements.rawPayload.textContent = bytes.length
      ? Array.from(bytes, byte => byte.toString(16).padStart(2, "0").toUpperCase()).join(" ")
      : "(tom)";
    elements.rawReceived.textContent = formatClockTime(telemetry.receivedAtUtc);

    renderDecodedTelemetry(decodeTelemetry(telemetry.function, bytes));
  } catch {
    // Status polling already communicates API/link failures. Keep telemetry polling quiet.
  }
}

function payloadToBytes(payload) {
  if (Array.isArray(payload)) {
    return Uint8Array.from(payload);
  }

  if (typeof payload !== "string" || payload.length === 0) {
    return new Uint8Array();
  }

  try {
    const binary = atob(payload);
    return Uint8Array.from(binary, character => character.charCodeAt(0));
  } catch {
    return new Uint8Array();
  }
}

function decodeTelemetry(functionCode, bytes) {
  // Firmware-protokollet har ännu ingen slutlig payloadlayout för batteri/djup/temperatur.
  // Lägg den typade avkodningen här när function codes och byteformat har bestämts.
  // Exempel: if (functionCode === 0x40 && bytes.length >= 2) { ... }
  void functionCode;
  void bytes;
  return {};
}

function renderDecodedTelemetry(decoded) {
  elements.batteryValue.textContent = decoded.batteryPercent == null
    ? "--"
    : `${decoded.batteryPercent}%`;
  elements.depthValue.textContent = decoded.depthMeters == null
    ? "--"
    : `${decoded.depthMeters.toFixed(1)} m`;
  elements.temperatureValue.textContent = decoded.temperatureCelsius == null
    ? "--"
    : `${decoded.temperatureCelsius.toFixed(1)}°`;
}

async function refreshPilotStatus() {
  try {
    const status = await apiRequest("/api/pilot/status", { timeout: 2400 });

    if (!hasPilotControl()) {
      state.pilot.occupiedBy = status.occupied ? status.displayName : null;
      state.pilot.expiresAtUtc = status.expiresAtUtc;
    }

    renderPilotState();
  } catch {
    // Do not discard a local lease merely because a status poll failed.
  }
}

async function claimPilot(displayName) {
  const response = await apiRequest("/api/pilot/claim", {
    method: "POST",
    body: {
      clientId: state.clientId,
      displayName: displayName || "Pilot"
    }
  });

  state.pilot.token = response.token;
  state.pilot.displayName = displayName || "Pilot";
  state.pilot.occupiedBy = state.pilot.displayName;
  state.pilot.expiresAtUtc = response.expiresAtUtc;
  state.pilot.heartbeatIntervalMilliseconds = response.heartbeatIntervalMilliseconds;

  sessionStorage.setItem("dasBoot.pilotToken", response.token);
  localStorage.setItem("dasBoot.pilotName", state.pilot.displayName);

  startPilotHeartbeat();
  renderPilotState();
}

function startPilotHeartbeat() {
  stopPilotHeartbeat();

  const requested = Number(state.pilot.heartbeatIntervalMilliseconds ?? 5000);
  const interval = Math.max(1000, requested);

  state.pilot.heartbeatTimer = window.setInterval(async () => {
    if (!state.pilot.token) {
      return;
    }

    try {
      const response = await apiRequest("/api/pilot/heartbeat", {
        method: "POST",
        body: { token: state.pilot.token },
        timeout: 2800
      });
      state.pilot.expiresAtUtc = response.expiresAtUtc;
    } catch (error) {
      if (error instanceof ApiError && error.status === 409) {
        losePilotLease("Pilotlåset löpte ut eller togs över.");
      }
    }
  }, interval);
}

function stopPilotHeartbeat() {
  if (state.pilot.heartbeatTimer) {
    window.clearInterval(state.pilot.heartbeatTimer);
    state.pilot.heartbeatTimer = 0;
  }
}

async function releasePilot({ quiet = false } = {}) {
  const token = state.pilot.token;
  clearLocalPilotLease();

  if (!token) {
    return;
  }

  try {
    await apiRequest("/api/pilot/release", {
      method: "POST",
      body: { token },
      timeout: 2200
    });
    if (!quiet) {
      showToast("Styrningen har släppts.");
    }
  } catch (error) {
    if (!quiet && !(error instanceof ApiError && error.status === 409)) {
      showToast("Låset släpps automatiskt när leasens timeout löper ut.", "error");
    }
  }

  await refreshPilotStatus();
}

function losePilotLease(message) {
  clearLocalPilotLease();
  showToast(message, "error");
  void refreshPilotStatus();
}

function clearLocalPilotLease() {
  stopPilotHeartbeat();
  state.pilot.token = null;
  state.pilot.occupiedBy = null;
  state.pilot.expiresAtUtc = null;
  sessionStorage.removeItem("dasBoot.pilotToken");
  renderPilotState();
}

function renderPilotState() {
  const ownLease = hasPilotControl();
  const occupiedByOther = !ownLease && Boolean(state.pilot.occupiedBy);

  elements.pilotButton.dataset.active = String(ownLease);
  elements.pilotButtonLabel.textContent = ownLease
    ? "Släpp kontroll"
    : occupiedByOther
      ? "Pilot upptagen"
      : "Ta kontroll";

  if (ownLease) {
    elements.pilotBanner.dataset.mode = "pilot";
    elements.pilotBannerText.textContent = `Du styr som ${state.pilot.displayName || "Pilot"}`;
  } else if (occupiedByOther) {
    elements.pilotBanner.dataset.mode = "occupied";
    elements.pilotBannerText.textContent = `${state.pilot.occupiedBy} har styrningen · du är observatör`;
  } else {
    elements.pilotBanner.dataset.mode = "observer";
    elements.pilotBannerText.textContent = "Observatörsläge · inga styrkommandon skickas";
  }

  updateControlsState();
}

function setupStream() {
  const streamUrl = String(config.streamUrl ?? "").trim();
  const streamMode = String(config.streamMode ?? "mjpeg").toLowerCase();

  if (!streamUrl) {
    setStreamLive(false, "STREAM OFFLINE");
    return;
  }

  elements.streamPlaceholder.hidden = true;

  if (streamMode === "video") {
    elements.videoStream.hidden = false;
    elements.videoStream.src = streamUrl;
    elements.videoStream.addEventListener("playing", () => setStreamLive(true));
    elements.videoStream.addEventListener("waiting", () => setStreamLive(false, "BUFFERING"));
    elements.videoStream.addEventListener("stalled", () => setStreamLive(false, "STALLED"));
    elements.videoStream.addEventListener("error", () => setStreamLive(false, "STREAM ERROR"));
    elements.videoStream.play().catch(() => setStreamLive(false, "TRYCK FÖR VIDEO"));
    elements.videoStream.addEventListener("click", () => elements.videoStream.play().catch(() => {}));
    return;
  }

  elements.mjpegStream.hidden = false;
  elements.mjpegStream.src = streamUrl;
  elements.mjpegStream.addEventListener("load", () => setStreamLive(true));
  elements.mjpegStream.addEventListener("error", () => setStreamLive(false, "STREAM ERROR"));
  setStreamLive(false, "CONNECTING");
}

function setStreamLive(isLive, label = isLive ? "LIVE" : "STREAM OFFLINE") {
  elements.liveBadge.dataset.live = String(isLive);
  elements.liveBadgeText.textContent = label;
}

async function toggleFullscreen() {
  try {
    if (document.fullscreenElement) {
      await document.exitFullscreen();
    } else {
      await elements.streamStage.requestFullscreen();
    }
  } catch {
    showToast("Helskärmsläget stöds inte av denna webbläsare.", "error");
  }
}

function formatRelativeTime(value) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return "--";
  }

  const seconds = Math.max(0, Math.round((Date.now() - date.getTime()) / 1000));
  if (seconds < 3) return "nyss";
  if (seconds < 60) return `${seconds} s sedan`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min sedan`;
  return formatClockTime(value);
}

function formatClockTime(value) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return "--";
  }

  return new Intl.DateTimeFormat("sv-SE", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit"
  }).format(date);
}

function showToast(message, kind = "info") {
  const toast = document.createElement("div");
  toast.className = "toast";
  toast.dataset.kind = kind;
  toast.textContent = message;
  elements.toastRegion.replaceChildren(toast);
  window.setTimeout(() => toast.remove(), 3600);
}

function openPilotDialog() {
  elements.pilotName.value = state.pilot.displayName;
  elements.pilotDialogError.hidden = true;
  elements.pilotDialogError.textContent = "";
  elements.pilotDialog.showModal();
  window.setTimeout(() => elements.pilotName.focus(), 0);
}

function bindEvents() {
  elements.pilotButton.addEventListener("click", () => {
    if (hasPilotControl()) {
      void releasePilot();
    } else {
      openPilotDialog();
    }
  });

  elements.pilotForm.addEventListener("submit", async event => {
    event.preventDefault();
    const displayName = elements.pilotName.value.trim() || "Pilot";

    elements.claimPilotButton.disabled = true;
    elements.pilotDialogError.hidden = true;

    try {
      await claimPilot(displayName);
      elements.pilotDialog.close();
      showToast("Du har nu styrningen.");
    } catch (error) {
      const occupiedName = error instanceof ApiError && error.status === 409
        ? error.body?.displayName
        : null;
      elements.pilotDialogError.textContent = occupiedName
        ? `${occupiedName} har redan styrningen.`
        : error?.message ?? "Kunde inte ta pilotlåset.";
      elements.pilotDialogError.hidden = false;
    } finally {
      elements.claimPilotButton.disabled = false;
    }
  });

  elements.cancelPilotButton.addEventListener("click", () => elements.pilotDialog.close());
  elements.fullscreenButton.addEventListener("click", toggleFullscreen);
  elements.pingButton.addEventListener("click", () => void pingStm32());
  elements.refreshButton.addEventListener("click", async () => {
    await Promise.all([
      refreshLinkStatus({ quiet: false }),
      refreshTelemetry(),
      refreshPilotStatus()
    ]);
  });

  window.addEventListener("pagehide", () => {
    if (!state.pilot.token) {
      return;
    }

    fetch(`${apiBaseUrl}/api/pilot/release`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ token: state.pilot.token }),
      credentials: "same-origin",
      keepalive: true
    }).catch(() => {});
  });
}

async function restorePilotLease() {
  if (!state.pilot.token) {
    return;
  }

  try {
    const response = await apiRequest("/api/pilot/heartbeat", {
      method: "POST",
      body: { token: state.pilot.token },
      timeout: 2600
    });
    state.pilot.expiresAtUtc = response.expiresAtUtc;
    startPilotHeartbeat();
  } catch {
    clearLocalPilotLease();
  }
}

async function initialize() {
  buildServoControls();
  bindEvents();
  setupStream();
  renderPilotState();
  renderLinkStatus();

  await restorePilotLease();
  await Promise.all([
    refreshLinkStatus(),
    refreshTelemetry(),
    refreshPilotStatus()
  ]);

  window.setInterval(
    () => void refreshLinkStatus(),
    Number(config.statusPollMilliseconds ?? 2000));
  window.setInterval(
    () => void refreshTelemetry(),
    Number(config.telemetryPollMilliseconds ?? 1000));
  window.setInterval(
    () => void refreshPilotStatus(),
    Number(config.pilotStatusPollMilliseconds ?? 3000));
}

void initialize();
