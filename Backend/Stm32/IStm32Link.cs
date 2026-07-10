namespace DasBoot.Api.Stm32;

public interface IStm32Link
{
    LinkSnapshot GetSnapshot();

    ValueTask<CommandReply> SendCommandAsync(
        byte function,
        ReadOnlyMemory<byte> payload,
        CancellationToken cancellationToken);
}
