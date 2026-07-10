using System.Security.Cryptography;

namespace DasBoot.Api.Pilot;

public sealed class PilotSessionService(PilotLeaseOptions options)
{
    private const int MaxClientIdLength = 96;
    private const int MaxDisplayNameLength = 32;

    private readonly object _gate = new();
    private string? _token;
    private string? _clientId;
    private string? _displayName;
    private DateTimeOffset? _expiresAtUtc;

    public PilotClaimResult TryClaim(string clientId, string? displayName)
    {
        clientId = NormalizeRequired(clientId, MaxClientIdLength);
        displayName = NormalizeOptional(displayName, MaxDisplayNameLength) ?? "Pilot";

        if (clientId.Length == 0)
        {
            return PilotClaimResult.Invalid("client_id_required");
        }

        lock (_gate)
        {
            ClearIfExpiredLocked(DateTimeOffset.UtcNow);

            if (_token is not null && !string.Equals(_clientId, clientId, StringComparison.Ordinal))
            {
                return PilotClaimResult.Occupied(GetSnapshotLocked());
            }

            var now = DateTimeOffset.UtcNow;
            _token = Convert.ToHexString(RandomNumberGenerator.GetBytes(24));
            _clientId = clientId;
            _displayName = displayName;
            _expiresAtUtc = now.AddMilliseconds(options.LeaseDurationMilliseconds);

            return PilotClaimResult.Granted(
                _token,
                _expiresAtUtc.Value,
                options.HeartbeatIntervalMilliseconds);
        }
    }

    public PilotHeartbeatResult Heartbeat(string token)
    {
        if (string.IsNullOrWhiteSpace(token))
        {
            return PilotHeartbeatResult.Denied;
        }

        lock (_gate)
        {
            ClearIfExpiredLocked(DateTimeOffset.UtcNow);

            if (_token is null || !FixedTimeEquals(_token, token))
            {
                return PilotHeartbeatResult.Denied;
            }

            _expiresAtUtc = DateTimeOffset.UtcNow.AddMilliseconds(options.LeaseDurationMilliseconds);
            return new PilotHeartbeatResult(true, _expiresAtUtc);
        }
    }

    public bool HasControl(string? token)
    {
        if (string.IsNullOrWhiteSpace(token))
        {
            return false;
        }

        lock (_gate)
        {
            ClearIfExpiredLocked(DateTimeOffset.UtcNow);
            return _token is not null && FixedTimeEquals(_token, token);
        }
    }

    public bool Release(string token)
    {
        if (string.IsNullOrWhiteSpace(token))
        {
            return false;
        }

        lock (_gate)
        {
            ClearIfExpiredLocked(DateTimeOffset.UtcNow);

            if (_token is null || !FixedTimeEquals(_token, token))
            {
                return false;
            }

            ClearLocked();
            return true;
        }
    }

    public PilotSessionSnapshot GetSnapshot()
    {
        lock (_gate)
        {
            ClearIfExpiredLocked(DateTimeOffset.UtcNow);
            return GetSnapshotLocked();
        }
    }

    private PilotSessionSnapshot GetSnapshotLocked() => new(
        _token is not null,
        _displayName,
        _expiresAtUtc);

    private void ClearIfExpiredLocked(DateTimeOffset now)
    {
        if (_expiresAtUtc is not null && _expiresAtUtc <= now)
        {
            ClearLocked();
        }
    }

    private void ClearLocked()
    {
        _token = null;
        _clientId = null;
        _displayName = null;
        _expiresAtUtc = null;
    }

    private static bool FixedTimeEquals(string left, string right)
    {
        var leftBytes = System.Text.Encoding.ASCII.GetBytes(left);
        var rightBytes = System.Text.Encoding.ASCII.GetBytes(right);

        return leftBytes.Length == rightBytes.Length
            && CryptographicOperations.FixedTimeEquals(leftBytes, rightBytes);
    }

    private static string NormalizeRequired(string? value, int maxLength) =>
        NormalizeOptional(value, maxLength) ?? string.Empty;

    private static string? NormalizeOptional(string? value, int maxLength)
    {
        var normalized = value?.Trim();
        if (string.IsNullOrEmpty(normalized))
        {
            return null;
        }

        return normalized.Length <= maxLength
            ? normalized
            : normalized[..maxLength];
    }
}

public readonly record struct PilotSessionSnapshot(
    bool Occupied,
    string? DisplayName,
    DateTimeOffset? ExpiresAtUtc);

public readonly record struct PilotHeartbeatResult(
    bool Accepted,
    DateTimeOffset? ExpiresAtUtc)
{
    public static PilotHeartbeatResult Denied => new(false, null);
}

public readonly record struct PilotClaimResult(
    PilotClaimOutcome Outcome,
    string? Token,
    DateTimeOffset? ExpiresAtUtc,
    int HeartbeatIntervalMilliseconds,
    PilotSessionSnapshot CurrentSession,
    string? ErrorCode)
{
    public static PilotClaimResult Granted(
        string token,
        DateTimeOffset expiresAtUtc,
        int heartbeatIntervalMilliseconds) =>
        new(
            PilotClaimOutcome.Granted,
            token,
            expiresAtUtc,
            heartbeatIntervalMilliseconds,
            default,
            null);

    public static PilotClaimResult Occupied(PilotSessionSnapshot currentSession) =>
        new(PilotClaimOutcome.Occupied, null, null, 0, currentSession, null);

    public static PilotClaimResult Invalid(string errorCode) =>
        new(PilotClaimOutcome.Invalid, null, null, 0, default, errorCode);
}

public enum PilotClaimOutcome : byte
{
    Granted,
    Occupied,
    Invalid
}
