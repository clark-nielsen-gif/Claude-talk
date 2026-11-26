# Proves that driver files would load on a normal PC WITHOUT test signing,
# independently of the runner's own boot configuration:
#  1. signtool verify /kp  (Windows kernel-mode code signing policy) must pass
#  2. the chain must end in a Microsoft root (attestation/WHQL), or be a legacy
#     cross-signed chain whose signing certificate predates 2015-07-29, which
#     Windows still accepts.
param(
    [Parameter(Mandatory)] [string] $Path,
    [Parameter(Mandatory)] [string] $OutFile,
    [switch] $NoThrow
)
$ErrorActionPreference = 'Continue'
$signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Recurse -Filter signtool.exe |
    Where-Object FullName -match '\\x64\\' | Sort-Object FullName | Select-Object -Last 1
if (-not $signtool) { throw 'signtool.exe not found on runner.' }

$results = foreach ($f in Get-ChildItem $Path -Recurse -Include *.sys, *.cat) {
    $sig = Get-AuthenticodeSignature $f.FullName
    $catArgs = @()
    if ($f.Extension -eq '.sys' -and $sig.Status -eq 'NotSigned') {
        # Catalog-signed driver binary: verify it through the .cat next to it.
        $cat = Get-ChildItem $f.DirectoryName -Filter *.cat | Select-Object -First 1
        if ($cat) { $catArgs = @('/c', $cat.FullName); $sig = Get-AuthenticodeSignature $cat.FullName }
    }
    $out = (& $signtool.FullName verify /kp /v /all @catArgs $f.FullName 2>&1) -join "`n"
    $kpOk = ($LASTEXITCODE -eq 0)
    $chain = New-Object Security.Cryptography.X509Certificates.X509Chain
    $chainNames = @()
    if ($sig.SignerCertificate) {
        [void]$chain.Build($sig.SignerCertificate)
        $chainNames = $chain.ChainElements | ForEach-Object { $_.Certificate.Subject }
    }
    $microsoft = [bool]($sig.SignerCertificate.Subject -match 'Microsoft Windows Hardware Compatibility Publisher')
    $legacyCross = [bool]($out -match 'Cross Certificate Chain' -and $sig.SignerCertificate.NotBefore -lt [datetime]'2015-07-29')
    [pscustomobject]@{
        File              = $f.FullName.Substring((Resolve-Path $Path).Path.Length)
        KernelPolicyPass  = $kpOk
        Signer            = "$($sig.SignerCertificate.Subject)"
        SignerNotBefore   = $sig.SignerCertificate.NotBefore
        MicrosoftSigned   = $microsoft
        LegacyCrossSigned = $legacyCross
        Chain             = $chainNames
        ProductionOk      = ($kpOk -and ($microsoft -or $legacyCross))
        SigntoolTail      = ($out -split "`n" | Select-Object -Last 12) -join "`n"
    }
}
$results | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 $OutFile
$results | Format-Table File, KernelPolicyPass, MicrosoftSigned, LegacyCrossSigned, ProductionOk, Signer -AutoSize | Out-String -Width 300 | Write-Host
$bad = $results | Where-Object { -not $_.ProductionOk }
if ($bad -and -not $NoThrow) { throw "Not production-signed for kernel mode: $($bad.File -join ', ')" }
