# Guarded release build for gakumas-screenshot.
#
# A running instance of gakumas-screenshot.exe holds an exclusive lock on the
# output binary, so a plain `cargo build --release` compiles for minutes and only
# THEN fails at the link step ("failed to remove file ... gakumas-screenshot.exe").
# This wrapper checks for a running instance FIRST and aborts in a second, so no
# compile time is wasted.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts/build.ps1            # cargo build --release
#   powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Kill     # stop a running instance first
#   powershell -ExecutionPolicy Bypass -File scripts/build.ps1 test       # forward args: cargo test
#
# -Kill stops a running instance automatically. WITHOUT it, a running instance is
# a hard error (we never kill the user's app unprompted — it might be mid-run).

param(
    [switch]$Kill,
    [Parameter(ValueFromRemainingArguments = $true)] $CargoArgs
)

$proc = Get-Process gakumas-screenshot -ErrorAction SilentlyContinue
if ($proc) {
    if ($Kill) {
        Write-Host "Stopping running gakumas-screenshot (PID $($proc.Id))..." -ForegroundColor Yellow
        $proc | Stop-Process -Force
        Start-Sleep -Milliseconds 600
    }
    else {
        Write-Host "ERROR: gakumas-screenshot.exe is running (PID $($proc.Id))." -ForegroundColor Red
        Write-Host "It locks the output binary, so the build would fail at the link step after a full compile." -ForegroundColor Red
        Write-Host "Close it (tray -> 終了) and re-run, or pass -Kill to stop it automatically." -ForegroundColor Red
        exit 1
    }
}

if (-not $CargoArgs -or $CargoArgs.Count -eq 0) {
    $CargoArgs = @('build', '--release')
}

Write-Host "> cargo $($CargoArgs -join ' ')" -ForegroundColor Cyan
& cargo @CargoArgs
exit $LASTEXITCODE
