//! What a Windows host prints for its listening ports, through
//! [`crate::windows_host`], in the lines [`super::parse`] reads from a POSIX
//! host's [`super::scan::COMMAND`].
//!
//! Windows has neither `ss` nor `lsof`: `Get-NetTCPConnection` lists the
//! listeners. A listener's workspace comes, as on other hosts, from its own
//! environment, which Windows lets a script read only through the process's
//! memory: its parameter block names the environment block and its size.
//! That needs the process opened for reading, which Windows allows for this
//! user's own processes and refuses for most others', whose listeners then
//! name no workspace and are dropped, as on other hosts.

use super::INTERVAL;

/// Once: the reader of another process's environment, compiled from C#. It
/// reads only 64-bit processes from a 64-bit PowerShell, whose parameter
/// block keeps the environment's address at 0x80 and its size at 0x3F0, and
/// reads at most 1 MiB.
// i18n: data
const SETUP: &str = r#"Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices; using System.Text;
public static class HerdrEnvironment {
    [DllImport("ntdll.dll")] static extern int NtQueryInformationProcess(IntPtr process, int kind, ref Basic info, int size, out int written);
    [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(int access, bool inherit, int id);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll")] static extern bool ReadProcessMemory(IntPtr process, IntPtr address, byte[] buffer, IntPtr size, out IntPtr read);
    [DllImport("kernel32.dll")] static extern bool IsWow64Process(IntPtr process, out bool wow);
    [StructLayout(LayoutKind.Sequential)] struct Basic { public IntPtr Exit; public IntPtr Peb; public IntPtr Affinity; public IntPtr Priority; public IntPtr Id; public IntPtr Parent; }
    static long Word(IntPtr process, long address) { var bytes = new byte[8]; IntPtr read; return ReadProcessMemory(process, (IntPtr)address, bytes, (IntPtr)8, out read) ? BitConverter.ToInt64(bytes, 0) : 0; }
    public static string Of(int id) {
        if (IntPtr.Size != 8) return null;
        IntPtr process = OpenProcess(0x1010, false, id);
        if (process == IntPtr.Zero) return null;
        try {
            bool wow; if (!IsWow64Process(process, out wow) || wow) return null;
            var info = new Basic(); int written;
            if (NtQueryInformationProcess(process, 0, ref info, Marshal.SizeOf(info), out written) != 0) return null;
            long parameters = Word(process, (long)info.Peb + 0x20); if (parameters == 0) return null;
            long block = Word(process, parameters + 0x80);
            long size = Word(process, parameters + 0x3F0);
            if (block == 0 || size <= 0 || size > 0x100000) return null;
            var bytes = new byte[size]; IntPtr read;
            if (!ReadProcessMemory(process, (IntPtr)block, bytes, (IntPtr)size, out read)) return null;
            return Encoding.Unicode.GetString(bytes, 0, (int)read);
        } finally { CloseHandle(process); }
    }
}
"@"#;

/// Each answer: `L <pid> <address:port> <process>` per listening socket (at
/// most 1024), IPv6 addresses bracketed; then, for each of the first 128
/// listening processes whose environment names a Herdr workspace or a
/// herdr-core terminal, its `E` or `C` line as [`super::scan::COMMAND`]
/// prints them, and a `W` line with its command line as Windows quotes it,
/// which the parser splits by Windows' rules rather than at every space.
// i18n: data
const BODY: &str = r#"    $herdrNames = @{}
    foreach ($herdrProcess in Get-Process) { $herdrNames[$herdrProcess.Id] = $herdrProcess.ProcessName }
    $herdrRows = @(Get-NetTCPConnection -State Listen | Select-Object -First 1024)
    foreach ($herdrRow in $herdrRows) {
        $herdrAddress = [string]$herdrRow.LocalAddress
        if ($herdrAddress.Contains(':')) { $herdrAddress = '[' + $herdrAddress + ']' }
        [void]$herdrOut.AppendLine('L ' + $herdrRow.OwningProcess + ' ' + $herdrAddress + ':' + $herdrRow.LocalPort + ' ' + $herdrNames[[int]$herdrRow.OwningProcess])
    }
    foreach ($herdrPid in @($herdrRows | ForEach-Object { [int]$_.OwningProcess } | Sort-Object -Unique | Select-Object -First 128)) {
        $herdrText = [HerdrEnvironment]::Of($herdrPid)
        if ($null -eq $herdrText) { continue }
        $herdrVariables = @{}
        foreach ($herdrLine in $herdrText.Split([char]0)) {
            $herdrAt = $herdrLine.IndexOf('=')
            if ($herdrAt -gt 0) {
                $herdrKey = $herdrLine.Substring(0, $herdrAt)
                if (-not $herdrVariables.ContainsKey($herdrKey)) { $herdrVariables[$herdrKey] = $herdrLine.Substring($herdrAt + 1) }
            }
        }
        if ($herdrVariables['HERDR_WORKSPACE_ID']) {
            [void]$herdrOut.AppendLine('E ' + $herdrPid + ' ' + $herdrVariables['HERDR_WORKSPACE_ID'] + ' ' + $herdrVariables['HERDR_SOCKET_PATH'])
        } elseif ($herdrVariables['HC_TERMINAL_ID']) {
            [void]$herdrOut.AppendLine('C ' + $herdrPid + ' ' + $herdrVariables['HC_TERMINAL_ID'] + ' ' + $herdrVariables['HC_TERMINAL_SOCKET'])
        } else { continue }
        $herdrArgs = [string](Get-CimInstance Win32_Process -Filter "ProcessId=$herdrPid").CommandLine
        $herdrArgs = $herdrArgs -replace '[\r\n]', ' '
        if ($herdrArgs.Length -gt 1024) { $herdrArgs = $herdrArgs.Substring(0, 1024) }
        [void]$herdrOut.AppendLine('W ' + $herdrPid + ' ' + $herdrArgs)
    }"#;

/// The scan script, answering every [`INTERVAL`], as the worker scans.
pub(super) fn script() -> String {
    crate::windows_host::script(SETUP, BODY, INTERVAL)
}
