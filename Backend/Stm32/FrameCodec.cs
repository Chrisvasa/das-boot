using System.Buffers.Binary;

namespace DasBoot.Api.Stm32;

internal static class FrameCodec
{
    public static byte[] Encode(
        ushort transactionId,
        byte function,
        ReadOnlySpan<byte> payload)
    {
        if (payload.Length > ProtocolConstants.MaxPayloadLength)
        {
            throw new ArgumentOutOfRangeException(
                nameof(payload),
                $"Payload may contain at most {ProtocolConstants.MaxPayloadLength} bytes.");
        }

        var frame = GC.AllocateUninitializedArray<byte>(
            ProtocolConstants.FrameOverhead + payload.Length);

        frame[0] = ProtocolConstants.Sync;
        BinaryPrimitives.WriteUInt16LittleEndian(frame.AsSpan(1, 2), transactionId);
        frame[3] = function;
        frame[4] = checked((byte)payload.Length);
        payload.CopyTo(frame.AsSpan(ProtocolConstants.HeaderLength));

        var crcOffset = ProtocolConstants.HeaderLength + payload.Length;
        var crc = Crc16Modbus.Compute(frame.AsSpan(0, crcOffset));
        BinaryPrimitives.WriteUInt16LittleEndian(frame.AsSpan(crcOffset, 2), crc);

        return frame;
    }
}
