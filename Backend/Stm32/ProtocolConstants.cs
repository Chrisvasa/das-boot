namespace DasBoot.Api.Stm32;

internal static class ProtocolConstants
{
    public const byte Sync = 0xAB;

    public const byte Ack = 0x06;
    public const byte Nack = 0x10;

    public const byte Ping = 0x15;
    public const byte SetServo = 0x20;
    public const byte GetServo = 0x21;
    public const byte GetServoAll = 0x22;

    public const int HeaderLength = 5;
    public const int CrcLength = 2;
    public const int FrameOverhead = HeaderLength + CrcLength;
    public const int MaxPayloadLength = byte.MaxValue;
    public const int MaxFrameLength = MaxPayloadLength + FrameOverhead;
}
