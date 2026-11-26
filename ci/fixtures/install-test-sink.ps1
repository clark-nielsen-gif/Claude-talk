# CI-ONLY fixture: a second virtual sound card that the fake programs
# ("Claude", "Discord", microphone, background) play into, so their sound
# never reaches VB-CABLE except through the engine. Uses the VirtualDrivers
# test driver, which loads only because hosted runners have test signing on.
# It is never part of the product (see DECISIONS.md D1).
$ErrorActionPreference = 'Stop'
$dir = Join-Path $env:RUNNER_TEMP 'vad'
gh release download 25.7.14 -R VirtualDrivers/Virtual-Audio-Driver -D $dir --clobber
Get-ChildItem $dir -Filter *.zip | ForEach-Object { Expand-Archive $_.FullName -DestinationPath "$dir\x" -Force }
$inf = (Get-ChildItem $dir -Recurse -Filter VirtualAudioDriver.inf | Select-Object -First 1).FullName
$cat = Get-ChildItem (Split-Path $inf) -Filter *.cat | Select-Object -First 1
# trust the test driver's publisher on this disposable runner only, so the
# install does not stop at a "Windows Security" prompt
$cert = (Get-AuthenticodeSignature $cat.FullName).SignerCertificate
$store = New-Object Security.Cryptography.X509Certificates.X509Store 'TrustedPublisher', 'LocalMachine'
$store.Open('ReadWrite'); $store.Add($cert); $store.Close()
$job = Start-Job -ScriptBlock {
    param($root, $inf)
    & "$root\ci\driver-proof\RootDevice.ps1" -InfPath $inf -HardwareId 'ROOT\VirtualAudioDriver'
} -ArgumentList (Resolve-Path "$PSScriptRoot\..\..").Path, $inf
if (-not (Wait-Job $job -Timeout 180)) { throw 'Test sink install timed out (driver prompt?)' }
Receive-Job $job
Start-Sleep 3
