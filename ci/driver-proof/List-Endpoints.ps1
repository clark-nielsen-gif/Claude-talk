# Lists active audio endpoints through the Core Audio API (IMMDeviceEnumerator)
# with PKEY_Device_FriendlyName: exactly the name Discord and Chrome display.
param([string] $OutFile)
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

public static class CoreAudioList {
    [StructLayout(LayoutKind.Sequential)] public struct PROPERTYKEY { public Guid fmtid; public int pid; }
    [StructLayout(LayoutKind.Explicit)] public struct PROPVARIANT {
        [FieldOffset(0)] public ushort vt; [FieldOffset(8)] public IntPtr p; [FieldOffset(16)] public IntPtr pad;
    }
    [ComImport, Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IMMDeviceEnumerator {
        int EnumAudioEndpoints(int dataFlow, int stateMask, out IMMDeviceCollection devices);
        int GetDefaultAudioEndpoint(int dataFlow, int role, out IMMDevice device);
    }
    [ComImport, Guid("0BD7A1BE-7A1A-44DB-8397-CC5392387B5E"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IMMDeviceCollection { int GetCount(out int n); int Item(int i, out IMMDevice d); }
    [ComImport, Guid("D666063F-1587-4E43-81F1-B948E807363F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IMMDevice {
        int Activate(ref Guid iid, int ctx, IntPtr p, [MarshalAs(UnmanagedType.IUnknown)] out object o);
        int OpenPropertyStore(int access, out IPropertyStore store);
        int GetId([MarshalAs(UnmanagedType.LPWStr)] out string id);
        int GetState(out int state);
    }
    [ComImport, Guid("886d8eeb-8cf2-4446-8d02-cdba1dbdcf99"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IPropertyStore {
        int GetCount(out int n); int GetAt(int i, out PROPERTYKEY k);
        int GetValue(ref PROPERTYKEY k, out PROPVARIANT v);
    }
    [ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")] class MMDeviceEnumerator { }
    [DllImport("ole32.dll")] static extern int PropVariantClear(ref PROPVARIANT v);

    public static List<string[]> List() {
        var result = new List<string[]>();
        var e = (IMMDeviceEnumerator)new MMDeviceEnumerator();
        foreach (int flow in new[] { 0, 1 }) {
            IMMDeviceCollection c; Marshal.ThrowExceptionForHR(e.EnumAudioEndpoints(flow, 1, out c));
            int n; c.GetCount(out n);
            for (int i = 0; i < n; i++) {
                IMMDevice d; c.Item(i, out d);
                string id; d.GetId(out id);
                IPropertyStore s; d.OpenPropertyStore(0, out s);
                var key = new PROPERTYKEY { fmtid = new Guid("a45c254e-df1c-4efd-8020-67d146a850e0"), pid = 14 };
                PROPVARIANT v; s.GetValue(ref key, out v);
                string name = v.vt == 31 ? Marshal.PtrToStringUni(v.p) : "";
                PropVariantClear(ref v);
                result.Add(new[] { flow == 0 ? "Render" : "Capture", name, id });
            }
        }
        return result;
    }
}
'@

$rows = [CoreAudioList]::List() | ForEach-Object { [pscustomobject]@{ Flow = $_[0]; FriendlyName = $_[1]; Id = $_[2] } }
$rows | Format-Table -AutoSize | Out-String -Width 300 | Write-Host
if ($OutFile) { $rows | ConvertTo-Json | Set-Content -Encoding utf8 $OutFile }
$rows
