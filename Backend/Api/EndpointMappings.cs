using DasBoot.Api.Contracts;
using DasBoot.Api.Pilot;
using DasBoot.Api.Serialization;
using DasBoot.Api.Stm32;

namespace DasBoot.Api.Api;

internal static class EndpointMappings
{
    private const string PilotTokenHeader = "X-Pilot-Token";

    public static void MapDasBootEndpoints(this WebApplication app)
    {
        app.MapGet("/health/live", static () =>
            TypedResults.Ok(new HealthResponse("ok")));

        app.MapGet("/health/stm32", GetStm32Health);
        app.MapGet("/api/status", GetStatus);
        app.MapGet("/api/telemetry/latest", GetLatestTelemetry);

        app.MapGet("/api/pilot/status", GetPilotStatus);
        app.MapPost("/api/pilot/claim", ClaimPilot);
        app.MapPost("/api/pilot/heartbeat", HeartbeatPilot);
        app.MapPost("/api/pilot/release", ReleasePilot);

        app.MapPost("/api/control/servo", SetServoAsync);
    }

    private static IResult GetStm32Health(IStm32Link link)
    {
        var snapshot = link.GetSnapshot();

        return snapshot.PortOpen
            ? TypedResults.Ok(new HealthResponse(
                snapshot.DeviceResponsive ? "responsive" : "port-open"))
            : Results.Json(
                new ApiErrorResponse("stm32_unavailable", "STM32 serial port is not open."),
                ApiJsonContext.Default.ApiErrorResponse,
                statusCode: StatusCodes.Status503ServiceUnavailable);
    }

    private static IResult GetStatus(IStm32Link link)
    {
        var snapshot = link.GetSnapshot();

        return TypedResults.Ok(new LinkStatusResponse(
            snapshot.PortOpen,
            snapshot.DeviceResponsive,
            snapshot.PortName,
            snapshot.BaudRate,
            snapshot.ConnectionOpenedAtUtc,
            snapshot.LastReceiveUtc,
            snapshot.LastTransmitUtc,
            snapshot.FramesReceived,
            snapshot.FramesSent,
            snapshot.CrcErrors,
            snapshot.CommandTimeouts,
            snapshot.LastError));
    }

    private static IResult GetLatestTelemetry(TelemetryStore telemetry)
    {
        var latest = telemetry.GetLatest();
        if (latest is null)
        {
            return TypedResults.NoContent();
        }

        return TypedResults.Ok(new RawTelemetryResponse(
            latest.ReceivedAtUtc,
            latest.TransactionId,
            latest.Function,
            latest.Payload));
    }

    private static IResult GetPilotStatus(PilotSessionService pilots)
    {
        var status = pilots.GetSnapshot();
        return TypedResults.Ok(new PilotStatusResponse(
            status.Occupied,
            status.DisplayName,
            status.ExpiresAtUtc));
    }

    private static IResult ClaimPilot(
        PilotClaimRequest request,
        PilotSessionService pilots)
    {
        var result = pilots.TryClaim(request.ClientId, request.DisplayName);

        return result.Outcome switch
        {
            PilotClaimOutcome.Granted => TypedResults.Ok(new PilotClaimResponse(
                result.Token!,
                result.ExpiresAtUtc!.Value,
                result.HeartbeatIntervalMilliseconds)),

            PilotClaimOutcome.Occupied => Results.Json(
                new PilotStatusResponse(
                    result.CurrentSession.Occupied,
                    result.CurrentSession.DisplayName,
                    result.CurrentSession.ExpiresAtUtc),
                ApiJsonContext.Default.PilotStatusResponse,
                statusCode: StatusCodes.Status409Conflict),

            _ => Results.Json(
                new ApiErrorResponse(
                    result.ErrorCode ?? "invalid_pilot_request",
                    "A non-empty client id is required."),
                ApiJsonContext.Default.ApiErrorResponse,
                statusCode: StatusCodes.Status400BadRequest)
        };
    }

    private static IResult HeartbeatPilot(
        PilotTokenRequest request,
        PilotSessionService pilots)
    {
        var result = pilots.Heartbeat(request.Token);
        if (!result.Accepted || result.ExpiresAtUtc is null)
        {
            return Results.Json(
                new ApiErrorResponse("pilot_lease_lost", "The pilot lease is no longer valid."),
                ApiJsonContext.Default.ApiErrorResponse,
                statusCode: StatusCodes.Status409Conflict);
        }

        return TypedResults.Ok(new PilotHeartbeatResponse(result.ExpiresAtUtc.Value));
    }

    private static IResult ReleasePilot(
        PilotTokenRequest request,
        PilotSessionService pilots)
    {
        return pilots.Release(request.Token)
            ? TypedResults.NoContent()
            : Results.Json(
                new ApiErrorResponse("pilot_lease_lost", "The pilot lease is no longer valid."),
                ApiJsonContext.Default.ApiErrorResponse,
                statusCode: StatusCodes.Status409Conflict);
    }

    private static async Task<IResult> SetServoAsync(
        ServoCommandRequest request,
        HttpContext httpContext,
        PilotSessionService pilots,
        IStm32Link link,
        Stm32LinkOptions options,
        CancellationToken cancellationToken)
    {
        var pilotToken = httpContext.Request.Headers[PilotTokenHeader].ToString();
        if (!pilots.HasControl(pilotToken))
        {
            return Results.Json(
                new ApiErrorResponse(
                    "pilot_required",
                    "A valid pilot lease is required before control commands are accepted."),
                ApiJsonContext.Default.ApiErrorResponse,
                statusCode: StatusCodes.Status403Forbidden);
        }

        if (request.Channel > options.MaxServoChannel)
        {
            return Results.Json(
                new ApiErrorResponse(
                    "invalid_servo_channel",
                    $"Channel must be between 0 and {options.MaxServoChannel}."),
                ApiJsonContext.Default.ApiErrorResponse,
                statusCode: StatusCodes.Status400BadRequest);
        }

        if (request.PulseWidthMicroseconds < options.ServoMinPulseMicroseconds
            || request.PulseWidthMicroseconds > options.ServoMaxPulseMicroseconds)
        {
            return Results.Json(
                new ApiErrorResponse(
                    "invalid_servo_pulse",
                    $"Pulse width must be between {options.ServoMinPulseMicroseconds} and "
                    + $"{options.ServoMaxPulseMicroseconds} microseconds."),
                ApiJsonContext.Default.ApiErrorResponse,
                statusCode: StatusCodes.Status400BadRequest);
        }

        var payload = ServoPayload.Encode(
            request.Channel,
            request.PulseWidthMicroseconds);

        var reply = await link.SendCommandAsync(
            ProtocolConstants.SetServo,
            payload,
            cancellationToken);

        return reply.Kind switch
        {
            CommandReplyKind.Accepted => TypedResults.Ok(new ServoCommandResponse(
                reply.TransactionId,
                true,
                null)),

            CommandReplyKind.Rejected => Results.Json(
                new ServoCommandResponse(
                    reply.TransactionId,
                    false,
                    reply.NackReason),
                ApiJsonContext.Default.ServoCommandResponse,
                statusCode: StatusCodes.Status422UnprocessableEntity),

            CommandReplyKind.Timeout => Results.Json(
                new ApiErrorResponse(
                    "stm32_timeout",
                    "STM32 did not acknowledge the command before the timeout."),
                ApiJsonContext.Default.ApiErrorResponse,
                statusCode: StatusCodes.Status504GatewayTimeout),

            CommandReplyKind.QueueFull => Results.Json(
                new ApiErrorResponse(
                    "command_queue_full",
                    "The bounded STM32 command queue is full."),
                ApiJsonContext.Default.ApiErrorResponse,
                statusCode: StatusCodes.Status429TooManyRequests),

            _ => Results.Json(
                new ApiErrorResponse(
                    "stm32_unavailable",
                    "The STM32 serial link is unavailable."),
                ApiJsonContext.Default.ApiErrorResponse,
                statusCode: StatusCodes.Status503ServiceUnavailable)
        };
    }
}
