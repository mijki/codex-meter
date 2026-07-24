$ErrorActionPreference = 'Stop'

try {
    $raw = [Console]::In.ReadToEnd()
    if ([string]::IsNullOrWhiteSpace($raw) -or $raw.Length -gt 262144) {
        exit 0
    }

    $source = $raw | ConvertFrom-Json
    $normalized = [ordered]@{
        event_id        = [guid]::NewGuid().ToString('N')
        hook_event_name = [string]$source.hook_event_name
        timestamp       = [DateTimeOffset]::UtcNow.ToString('o')
        session_id      = [string]$source.session_id
    }

    foreach ($field in @('turn_id', 'model', 'permission_mode', 'tool_name', 'subagent_id', 'subagent_type')) {
        if ($null -ne $source.$field -and -not [string]::IsNullOrWhiteSpace([string]$source.$field)) {
            $normalized[$field] = [string]$source.$field
        }
    }

    if ($null -ne $source.success) {
        $normalized['succeeded'] = [bool]$source.success
    }
    if ($null -ne $source.duration_ms) {
        $normalized['duration_ms'] = [long]$source.duration_ms
    }

    $payload = $normalized | ConvertTo-Json -Compress
    $dataRoot = Join-Path $env:LOCALAPPDATA 'CodexMeter'
    $secretPath = Join-Path $dataRoot 'collector-secret'
    $delivered = $false

    if (Test-Path -LiteralPath $secretPath -PathType Leaf) {
        $secret = (Get-Content -Raw -LiteralPath $secretPath).Trim()
        if (-not [string]::IsNullOrWhiteSpace($secret)) {
            try {
                Invoke-RestMethod -Uri 'http://127.0.0.1:9464/hooks' -Method Post -ContentType 'application/json' -Headers @{ Authorization = "Bearer $secret" } -Body $payload -TimeoutSec 1 | Out-Null
                $delivered = $true
            }
            catch {
                $delivered = $false
            }
        }
    }

    if (-not $delivered) {
        $spool = Join-Path $dataRoot 'spool'
        New-Item -ItemType Directory -Force -Path $spool | Out-Null
        $file = Join-Path $spool ("hook-{0}.json" -f $normalized.event_id)
        [IO.File]::WriteAllText($file, $payload, [Text.UTF8Encoding]::new($false))
    }
}
catch {
    # Hooks are fail-open. No successful-path or model-visible output is emitted.
}

exit 0
