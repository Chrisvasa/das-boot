using System.Collections.Concurrent;
using System.IO.Ports;
using System.Threading.Channels;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;

namespace DasBoot.Api.Stm32;

public sealed class Stm32LinkService : BackgroundService, IStm32Link
{
    private readonly Stm32LinkOptions _options;
    private readonly TelemetryStore _telemetry;
    private readonly ILogger<Stm32LinkService> _logger;
    private readonly Channel<OutboundCommand> _outbound;
    private readonly ConcurrentDictionary<ushort, PendingCommand> _pending = new();
    private readonly object _stateLock = new();

    private int _nextTransactionId;
    private int _connectionGeneration;
    private bool _portOpen;
    private DateTimeOffset? _connectionOpenedAtUtc;
    private DateTimeOffset? _lastReceiveUtc;
    private DateTimeOffset? _lastTransmitUtc;
    private long _framesReceived;
    private long _framesSent;
    private long _crcErrors;
    private long _commandTimeouts;
    private string? _lastError;

    public Stm32LinkService(
        Stm32LinkOptions options,
        TelemetryStore telemetry,
        ILogger<Stm32LinkService> logger)
    {
        _options = options;
        _telemetry = telemetry;
        _logger = logger;

        _outbound = Channel.CreateBounded<OutboundCommand>(new BoundedChannelOptions(
            options.CommandQueueCapacity)
        {
            SingleReader = true,
            SingleWriter = false,
            FullMode = BoundedChannelFullMode.Wait,
            AllowSynchronousContinuations = false
        });
    }

    public LinkSnapshot GetSnapshot()
    {
        lock (_stateLock)
        {
            var responsive = _portOpen
                && _lastReceiveUtc is { } lastReceive
                && DateTimeOffset.UtcNow - lastReceive
                    <= TimeSpan.FromMilliseconds(_options.ResponsiveWindowMilliseconds);

            return new LinkSnapshot(
                _portOpen,
                responsive,
                _options.PortName,
                _options.BaudRate,
                _connectionOpenedAtUtc,
                _lastReceiveUtc,
                _lastTransmitUtc,
                _framesReceived,
                _framesSent,
                _crcErrors,
                _commandTimeouts,
                _lastError);
        }
    }

    public async ValueTask<CommandReply> SendCommandAsync(
        byte function,
        ReadOnlyMemory<byte> payload,
        CancellationToken cancellationToken)
    {
        var connection = GetOpenConnection();
        if (!connection.PortOpen)
        {
            return new CommandReply(CommandReplyKind.LinkUnavailable, 0, null);
        }

        PendingCommand? pending = null;
        ushort transactionId = 0;

        for (var attempt = 0; attempt < ushort.MaxValue; attempt++)
        {
            transactionId = NextTransactionId();
            pending = new PendingCommand();

            if (_pending.TryAdd(transactionId, pending))
            {
                break;
            }

            pending = null;
        }

        if (pending is null)
        {
            return new CommandReply(CommandReplyKind.QueueFull, 0, null);
        }

        var frame = FrameCodec.Encode(transactionId, function, payload.Span);
        var command = new OutboundCommand(
            transactionId,
            connection.Generation,
            frame);

        if (!_outbound.Writer.TryWrite(command))
        {
            _pending.TryRemove(transactionId, out _);
            return new CommandReply(CommandReplyKind.QueueFull, transactionId, null);
        }

        try
        {
            return await pending.Completion.Task.WaitAsync(
                TimeSpan.FromMilliseconds(_options.CommandTimeoutMilliseconds),
                cancellationToken);
        }
        catch (TimeoutException)
        {
            RecordCommandTimeout();
            return new CommandReply(CommandReplyKind.Timeout, transactionId, null);
        }
        finally
        {
            _pending.TryRemove(transactionId, out _);
        }
    }

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        while (!stoppingToken.IsCancellationRequested)
        {
            using var serialPort = CreateSerialPort();

            try
            {
                serialPort.Open();
                var connectionGeneration = MarkPortOpened();

                _logger.LogInformation(
                    "STM32 serial port opened: {PortName} at {BaudRate} baud",
                    _options.PortName,
                    _options.BaudRate);

                using var connectionCts = CancellationTokenSource.CreateLinkedTokenSource(
                    stoppingToken);

                var stream = serialPort.BaseStream;
                var readTask = ReadLoopAsync(stream, connectionCts.Token);
                var writeTask = WriteLoopAsync(
                    stream,
                    connectionGeneration,
                    connectionCts.Token);

                var completedTask = await Task.WhenAny(readTask, writeTask);
                connectionCts.Cancel();

                try
                {
                    serialPort.Close();
                }
                catch (Exception closeError)
                {
                    _logger.LogDebug(closeError, "Error while closing serial port");
                }

                await Task.WhenAll(readTask, writeTask);
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
            {
                break;
            }
            catch (Exception error)
            {
                MarkError(error.Message);
                _logger.LogWarning(
                    error,
                    "STM32 serial link failed; reconnecting in {DelayMs} ms",
                    _options.ReconnectDelayMilliseconds);
            }
            finally
            {
                MarkPortClosed();
                FailAllPendingCommands();
            }

            try
            {
                await Task.Delay(_options.ReconnectDelayMilliseconds, stoppingToken);
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
            {
                break;
            }
        }
    }

    private SerialPort CreateSerialPort()
    {
        return new SerialPort(
            _options.PortName,
            _options.BaudRate,
            Parity.None,
            8,
            StopBits.One)
        {
            Handshake = Handshake.None,
            DtrEnable = false,
            RtsEnable = false
        };
    }

    private async Task ReadLoopAsync(Stream stream, CancellationToken cancellationToken)
    {
        var parser = new FrameParser();
        var readBuffer = GC.AllocateUninitializedArray<byte>(256);

        while (!cancellationToken.IsCancellationRequested)
        {
            var bytesRead = await stream.ReadAsync(readBuffer.AsMemory(), cancellationToken);
            if (bytesRead == 0)
            {
                throw new IOException("Serial stream closed.");
            }

            if (!parser.Append(readBuffer.AsSpan(0, bytesRead)))
            {
                MarkError("RX parser buffer overflow; parser state reset.");
                parser.Reset();
                continue;
            }

            while (true)
            {
                var parseStep = parser.TryRead(out var frame);

                if (parseStep == ParseStep.NeedMoreData)
                {
                    break;
                }

                if (parseStep == ParseStep.InvalidCrcConsumed)
                {
                    RecordCrcError();
                    continue;
                }

                RecordFrameReceived();
                DispatchFrame(frame);
            }
        }
    }

    private async Task WriteLoopAsync(
        Stream stream,
        int connectionGeneration,
        CancellationToken cancellationToken)
    {
        while (await _outbound.Reader.WaitToReadAsync(cancellationToken))
        {
            while (_outbound.Reader.TryRead(out var command))
            {
                if (!_pending.ContainsKey(command.TransactionId))
                {
                    continue;
                }

                if (command.ConnectionGeneration != connectionGeneration)
                {
                    if (_pending.TryRemove(command.TransactionId, out var stalePending))
                    {
                        stalePending.Completion.TrySetResult(new CommandReply(
                            CommandReplyKind.LinkUnavailable,
                            command.TransactionId,
                            null));
                    }

                    continue;
                }

                try
                {
                    await stream.WriteAsync(command.Frame.AsMemory(), cancellationToken);
                    RecordFrameSent();
                }
                catch
                {
                    if (_pending.TryRemove(command.TransactionId, out var pending))
                    {
                        pending.Completion.TrySetResult(new CommandReply(
                            CommandReplyKind.LinkUnavailable,
                            command.TransactionId,
                            null));
                    }

                    throw;
                }
            }
        }
    }

    private void DispatchFrame(ProtocolFrame frame)
    {
        if (frame.Function == ProtocolConstants.Ack)
        {
            if (_pending.TryRemove(frame.TransactionId, out var pending))
            {
                pending.Completion.TrySetResult(new CommandReply(
                    CommandReplyKind.Accepted,
                    frame.TransactionId,
                    null));
            }

            return;
        }

        if (frame.Function == ProtocolConstants.Nack)
        {
            var reason = frame.Payload.Length > 0 ? frame.Payload[0] : (byte?)null;

            if (_pending.TryRemove(frame.TransactionId, out var pending))
            {
                pending.Completion.TrySetResult(new CommandReply(
                    CommandReplyKind.Rejected,
                    frame.TransactionId,
                    reason));
            }

            return;
        }

        // Proposed convention: device-originated, unsolicited telemetry uses txn = 0.
        // Nonzero txn values are reserved for replies to host-owned transactions.
        if (frame.TransactionId == 0)
        {
            _telemetry.Update(frame);
            return;
        }

        _logger.LogDebug(
            "Ignoring unexpected non-control frame func=0x{Function:X2}, txn={TransactionId}",
            frame.Function,
            frame.TransactionId);
    }

    private ushort NextTransactionId()
    {
        ushort transactionId;

        do
        {
            transactionId = unchecked((ushort)Interlocked.Increment(ref _nextTransactionId));
        }
        while (transactionId == 0);

        return transactionId;
    }

    private (bool PortOpen, int Generation) GetOpenConnection()
    {
        lock (_stateLock)
        {
            return (_portOpen, _connectionGeneration);
        }
    }

    private int MarkPortOpened()
    {
        lock (_stateLock)
        {
            _connectionGeneration++;
            _portOpen = true;
            _connectionOpenedAtUtc = DateTimeOffset.UtcNow;
            _lastError = null;
            return _connectionGeneration;
        }
    }

    private void MarkPortClosed()
    {
        lock (_stateLock)
        {
            _portOpen = false;
            _connectionOpenedAtUtc = null;
        }
    }

    private void RecordFrameReceived()
    {
        lock (_stateLock)
        {
            _framesReceived++;
            _lastReceiveUtc = DateTimeOffset.UtcNow;
        }
    }

    private void RecordFrameSent()
    {
        lock (_stateLock)
        {
            _framesSent++;
            _lastTransmitUtc = DateTimeOffset.UtcNow;
        }
    }

    private void RecordCrcError()
    {
        lock (_stateLock)
        {
            _crcErrors++;
        }
    }

    private void RecordCommandTimeout()
    {
        lock (_stateLock)
        {
            _commandTimeouts++;
        }
    }

    private void MarkError(string message)
    {
        lock (_stateLock)
        {
            _lastError = message;
        }
    }

    private void FailAllPendingCommands()
    {
        foreach (var pair in _pending)
        {
            if (_pending.TryRemove(pair.Key, out var pending))
            {
                pending.Completion.TrySetResult(new CommandReply(
                    CommandReplyKind.LinkUnavailable,
                    pair.Key,
                    null));
            }
        }

        while (_outbound.Reader.TryRead(out _))
        {
        }
    }

    private sealed class PendingCommand
    {
        public TaskCompletionSource<CommandReply> Completion { get; } = new(
            TaskCreationOptions.RunContinuationsAsynchronously);
    }

    private readonly record struct OutboundCommand(
        ushort TransactionId,
        int ConnectionGeneration,
        byte[] Frame);
}
