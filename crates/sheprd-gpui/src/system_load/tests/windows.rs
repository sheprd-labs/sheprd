use super::*;
use crate::system_load::{
    sample::Disk,
    windows::{END, SCRIPT},
};

/// Two answers as the PowerShell sampler prints them, with Windows line
/// endings, 1000 units of 100 ns apart in all.
const WINDOWS: &str = "cpu 600 0 200 1200\r
MemTotal: 33370984 kB\r
MemAvailable: 25000772 kB\r
cores 8\r
uptime 152517\r
disk C:\\ 998797308 951397972 47399336 96% C:\\\r
";

const WINDOWS_LATER: &str = "cpu 900 0 300 1800\r
MemTotal: 33370984 kB\r
MemAvailable: 24998012 kB\r
cores 8\r
uptime 152519\r
disk C:\\ 998797308 951397972 47399336 96% C:\\\r
";

#[test]
fn a_windows_answer_reads_as_a_linux_one() {
    let first = parse(WINDOWS, None).unwrap();
    assert_eq!(first.sample.cpu, None, "one answer has nothing to compare");
    assert_eq!(first.sample.cores, Some(8));
    assert_eq!(first.sample.load, None, "Windows keeps no load average");
    assert_eq!(
        first.sample.memory,
        Some(Memory {
            used: (33_370_984 - 25_000_772) * 1024,
            total: 33_370_984 * 1024,
        })
    );
    assert_eq!(first.sample.uptime, Some(152_517));
    assert_eq!(
        first.sample.disk,
        Some(Disk {
            available: 47_399_336 * 1024,
            total: 998_797_308 * 1024,
        })
    );
    let second = parse(WINDOWS_LATER, first.ticks).unwrap();
    // 300 user and 100 kernel time busy, of 1000 in all.
    let cpu = second.sample.cpu.unwrap();
    assert!((cpu - 40.).abs() < 0.01, "{cpu}");
}

#[test]
fn the_script_ends_each_answer_with_the_marker() {
    assert!(SCRIPT.contains(&format!("'{END}'")));
}

/// A local `sh` stands in for SSH and PowerShell.
#[cfg(unix)]
fn fake(script: &str) -> crate::system_load::windows::Sampler {
    let mut command = std::process::Command::new("/bin/sh");
    command
        .args(["-c", script])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    crate::system_load::windows::Sampler::start(command, None).unwrap()
}

#[cfg(unix)]
#[test]
fn each_answer_is_returned_without_its_end_line() {
    let mut sampler = fake(&format!(
        "n=0; while read _; do n=$((n + 1)); printf 'cores %s\\r\\n{END}\\r\\n' \"$n\"; done"
    ));
    let timeout = Duration::from_secs(5);
    assert_eq!(sampler.sample(timeout).unwrap(), "cores 1\r\n");
    assert_eq!(sampler.sample(timeout).unwrap(), "cores 2\r\n");
}

#[cfg(unix)]
#[test]
fn the_marker_ends_an_answer_only_on_a_line_of_its_own() {
    let mut sampler = fake(&format!(
        "read _; printf 'x{END}\\ncores 1\\n'; printf '{END}\\n'; read _"
    ));
    let answer = sampler.sample(Duration::from_secs(5)).unwrap();
    assert_eq!(answer, format!("x{END}\ncores 1\n"));
}

#[cfg(unix)]
#[test]
fn a_host_that_exits_is_unreachable() {
    let mut sampler = fake("exit 0");
    let error = sampler.sample(Duration::from_secs(5)).unwrap_err();
    assert!(
        matches!(error, crate::Error::Usage(sheprd_usage::Error::Unreachable)),
        "{error:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_host_that_does_not_answer_times_out() {
    let mut sampler = fake("while read _; do :; done");
    let error = sampler.sample(Duration::from_millis(200)).unwrap_err();
    assert!(
        matches!(error, crate::Error::Usage(sheprd_usage::Error::Timeout)),
        "{error:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_windows_source_measures_cpu_between_answers() {
    // `%s` prints each answer as it is, backslashes included.
    let sampler = fake(&format!(
        "read _; printf '%s{END}\\n' '{WINDOWS}'; read _; printf '%s{END}\\n' '{WINDOWS_LATER}'; read _"
    ));
    let mut source = Source::Windows {
        sampler,
        ticks: None,
    };
    assert_eq!(source.sample(false).unwrap().cpu, None);
    let cpu = source.sample(false).unwrap().cpu.unwrap();
    assert!((cpu - 40.).abs() < 0.01, "{cpu}");
}
