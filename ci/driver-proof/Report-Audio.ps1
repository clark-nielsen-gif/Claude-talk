# Writes a machine-readable report of the audio state of the runner:
# signing policy, Secure Boot, every MEDIA/AudioEndpoint device with its
# problem code, and every capture/render endpoint with the name apps see.
param([Parameter(Mandatory)] [string] $OutFile, [string] $Label = '')
$ErrorActionPreference = 'Continue'

$secureBoot = try { Confirm-SecureBootUEFI } catch { "unavailable: $($_.Exception.Message)" }
$bcd = (bcdedit /enum '{current}' 2>&1) -join "`n"
$testSigning = [bool]($bcd -match 'testsigning\s+Yes')

$devices = Get-PnpDevice -Class MEDIA, AudioEndpoint -ErrorAction SilentlyContinue | ForEach-Object {
    $code = (Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName 'DEVPKEY_Device_ProblemCode' -ErrorAction SilentlyContinue).Data
    [pscustomobject]@{ Class = $_.Class; Name = $_.FriendlyName; Status = "$($_.Status)"; ProblemCode = $code; InstanceId = $_.InstanceId }
}

# Endpoint names exactly as WASAPI clients (Discord, Chrome) read them.
$endpoints = @()
foreach ($flow in 'Capture', 'Render') {
    $root = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$flow"
    if (-not (Test-Path $root)) { continue }
    Get-ChildItem $root | ForEach-Object {
        $state = (Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue).DeviceState
        $p = Get-ItemProperty (Join-Path $_.PSPath 'Properties') -ErrorAction SilentlyContinue
        $endpoints += [pscustomobject]@{
            Flow       = $flow
            Id         = $_.PSChildName
            State      = $state   # 1 = active
            DeviceDesc = $p.'{a45c254e-df1c-4efd-8020-67d146a850e0},2'
            Interface  = $p.'{b3f8fa53-0004-438e-9003-51a46e139bfc},6'
        }
    }
}

$report = [pscustomobject]@{
    Label       = $Label
    Os          = (Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, BuildNumber)
    SecureBoot  = $secureBoot
    TestSigning = $testSigning
    Devices     = $devices
    Endpoints   = $endpoints
}
$report | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 $OutFile
$endpoints | Format-Table -AutoSize | Out-String -Width 300 | Write-Host
$devices | Format-Table -AutoSize | Out-String -Width 300 | Write-Host
if ($testSigning) { throw 'Test signing is ON. That is forbidden for this project.' }
