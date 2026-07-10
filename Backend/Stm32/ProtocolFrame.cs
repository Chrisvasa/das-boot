namespace DasBoot.Api.Stm32;

internal readonly record struct ProtocolFrame(
    ushort TransactionId,
    byte Function,
    byte[] Payload);
