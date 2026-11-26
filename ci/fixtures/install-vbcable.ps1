# CI fixture: installs VB-CABLE exactly as the product installer will
# (official download, signature check, silent install) and renames the
# virtual microphone. Prints the endpoints afterwards.
param([string] $ResultsDir = 'results')
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force $ResultsDir | Out-Null
# Windows Server images ship with the audio service disabled.
Set-Service Audiosrv -StartupType Automatic
Start-Service Audiosrv
# Server images deny microphone access to desktop apps (privacy setting);
# allow it as a normal Windows 11 PC does by default.
$cs = 'SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone'
foreach ($k in "HKLM:\$cs", "HKCU:\$cs", "HKCU:\$cs\NonPackaged") {
    New-Item -Path $k -Force | Out-Null
    Set-ItemProperty -Path $k -Name Value -Value 'Allow'
}

$ProgressPreference = 'SilentlyContinue'
$zip = Join-Path $env:RUNNER_TEMP 'vbcable.zip'
Invoke-WebRequest https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack46.zip -OutFile $zip
$dir = Join-Path $env:RUNNER_TEMP 'vbcable'
Expand-Archive $zip -DestinationPath $dir -Force
$cat = Get-AuthenticodeSignature (Join-Path $dir 'vbaudio_cable64_win10.cat')
if ($cat.Status -ne 'Valid' -or $cat.SignerCertificate.Subject -notmatch 'Microsoft Windows Hardware Compatibility Publisher') {
    throw "VB-CABLE package is not Microsoft-signed: $($cat.SignerCertificate.Subject)"
}
$p = Start-Process (Join-Path $dir 'VBCABLE_Setup_x64.exe') -ArgumentList '-i', '-h' -Wait -PassThru
"VB-CABLE setup exit=$($p.ExitCode)"
Start-Sleep 3
$d = [char]0x2013
& "$PSScriptRoot\..\driver-proof\Rename-Endpoint.ps1" -Flow Capture -MatchFriendlyName 'CABLE Output' -NewDeviceDesc "Claude Talk $d til Discord" -NewInterface 'VB-Audio' | Out-Null
