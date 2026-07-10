using System.Buffers.Binary;

namespace DasBoot.Api.Stm32;

internal static class ServoPayload
{
    // Proposed payload contract until the STM32 command dispatcher defines it:
    // [channel:u8][pulse_width_us:u16 little-endian]
    public static byte[] Encode(byte channel, ushort pulseWidthMicroseconds)
    {
        var payload = new byte[3];
        payload[0] = channel;
        BinaryPrimitives.WriteUInt16LittleEndian(payload.AsSpan(1, 2), pulseWidthMicroseconds);
        return payload;
    }
}
