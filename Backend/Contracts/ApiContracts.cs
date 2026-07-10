namespace DasBoot.Api.Contracts;

public readonly record struct HealthResponse(string Status);

public readonly record struct LinkStatusResponse(
    bool PortOpen,
    bool DeviceResponsive,
    string PortName,
    int BaudRate,
    DateTimeOffset? ConnectionOpenedAtUtc,
    DateTimeOffset? LastReceiveUtc,
    DateTimeOffset? LastTransmitUtc,
    long FramesReceived,
    long FramesSent,
    long CrcErrors,
    long CommandTimeouts,
    string? LastError);

public readonly record struct ServoCommandRequest(
    byte Channel,
    ushort PulseWidthMicroseconds);

public readonly record struct ServoCommandResponse(
    ushort TransactionId,
    bool Accepted,
    byte? NackReason);

public readonly record struct RawTelemetryResponse(
    DateTimeOffset ReceivedAtUtc,
    ushort TransactionId,
    byte Function,
    byte[] Payload);

public readonly record struct PilotClaimRequest(
    string ClientId,
    string? DisplayName);

public readonly record struct PilotClaimResponse(
    string Token,
    DateTimeOffset ExpiresAtUtc,
    int HeartbeatIntervalMilliseconds);

public readonly record struct PilotTokenRequest(string Token);

public readonly record struct PilotHeartbeatResponse(DateTimeOffset ExpiresAtUtc);

public readonly record struct PilotStatusResponse(
    bool Occupied,
    string? DisplayName,
    DateTimeOffset? ExpiresAtUtc);

public readonly record struct ApiErrorResponse(
    string Code,
    string Message);
