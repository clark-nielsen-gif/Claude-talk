# Renames audio endpoints (the name Discord/Chrome/Windows show).
# Method 1: Core Audio property store opened for write (what the Sound
#           control panel does; needs an elevated caller).
# Method 2: if that is refused, write the MMDevices registry properties as
#           SYSTEM through a one-shot scheduled task, then restart AudioSrv.
# Prints which method worked; both are recorded for DECISIONS.md.
param(
    [Parameter(Mandatory)] [ValidateSet('Capture', 'Render')] [string] $Flow,
    [Parameter(Mandatory)] [string] $MatchFriendlyName,   # regex on current PKEY_Device_FriendlyName
    [Parameter(Mandatory)] [string] $NewDeviceDesc,
    [string] $NewInterface,                              # optional: replaces the "(...)" suffix
    [switch] $RegistryOnly
)
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

public static class EndpointRename {
    [StructLayout(LayoutKind.Sequential)] public struct PROPERTYKEY { public Guid fmtid; public int pid; }
    [StructLayout(LayoutKind.Explicit, Size = 24)] public struct PROPVARIANT {
        [FieldOffset(0)] public ushort vt; [FieldOffset(8)] public IntPtr p;
    }
    [ComImport, Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IMMDeviceEnumerator { int EnumAudioEndpoints(int flow, int mask, out IMMDeviceCollection c); }
    [ComImport, Guid("0BD7A1BE-7A1A-44DB-8397-CC5392387B5E"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IMMDeviceCollection { int GetCount(out int n); int Item(int i, out IMMDevice d); }
    [ComImport, Guid("D666063F-1587-4E43-81F1-B948E807363F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IMMDevice {
        int Activate(ref Guid iid, int ctx, IntPtr p, [MarshalAs(UnmanagedType.IUnknown)] out object o);
        [PreserveSig] int OpenPropertyStore(int access, out IPropertyStore store);
        int GetId([MarshalAs(UnmanagedType.LPWStr)] out string id);
    }
    [ComImport, Guid("886d8eeb-8cf2-4446-8d02-cdba1dbdcf99"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IPropertyStore {
        int GetCount(out int n); int GetAt(int i, out PROPERTYKEY k);
        [PreserveSig] int GetValue(ref PROPERTYKEY k, out PROPVARIANT v);
        [PreserveSig] int SetValue(ref PROPERTYKEY k, ref PROPVARIANT v);
        [PreserveSig] int Commit();
    }
    [ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")] class MMDeviceEnumerator { }
    [DllImport("ole32.dll")] static extern int PropVariantClear(ref PROPVARIANT v);

    static PROPERTYKEY Key(string g, int pid) { return new PROPERTYKEY { fmtid = new Guid(g), pid = pid }; }
    static string Read(IPropertyStore s, PROPERTYKEY k) {
        PROPVARIANT v; if (s.GetValue(ref k, out v) != 0) return null;
        string r = v.vt == 31 ? Marshal.PtrToStringUni(v.p) : null; PropVariantClear(ref v); return r;
    }
    static int Write(IPropertyStore s, PROPERTYKEY k, string value) {
        var v = new PROPVARIANT { vt = 31, p = Marshal.StringToCoTaskMemUni(value) };
        int hr = s.SetValue(ref k, ref v);
        Marshal.FreeCoTaskMem(v.p);
        return hr;
    }

    // Returns one line per matched endpoint: "<id>|<old name>|<hresult of write+commit or 'readonly'>"
    public static List<string> Rename(int flow, string pattern, string desc, string iface) {
        var outp = new List<string>();
        var rx = new System.Text.RegularExpressions.Regex(pattern);
        var e = (IMMDeviceEnumerator)new MMDeviceEnumerator();
        IMMDeviceCollection c; Marshal.ThrowExceptionForHR(e.EnumAudioEndpoints(flow, 0xF, out c));
        int n; c.GetCount(out n);
        var kFriendly = Key("a45c254e-df1c-4efd-8020-67d146a850e0", 14);
        var kDesc = Key("a45c254e-df1c-4efd-8020-67d146a850e0", 2);
        var kIface = Key("b3f8fa53-0004-438e-9003-51a46e139bfc", 6);
        for (int i = 0; i < n; i++) {
            IMMDevice d; c.Item(i, out d);
            string id; d.GetId(out id);
            IPropertyStore r; if (d.OpenPropertyStore(0, out r) != 0) continue;
            string name = Read(r, kFriendly) ?? "";
            if (!rx.IsMatch(name)) continue;
            IPropertyStore w; int hr = d.OpenPropertyStore(2 /* STGM_READWRITE */, out w);
            if (hr != 0) { outp.Add(id + "|" + name + "|open:0x" + hr.ToString("X8")); continue; }
            hr = Write(w, kDesc, desc);
            if (hr == 0 && iface != null) hr = Write(w, kIface, iface);
            if (hr == 0) hr = w.Commit();
            outp.Add(id + "|" + name + "|0x" + hr.ToString("X8"));
        }
        return outp;
    }
}
'@

$flowId = if ($Flow -eq 'Render') { 0 } else { 1 }
$iface = if ($PSBoundParameters.ContainsKey('NewInterface')) { $NewInterface } else { $null }
$apiOk = $false
if (-not $RegistryOnly) {
    $lines = [EndpointRename]::Rename($flowId, $MatchFriendlyName, $NewDeviceDesc, $iface)
    if (-not $lines) { throw "No $Flow endpoint matches '$MatchFriendlyName'." }
    $lines | ForEach-Object { Write-Host "API rename: $_" }
    $apiOk = -not ($lines | Where-Object { $_ -notmatch '\|0x00000000$' })
}
if ($apiOk) { Write-Host "METHOD=api"; return 'api' }

# Method 2: registry as SYSTEM. Values are embedded in a generated script
# file, so no quoting goes through the command line.
$ids = @()
$root = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$Flow"
foreach ($k in Get-ChildItem $root) {
    $p = Get-ItemProperty (Join-Path $k.PSPath 'Properties') -ErrorAction SilentlyContinue
    $friendly = "$($p.'{a45c254e-df1c-4efd-8020-67d146a850e0},2') ($($p.'{b3f8fa53-0004-438e-9003-51a46e139bfc},6'))"
    if ($friendly -match $MatchFriendlyName) { $ids += $k.PSChildName }
}
if (-not $ids) { throw "No $Flow endpoint in the registry matches '$MatchFriendlyName'." }

$tmp = "$env:SystemRoot\Temp\ct-rename-$([guid]::NewGuid().ToString('N'))"
$enc = New-Object Text.UTF8Encoding $true
$body = @"
`$ErrorActionPreference = 'Stop'
Start-Transcript -Path '$tmp.log' | Out-Null
try {
$(($ids | ForEach-Object {
    $props = "$root\$_\Properties"
    "  Set-ItemProperty -LiteralPath '$props' -Name '{a45c254e-df1c-4efd-8020-67d146a850e0},2' -Value '$($NewDeviceDesc -replace "'", "''")'"
    if ($null -ne $iface) { "  Set-ItemProperty -LiteralPath '$props' -Name '{b3f8fa53-0004-438e-9003-51a46e139bfc},6' -Value '$($iface -replace "'", "''")'" }
}) -join "`r`n")
  'ok' | Set-Content '$tmp.done'
} catch { `$_ | Out-String | Set-Content '$tmp.err'; throw } finally { Stop-Transcript | Out-Null }
"@
[IO.File]::WriteAllText("$tmp.ps1", $body, $enc)
$name = "ClaudeTalkRename-$([guid]::NewGuid().ToString('N'))"
$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument "-NoProfile -ExecutionPolicy Bypass -File `"$tmp.ps1`""
Register-ScheduledTask -TaskName $name -Action $action -User 'SYSTEM' -RunLevel Highest -Force | Out-Null
Start-ScheduledTask -TaskName $name
$deadline = (Get-Date).AddSeconds(60)
while (-not (Test-Path "$tmp.done") -and -not (Test-Path "$tmp.err") -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 300 }
Unregister-ScheduledTask -TaskName $name -Confirm:$false
if (Test-Path "$tmp.log") { Get-Content "$tmp.log" | Write-Host }
$failed = -not (Test-Path "$tmp.done")
$err = if (Test-Path "$tmp.err") { Get-Content "$tmp.err" -Raw } else { 'timed out' }
Remove-Item "$tmp.*" -ErrorAction SilentlyContinue
if ($failed) { throw "Registry rename as SYSTEM failed: $err" }
Restart-Service -Name Audiosrv -Force
Start-Sleep -Seconds 3
Write-Host "METHOD=registry-system"
'registry-system'
