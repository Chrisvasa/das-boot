using System.Buffers.Binary;

namespace DasBoot.Api.Stm32;

internal enum ParseStep
{
    NeedMoreData,
    FrameReady,
    InvalidCrcConsumed
}

internal sealed class FrameParser
{
    private readonly byte[] _buffer = new byte[ProtocolConstants.MaxFrameLength * 2];
    private int _length;

    public bool Append(ReadOnlySpan<byte> data)
    {
        if (data.Length > _buffer.Length - _length)
        {
            _length = 0;
            return false;
        }

        data.CopyTo(_buffer.AsSpan(_length));
        _length += data.Length;
        return true;
    }

    public ParseStep TryRead(out ProtocolFrame frame)
    {
        frame = default;

        if (_length == 0)
        {
            return ParseStep.NeedMoreData;
        }

        var syncOffset = _buffer.AsSpan(0, _length).IndexOf(ProtocolConstants.Sync);
        if (syncOffset < 0)
        {
            _length = 0;
            return ParseStep.NeedMoreData;
        }

        if (syncOffset > 0)
        {
            Consume(syncOffset);
        }

        if (_length < ProtocolConstants.HeaderLength)
        {
            return ParseStep.NeedMoreData;
        }

        var payloadLength = _buffer[4];
        var frameLength = ProtocolConstants.FrameOverhead + payloadLength;

        if (_length < frameLength)
        {
            return ParseStep.NeedMoreData;
        }

        var crcOffset = ProtocolConstants.HeaderLength + payloadLength;
        var expectedCrc = BinaryPrimitives.ReadUInt16LittleEndian(
            _buffer.AsSpan(crcOffset, ProtocolConstants.CrcLength));
        var actualCrc = Crc16Modbus.Compute(_buffer.AsSpan(0, crcOffset));

        if (expectedCrc != actualCrc)
        {
            Consume(1);
            return ParseStep.InvalidCrcConsumed;
        }

        var payload = payloadLength == 0
            ? Array.Empty<byte>()
            : _buffer.AsSpan(ProtocolConstants.HeaderLength, payloadLength).ToArray();

        frame = new ProtocolFrame(
            BinaryPrimitives.ReadUInt16LittleEndian(_buffer.AsSpan(1, 2)),
            _buffer[3],
            payload);

        Consume(frameLength);
        return ParseStep.FrameReady;
    }

    public void Reset() => _length = 0;

    private void Consume(int count)
    {
        if (count >= _length)
        {
            _length = 0;
            return;
        }

        _buffer.AsSpan(count, _length - count).CopyTo(_buffer);
        _length -= count;
    }
}
