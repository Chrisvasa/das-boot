namespace DasBoot.Api.Stm32;

public readonly record struct LinkSnapshot(
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

public enum CommandReplyKind
{
    Accepted,
    Rejected,
    Timeout,
    LinkUnavailable,
    QueueFull
}

public readonly record struct CommandReply(
    CommandReplyKind Kind,
    ushort TransactionId,
    byte? NackReason);
