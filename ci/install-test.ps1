# Install test of the real ClaudeTalk-Setup.exe on a clean runner:
# silent install (which downloads and installs VB-CABLE from vb-audio.com),
# devices named, app starts (start time, CPU, memory), silent uninstall
# removes everything again. Writes results/install.json; exit 1 on failure.
param([Parameter(Mandatory)] [string] $Setup, [string] $Out = 'results')
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory $Out -Force | Out-Null
$r = [ordered]@{}
$fail = New-Object System.Collections.Generic.List[string]
function Check([string] $name, [bool] $ok, $detail) {
    $r[$name] = [ordered]@{ pass = $ok; detail = $detail }
    Write-Host ("[{0}] {1}: {2}" -f ($(if ($ok) { 'PASS' } else { 'FAIL' })), $name, ($detail | ConvertTo-Json -Compress -Depth 5))
    if (-not $ok) { $fail.Add($name) }
}
function Endpoints {
    # separate process: the COM type in List-Endpoints must not clash
    $f = Join-Path $env:RUNNER_TEMP ("ep-" + [guid]::NewGuid() + ".json")
    powershell -NoProfile -ExecutionPolicy Bypass -File "$PSScriptRoot/driver-proof/List-Endpoints.ps1" -OutFile $f | Out-Null
    if (Test-Path $f) { @(Get-Content $f -Raw -Encoding utf8 | ConvertFrom-Json | ForEach-Object { $_ }) } else { @() }
}
$dash = [char]0x2013

# Server images ship with the audio service disabled; a PC has it running.
Set-Service Audiosrv -StartupType Automatic
Start-Service Audiosrv
$cs = 'SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone'
foreach ($k in "HKLM:\$cs", "HKCU:\$cs", "HKCU:\$cs\NonPackaged") {
    New-Item -Path $k -Force | Out-Null
    Set-ItemProperty -Path $k -Name Value -Value 'Allow'
}
$before = Endpoints
$r.endpoints_before = $before

try {
# 1. silent install
$sw = [Diagnostics.Stopwatch]::StartNew()
$p = Start-Process $Setup -ArgumentList '/S' -PassThru
if (-not $p.WaitForExit(600000)) { $p.Kill(); Check 'install_finishes' $false 'timeout after 10 min' }
else { Check 'install_finishes' ($p.ExitCode -eq 0) @{ exit = $p.ExitCode; seconds = [math]::Round($sw.Elapsed.TotalSeconds, 1) } }
$dir = Join-Path $env:ProgramFiles 'Claude Talk'
$exe = Join-Path $dir 'claude-talk.exe'
Check 'program_installed' (Test-Path $exe) $exe
$uninstKey = Get-ChildItem 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall' |
    Where-Object { (Get-ItemProperty $_.PSPath).DisplayName -eq 'Claude Talk' } | Select-Object -First 1
Check 'listed_in_installed_apps' ($null -ne $uninstKey) $(if ($uninstKey) { $uninstKey.PSChildName } else { 'missing' })

Start-Sleep 3
$after = Endpoints
$r.endpoints_after = $after
$cap = $after | Where-Object { $_.Flow -eq 'Capture' -and $_.FriendlyName -like "Claude Talk $dash til Discord*" }
$ren = $after | Where-Object { $_.Flow -eq 'Render' -and $_.FriendlyName -like "Claude Talk $dash intern*" }
Check 'discord_microphone_named' ($null -ne $cap) @{ capture = @($cap | ForEach-Object FriendlyName); render = @($ren | ForEach-Object FriendlyName) }
Check 'cable_render_named' ($null -ne $ren) @($ren | ForEach-Object FriendlyName)
$marker = Join-Path $env:ProgramFiles 'Claude Talk\vbcable\installed-by-claude-talk'
Check 'cable_marked_as_ours' (Test-Path $marker) $marker
# the cable's own driver (MEDIA class), not the audio endpoints Windows creates for it
$driver = Get-CimInstance Win32_PnPSignedDriver | Where-Object { $_.DeviceClass -eq 'MEDIA' -and ($_.DeviceName -like '*VB-Audio*' -or $_.Manufacturer -like '*VB-Audio*') } | Select-Object -First 1 DeviceName, DriverVersion, Signer, Manufacturer
Check 'driver_production_signed' ($null -ne $driver -and $driver.Signer -like '*Microsoft*') $driver

# 2. the app starts: time to the audio engine, CPU and memory while idle
$logs = Join-Path $env:LOCALAPPDATA 'Claude Talk\logs'
if (Test-Path $logs) { Remove-Item $logs -Recurse -Force }
$t0 = Get-Date
# as after a Windows start: in the tray (window hidden)
$app = Start-Process $exe -ArgumentList "--minimized" -PassThru
$started = $null
for ($i = 0; $i -lt 100 -and -not $started; $i++) {
    Start-Sleep -Milliseconds 100
    $log = Get-ChildItem $logs -Filter *.log -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($log -and (Select-String -Path $log.FullName -Pattern 'audio engine started' -Quiet)) { $started = (Get-Date) }
}
$startS = if ($started) { [math]::Round(($started - $t0).TotalSeconds, 2) } else { $null }
$exited = $app.HasExited
$crash = if ($exited) {
    Get-WinEvent -FilterHashtable @{ LogName = 'Application'; StartTime = $t0 } -ErrorAction SilentlyContinue |
        Where-Object { $_.Message -like '*claude-talk*' } | Select-Object -First 3 | ForEach-Object { $_.Message }
} else { $null }
Check 'app_starts' ($null -ne $started -and -not $exited) @{ seconds_to_audio_engine = $startS; exited = $exited; exit_code = $(if ($exited) { $app.ExitCode } else { $null }); events = $crash }
Get-ChildItem $logs -ErrorAction SilentlyContinue | Copy-Item -Destination $Out
if ($exited) { throw 'the app did not keep running' }
Start-Sleep 5
# the program and its WebView2 helpers (none while the Claude window is closed)
# the program and all its WebView2 processes (renderers are grandchildren)
function Tree {
    $all = Get-CimInstance Win32_Process
    $ids = @($app.Id); $grew = $true
    while ($grew) {
        $more = @($all | Where-Object { $ids -contains $_.ParentProcessId -and $ids -notcontains $_.ProcessId } | ForEach-Object ProcessId)
        $grew = $more.Count -gt 0; $ids += $more
    }
    $all | Where-Object { $ids -contains $_.ProcessId }
}
function CpuSeconds($p) { ($p | ForEach-Object { $_.TotalProcessorTime.TotalSeconds } | Measure-Object -Sum).Sum }
$before = @{}; Get-Process -Id (Tree).ProcessId | ForEach-Object { $before[$_.Id] = $_.TotalProcessorTime.TotalSeconds }
$cpu0 = ($before.Values | Measure-Object -Sum).Sum
$w0 = Get-Date
Start-Sleep 10
$procs = Get-Process -Id (Tree).ProcessId
$cpu1 = CpuSeconds $procs
# where the time and memory go, per process (CPU % of one core)
$span = ((Get-Date) - $w0).TotalSeconds
$perProcess = @($procs | ForEach-Object {
    $was = if ($before.ContainsKey($_.Id)) { $before[$_.Id] } else { 0 }
    '{0} {1}: cpu {2}% ram {3} MB' -f $_.ProcessName, $_.Id, [math]::Round(100 * ($_.TotalProcessorTime.TotalSeconds - $was) / $span, 2), [math]::Round($_.WorkingSet64 / 1MB, 1)
})
$cores = [Environment]::ProcessorCount
$cpuPct = [math]::Round(100 * ($cpu1 - $cpu0) / ((Get-Date) - $w0).TotalSeconds / $cores, 2)
$ramMB = [math]::Round(($procs | Measure-Object WorkingSet64 -Sum).Sum / 1MB, 1)
Check 'performance' ($startS -lt 2 -and $cpuPct -lt 3 -and $ramMB -lt 150) @{ start_s = $startS; cpu_percent = $cpuPct; ram_mb = $ramMB; processes = $procs.Count; cores = $cores; per_process = $perProcess }
function AutostartEntries($root = 'HKCU') {
    $k = Get-Item "$($root):\Software\Microsoft\Windows\CurrentVersion\Run" -ErrorAction SilentlyContinue
    if (-not $k) { return @() }
    @($k.GetValueNames() | Where-Object { "$($k.GetValue($_))" -like '*claude-talk.exe*' } | ForEach-Object { "$_ = $($k.GetValue($_))" })
}
$auto = AutostartEntries
$autoAll = AutostartEntries 'HKLM'
# for this user only (the test runs elevated: nothing may land in HKLM),
# with the path quoted (it contains a space)
Check 'autostart_registered' ($auto.Count -gt 0 -and $autoAll.Count -eq 0 -and ($auto -join ' ') -like '*"C:\Program Files\Claude Talk\claude-talk.exe" --minimized*') @{ user = $auto; all_users = $autoAll }
Get-ChildItem $logs -ErrorAction SilentlyContinue | Copy-Item -Destination $Out
Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
Start-Sleep 2

# 3. silent uninstall
$un = Join-Path $dir 'uninstall.exe'
$p = Start-Process $un -ArgumentList '/S' -PassThru
$p.WaitForExit(300000) | Out-Null
# the NSIS uninstaller re-launches itself from %TEMP%: wait for the files to go
for ($i = 0; $i -lt 120 -and (Test-Path $exe); $i++) { Start-Sleep 1 }
Start-Sleep 5
Check 'program_removed' (-not (Test-Path $exe)) @{ dir_exists = (Test-Path $dir) }
$gone = Get-ChildItem 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall' |
    Where-Object { (Get-ItemProperty $_.PSPath).DisplayName -eq 'Claude Talk' }
Check 'removed_from_installed_apps' ($null -eq $gone) $null
$auto = @(AutostartEntries) + @(AutostartEntries 'HKLM')
Check 'autostart_removed' ($auto.Count -eq 0) $auto
$end = Endpoints
$r.endpoints_end = $end
$left = @($end | Where-Object { $_.FriendlyName -like '*Claude Talk*' -or $_.FriendlyName -like '*VB-Audio Virtual Cable*' })
Check 'virtual_microphone_removed' ($left.Count -eq 0) @($left | ForEach-Object FriendlyName)
Check 'cable_files_removed' (-not (Test-Path (Split-Path $marker))) (Split-Path $marker)

} catch {
    Check 'script_error' $false ($_ | Out-String)
}
$r.pass = ($fail.Count -eq 0)
$r.failed = $fail
$r | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 (Join-Path $Out 'install.json')
if ($fail.Count -gt 0) { Write-Host "FAILED: $($fail -join ', ')"; exit 1 }
Write-Host 'install test passed'
