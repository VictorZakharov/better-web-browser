param([string] $ReadyFile)
# Owned timeout fixture: exposes its PID for the launcher cleanup assertion, not readiness.
[IO.File]::WriteAllText($ReadyFile + '.pid', [string]$PID)
Start-Sleep -Seconds 60
