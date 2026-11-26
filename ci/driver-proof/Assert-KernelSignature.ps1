# Proves that the driver packages Windows actually installed would load on a
# normal PC WITHOUT test signing, independently of the runner's boot config.
# For each driver package (INF + CAT + SYS) found under -Path:
#  1. signtool verify /kp of every .sys through the package catalog
#     (the kernel-mode code signing policy check) must pass, and
#  2. the catalog must be signed by Microsoft (attestation or WHQL:
#     "Microsoft Windows Hardware Compatibility Publisher"), or carry a
#     legacy cross-signed chain from a certificate issued before 2015-07-29,
#     which Windows 10/11 still accept with Secure Boot on.
param(
    [Parameter(Mandatory)] [string[]] $Path,
    [Parameter(Mandatory)] [string] $OutFile,
    [string] $Filter = '*',
    [switch] $NoThrow
)
$ErrorActionPreference = 'Continue'
$signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Recurse -Filter signtool.exe |
    Where-Object FullName -match '\\x64\\' | Sort-Object FullName | Select-Object -Last 1
if (-not $signtool) { throw 'signtool.exe not found on runner.' }

$results = @()
foreach ($root in $Path) {
    foreach ($cat in Get-ChildItem $root -Recurse -Filter "$Filter.cat" -ErrorAction SilentlyContinue) {
        $catSig = Get-AuthenticodeSignature $cat.FullName
        $signer = $catSig.SignerCertificate
        $microsoft = [bool]("$($signer.Subject)" -match 'Microsoft Windows Hardware Compatibility Publisher')
        $catOut = (& $signtool.FullName verify /kp /v $cat.FullName 2>&1) -join "`n"
        $legacyCross = [bool]($catOut -match 'Cross Certificate Chain' -and $signer -and $signer.NotBefore -lt [datetime]'2015-07-29')
        $syss = Get-ChildItem $cat.DirectoryName -Filter *.sys
        foreach ($sys in $syss) {
            $out = (& $signtool.FullName verify /kp /v /c $cat.FullName $sys.FullName 2>&1) -join "`n"
            $kpOk = ($LASTEXITCODE -eq 0)
            $results += [pscustomobject]@{
                Catalog           = $cat.FullName
                Driver            = $sys.Name
                CatalogSigner     = "$($signer.Subject)"
                MicrosoftSigned   = $microsoft
                LegacyCrossSigned = $legacyCross
                KernelPolicyPass  = $kpOk
                ProductionOk      = ($kpOk -and ($microsoft -or $legacyCross))
                SigntoolTail      = ($out -split "`n" | Where-Object { $_.Trim() } | Select-Object -Last 14) -join "`n"
            }
        }
    }
}
if (-not $results) { throw "No driver catalogs matching '$Filter.cat' under $($Path -join ', ')." }
$results | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 $OutFile
foreach ($r in $results) {
    Write-Host ("[{0}] {1} via {2}" -f ($(if ($r.ProductionOk) { 'PRODUCTION OK' } else { 'NOT PRODUCTION' })), $r.Driver, (Split-Path $r.Catalog -Leaf))
    Write-Host ("    catalog signer: {0} | Microsoft={1} LegacyCross={2} /kp={3}" -f $r.CatalogSigner, $r.MicrosoftSigned, $r.LegacyCrossSigned, $r.KernelPolicyPass)
    $r.SigntoolTail -split "`n" | ForEach-Object { Write-Host "    | $_" }
}
$bad = $results | Where-Object { -not $_.ProductionOk }
if ($bad -and -not $NoThrow) { throw "Not production-signed for kernel mode: $(($bad | ForEach-Object Driver) -join ', ')" }
