using System.Buffers.Binary;

namespace DasBoot.Api.Stm32;

internal static class ServoPayload
{
    // Firmware payload: [servo_num:u8][target:u16 little-endian].
    // At 50 Hz with a 20,000-unit PWM period, values such as 500-2500 map
    // directly to the usual servo pulse widths in microseconds.
    public static byte[] Encode(byte channel, ushort pulseWidthMicroseconds)
    {
        var payload = new byte[3];
        payload[0] = channel;
        BinaryPrimitives.WriteUInt16LittleEndian(payload.AsSpan(1, 2), pulseWidthMicroseconds);
        return payload;
    }
}
