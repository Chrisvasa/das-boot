using Microsoft.Extensions.Configuration;

namespace DasBoot.Api.Stm32;

public sealed class Stm32LinkOptions
{
    public string PortName { get; init; } = "/dev/serial0";
    public int BaudRate { get; init; } = 115200;
    public int CommandTimeoutMilliseconds { get; init; } = 750;
    public int ReconnectDelayMilliseconds { get; init; } = 1000;
    public int ResponsiveWindowMilliseconds { get; init; } = 3000;
    public int CommandQueueCapacity { get; init; } = 32;
    public int MaxServoChannel { get; init; } = 15;
    public int ServoMinPulseMicroseconds { get; init; } = 500;
    public int ServoMaxPulseMicroseconds { get; init; } = 2500;

    public static Stm32LinkOptions FromConfiguration(IConfiguration configuration)
    {
        return new Stm32LinkOptions
        {
            PortName = configuration["Stm32:PortName"] ?? "/dev/serial0",
            BaudRate = ReadInt(configuration, "Stm32:BaudRate", 115200, 1, 4_000_000),
            CommandTimeoutMilliseconds = ReadInt(
                configuration, "Stm32:CommandTimeoutMilliseconds", 750, 50, 60_000),
            ReconnectDelayMilliseconds = ReadInt(
                configuration, "Stm32:ReconnectDelayMilliseconds", 1000, 100, 60_000),
            ResponsiveWindowMilliseconds = ReadInt(
                configuration, "Stm32:ResponsiveWindowMilliseconds", 3000, 100, 60_000),
            CommandQueueCapacity = ReadInt(
                configuration, "Stm32:CommandQueueCapacity", 32, 1, 1024),
            MaxServoChannel = ReadInt(
                configuration, "Stm32:MaxServoChannel", 15, 0, byte.MaxValue),
            ServoMinPulseMicroseconds = ReadInt(
                configuration, "Stm32:ServoMinPulseMicroseconds", 500, 1, ushort.MaxValue),
            ServoMaxPulseMicroseconds = ReadInt(
                configuration, "Stm32:ServoMaxPulseMicroseconds", 2500, 1, ushort.MaxValue)
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
