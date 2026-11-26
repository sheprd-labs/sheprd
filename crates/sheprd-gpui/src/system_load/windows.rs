//! What a Windows SSH host prints for its load, through
//! [`crate::windows_host`], in the lines [`super::sample::parse`] reads.

use super::INTERVAL;

/// Once: whether to probe the GPU, the volume holding the user's profile, and
/// `nvidia-smi` if installed.
// i18n: data
const SETUP: &str = r#"$herdrGpu = __GPU__
$herdrRoot = [System.IO.Path]::GetPathRoot($env:USERPROFILE)
$herdrNvidia = Get-Command nvidia-smi.exe -CommandType Application | Select-Object -First 1"#;

/// Each answer: `cores`; `cpu-idle`, the `_Total` processor's idle time and
/// timestamp, compared with the previous answer's as Linux ticks are; memory
/// in KiB, total and available; uptime in seconds; the profile's volume in
/// bytes, total and available to the user; and, when `$herdrGpu` is set, a
/// `gpu-nvidia` line per NVIDIA card. The raw counter classes are named the
/// same in every Windows language, unlike `Get-Counter` paths.
// i18n: data
const BODY: &str = r#"    $herdrOs = Get-CimInstance Win32_OperatingSystem
    $herdrCpu = Get-CimInstance Win32_PerfRawData_PerfOS_Processor -Filter "Name='_Total'"
    [void]$herdrOut.AppendLine('cores ' + [Environment]::ProcessorCount)
    if ($null -ne $herdrCpu) { [void]$herdrOut.AppendLine('cpu-idle ' + $herdrCpu.PercentProcessorTime + ' ' + $herdrCpu.Timestamp_Sys100NS) }
    if ($null -ne $herdrOs) {
        [void]$herdrOut.AppendLine('memory-kb ' + $herdrOs.TotalVisibleMemorySize + ' ' + $herdrOs.FreePhysicalMemory)
        [void]$herdrOut.AppendLine('uptime ' + [long]((Get-Date) - $herdrOs.LastBootUpTime).TotalSeconds)
    }
    try { $herdrDisk = New-Object System.IO.DriveInfo $herdrRoot; [void]$herdrOut.AppendLine('disk-bytes ' + $herdrDisk.TotalSize + ' ' + $herdrDisk.AvailableFreeSpace) } catch {}
    if ($herdrGpu -and $null -ne $herdrNvidia) {
        & $herdrNvidia.Source --query-gpu=utilization.gpu,memory.used,memory.total --format=csv,noheader,nounits 2>$null | ForEach-Object { [void]$herdrOut.AppendLine('gpu-nvidia ' + $_) }
    }"#;

/// The load script, with the GPU probed or not; answers come every
/// [`INTERVAL`], as the worker samples.
pub(super) fn script(gpu: bool) -> String {
    let setup = SETUP.replace("__GPU__", if gpu { "$true" } else { "$false" });
    crate::windows_host::script(&setup, BODY, INTERVAL)
}
