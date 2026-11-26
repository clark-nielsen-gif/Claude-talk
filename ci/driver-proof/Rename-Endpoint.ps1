# Renames an audio endpoint (the name Discord/Chrome/Windows show) by
# writing its MMDevices property store. Those keys are writable only by
# SYSTEM, so when run as an administrator the script re-launches itself
# as SYSTEM through a one-shot scheduled task, then restarts AudioSrv.
param(
    [Parameter(Mandatory)] [ValidateSet('Capture', 'Render')] [string] $Flow,
    [Parameter(Mandatory)] [string] $MatchInterface,   # regex on current interface name
    [Parameter(Mandatory)] [string] $MatchDeviceDesc,  # regex on current device description
    [Parameter(Mandatory)] [string] $NewDeviceDesc,
    [AllowEmptyString()] [string] $NewInterface = $null,
    [switch] $AsSystem
)
$ErrorActionPreference = 'Stop'
$PK_DESC = '{a45c254e-df1c-4efd-8020-67d146a850e0},2'
$PK_IFACE = '{b3f8fa53-0004-438e-9003-51a46e139bfc},6'
$root = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$Flow"

$targets = Get-ChildItem $root | Where-Object {
    $p = Get-ItemProperty (Join-Path $_.PSPath 'Properties') -ErrorAction SilentlyContinue
    $p.$PK_IFACE -match $MatchInterface -and $p.$PK_DESC -match $MatchDeviceDesc
}
if (-not $targets) { throw "No $Flow endpoint matches interface '$MatchInterface' / desc '$MatchDeviceDesc'." }

$isSystem = ([Security.Principal.WindowsIdentity]::GetCurrent().User.Value -eq 'S-1-5-18')
if (-not $isSystem -and -not $AsSystem) {
    $log = Join-Path $env:TEMP "rename-$([guid]::NewGuid()).log"
    $argList = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$PSCommandPath`"",
        '-Flow', $Flow, '-MatchInterface', "`"$MatchInterface`"", '-MatchDeviceDesc', "`"$MatchDeviceDesc`"",
        '-NewDeviceDesc', "`"$NewDeviceDesc`"", '-AsSystem')
    if ($PSBoundParameters.ContainsKey('NewInterface')) { $argList += @('-NewInterface', "`"$NewInterface`"") }
    $cmd = "& powershell.exe $($argList -join ' ') *> `"$log`""
    $enc = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($cmd))
    $name = "ClaudeTalkRename-$([guid]::NewGuid().ToString('N'))"
    $action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument "-NoProfile -EncodedCommand $enc"
    Register-ScheduledTask -TaskName $name -Action $action -User 'SYSTEM' -RunLevel Highest -Force | Out-Null
    Start-ScheduledTask -TaskName $name
    $deadline = (Get-Date).AddSeconds(60)
    do { Start-Sleep -Milliseconds 500; $st = (Get-ScheduledTask -TaskName $name).State } while ($st -eq 'Running' -and (Get-Date) -lt $deadline)
    $info = Get-ScheduledTaskInfo -TaskName $name
    Unregister-ScheduledTask -TaskName $name -Confirm:$false
    if (Test-Path $log) { Get-Content $log | Write-Host; Remove-Item $log }
    if ($info.LastTaskResult -ne 0) { throw "Rename as SYSTEM failed with exit code $($info.LastTaskResult)." }
} else {
    foreach ($t in $targets) {
        $props = Join-Path $t.PSPath 'Properties'
        Set-ItemProperty -Path $props -Name $PK_DESC -Value $NewDeviceDesc
        if ($PSBoundParameters.ContainsKey('NewInterface')) { Set-ItemProperty -Path $props -Name $PK_IFACE -Value $NewInterface }
        Write-Host "Renamed $Flow endpoint $($t.PSChildName) -> '$NewDeviceDesc' / '$NewInterface'"
    }
    if ($isSystem) { exit 0 }
}
Restart-Service -Name Audiosrv -Force
Start-Sleep -Seconds 3
