using System.Text.Json.Serialization;
using DasBoot.Api.Contracts;

namespace DasBoot.Api.Serialization;

[JsonSerializable(typeof(HealthResponse))]
[JsonSerializable(typeof(LinkStatusResponse))]
[JsonSerializable(typeof(ServoCommandRequest))]
[JsonSerializable(typeof(ServoCommandResponse))]
[JsonSerializable(typeof(RawTelemetryResponse))]
[JsonSerializable(typeof(PilotClaimRequest))]
[JsonSerializable(typeof(PilotClaimResponse))]
[JsonSerializable(typeof(PilotStatusResponse))]
[JsonSerializable(typeof(PilotTokenRequest))]
[JsonSerializable(typeof(PilotHeartbeatResponse))]
[JsonSerializable(typeof(PilotStatusResponse))]
[JsonSerializable(typeof(ApiErrorResponse))]
internal partial class ApiJsonContext : JsonSerializerContext
{
}
