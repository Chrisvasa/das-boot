using Microsoft.Extensions.Configuration;

namespace DasBoot.Api.Pilot;

public sealed class PilotLeaseOptions
{
    public int LeaseDurationMilliseconds { get; init; } = 15_000;
    public int HeartbeatIntervalMilliseconds { get; init; } = 5_000;

    public static PilotLeaseOptions FromConfiguration(IConfiguration configuration)
    {
        var lease = ReadInt(
            configuration,
            "Pilot:LeaseDurationMilliseconds",
            15_000,
            5_000,
            120_000);

        var heartbeat = ReadInt(
            configuration,
            "Pilot:HeartbeatIntervalMilliseconds",
            5_000,
            1_000,
            Math.Max(1_000, lease / 2));

        return new PilotLeaseOptions
        {
            LeaseDurationMilliseconds = lease,
            HeartbeatIntervalMilliseconds = heartbeat
        };
    }

    private static int ReadInt(
        IConfiguration configuration,
        string key,
        int fallback,
        int minimum,
        int maximum)
    {
        return int.TryParse(configuration[key], out var parsed)
            ? Math.Clamp(parsed, minimum, maximum)
            : fallback;
    }
}
