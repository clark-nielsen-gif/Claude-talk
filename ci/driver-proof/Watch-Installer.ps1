# Runs an installer, and while it runs records what it is waiting for:
# desktop screenshots, visible window titles and child processes. Kills it
# after -TimeoutSeconds so a hidden dialog cannot hang the job.
param(
    [Parameter(Mandatory)] [string] $Exe,
    [string[]] $Arguments = @(),
    [Parameter(Mandatory)] [string] $OutDir,
    [string] $Name = 'installer',
    [int] $TimeoutSeconds = 150
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
New-Item -ItemType Directory -Force $OutDir | Out-Null

function Save-Screen([string] $file) {
    try {
        $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
        $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size)
        $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
        $g.Dispose(); $bmp.Dispose()
    } catch { "screenshot failed: $($_.Exception.Message)" | Add-Content (Join-Path $OutDir "$Name-notes.txt") }
}

$sig = Get-AuthenticodeSignature $Exe
"signer: $($sig.SignerCertificate.Subject) / status $($sig.Status) / notBefore $($sig.SignerCertificate.NotBefore)" |
    Tee-Object (Join-Path $OutDir "$Name-notes.txt")

$start = Get-Date
$p = Start-Process $Exe -ArgumentList $Arguments -PassThru
$i = 0
while (-not $p.HasExited -and ((Get-Date) - $start).TotalSeconds -lt $TimeoutSeconds) {
    Start-Sleep -Seconds 10
    $i++
    $wins = Get-Process | Where-Object { $_.MainWindowTitle } | ForEach-Object { "$($_.ProcessName) [$($_.Id)]: $($_.MainWindowTitle)" }
    $kids = Get-CimInstance Win32_Process | Where-Object { $_.ParentProcessId -eq $p.Id -or $_.Name -match 'drvinst|rundll32|DrvInst|newdev|setup' } |
        ForEach-Object { "$($_.Name) [$($_.ProcessId)] parent=$($_.ParentProcessId) $($_.CommandLine)" }
    "--- t+$([int]((Get-Date) - $start).TotalSeconds)s windows:`n$($wins -join "`n")`nprocesses:`n$($kids -join "`n")" |
        Tee-Object -Append (Join-Path $OutDir "$Name-notes.txt")
    if ($i -in 1, 3, 9) { Save-Screen (Join-Path $OutDir "$Name-t$i.png") }
}
if (-not $p.HasExited) {
    "TIMEOUT after $TimeoutSeconds s: installer still running, killing it." | Tee-Object -Append (Join-Path $OutDir "$Name-notes.txt")
    Save-Screen (Join-Path $OutDir "$Name-timeout.png")
    Get-CimInstance Win32_Process | Where-Object { $_.ParentProcessId -eq $p.Id } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
    Stop-Process -Id $p.Id -Force
    'timeout'
} else {
    "exit code $($p.ExitCode) after $([int]((Get-Date) - $start).TotalSeconds) s" | Tee-Object -Append (Join-Path $OutDir "$Name-notes.txt")
    "exit=$($p.ExitCode)"
}
