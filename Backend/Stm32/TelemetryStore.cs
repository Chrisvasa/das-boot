namespace DasBoot.Api.Stm32;

public sealed record RawTelemetrySample(
    DateTimeOffset ReceivedAtUtc,
    ushort TransactionId,
    byte Function,
    byte[] Payload);

public sealed class TelemetryStore
{
    private RawTelemetrySample? _latest;

    public RawTelemetrySample? GetLatest() => Volatile.Read(ref _latest);

    internal void Update(ProtocolFrame frame)
    {
        var sample = new RawTelemetrySample(
            DateTimeOffset.UtcNow,
            frame.TransactionId,
            frame.Function,
            frame.Payload);

        Volatile.Write(ref _latest, sample);
    }
}
