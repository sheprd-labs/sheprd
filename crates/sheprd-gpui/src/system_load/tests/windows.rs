//! A Windows SSH host: its PowerShell answer, how answers are split out of
//! the stream, and, on Windows, the script run by a real PowerShell.
use super::*;
use crate::system_load::{sample::Disk, windows::script};

/// Two answers as [`SCRIPT`] prints them, two seconds apart, with CRLF line
/// endings: 24 cores, 64 GiB of memory, one NVIDIA card.
const FIRST: &str = "cores 24\r\ncpu-idle 47379108072 134398531568079922\r\n\
memory-kb 67108864 50331648\r\nuptime 5613\r\n\
disk-bytes 1999269523456 936740401152\r\ngpu-nvidia 4, 6347, 16303\r\n";
const SECOND: &str = "cores 24\r\ncpu-idle 47394108072 134398531588079922\r\n\
memory-kb 67108864 33554432\r\nuptime 5615\r\n\
disk-bytes 1999269523456 936740401152\r\n";

#[test]
fn a_windows_answer_reads_like_any_host() {
    let first = parse(FIRST, None).unwrap();
    assert_eq!(
        first.sample.cpu, None,
        "the first answer has nothing to compare"
    );
    assert_eq!(first.sample.cores, Some(24));
    assert_eq!(
        first.sample.memory,
        Some(Memory {
            used: 16 << 30,
            total: 64 << 30,
        })
    );
    assert_eq!(first.sample.uptime, Some(5613));
    assert_eq!(first.sample.load, None, "Windows keeps no load average");
    assert_eq!(
        first.sample.disk,
        Some(Disk {
            available: 936_740_401_152,
            total: 1_999_269_523_456,
        })
    );
    let gpu = first.sample.gpu.unwrap();
    assert_eq!((gpu.count, gpu.busy), (1, Some(4.)));

    // 1.5 s idle of 2 s elapsed, averaged over the cores: 25% busy.
    let second = parse(SECOND, first.ticks).unwrap();
    assert_eq!(second.sample.cpu, Some(25.));
    assert_eq!(second.sample.memory.unwrap().used, 32 << 30);
    assert_eq!(second.sample.gpu, None, "no card line, no GPU");
}

#[test]
fn malformed_windows_lines_are_left_out() {
    let answer = parse(
        "cores 4\ncpu-idle 12 x\nmemory-kb 100\ndisk-bytes 0 0\nmemory-kb 100 400\n",
        None,
    )
    .unwrap();
    assert_eq!(answer.ticks, None);
    assert_eq!(answer.sample.disk, None);
    // More available than installed is read as none in use.
    assert_eq!(
        answer.sample.memory,
        Some(Memory {
            used: 0,
            total: 100 * 1024,
        })
    );
    assert!(matches!(
        parse("cores 4\r\n", None),
        Err(crate::Error::SystemLoad(Error::Output))
    ));
}

#[test]
fn the_script_carries_the_gpu_choice_and_the_interval() {
    for (gpu, line) in [
        (true, "$herdrGpu = $true\n"),
        (false, "$herdrGpu = $false\n"),
    ] {
        let script = script(gpu);
        assert!(script.contains(line), "{gpu}");
        assert!(script.contains("cpu-idle"));
        assert!(script.contains("Start-Sleep -Seconds 2\n"));
        assert!(!script.contains("__"), "every placeholder is filled");
    }
}

/// The load script run by this machine's own PowerShell: answers keep coming
/// without being asked for.
#[cfg(windows)]
#[test]
fn windows_powershell_reports_load_every_interval() {
    let mut session = crate::windows_host::Session::local(&script(true)).unwrap();
    let limit = Duration::from_secs(60);
    let first = parse(&session.sample(limit).unwrap(), None).unwrap();
    assert!(first.ticks.is_some());
    assert!(first.sample.memory.is_some_and(|memory| memory.total > 0));
    assert!(first.sample.cores.is_some_and(|cores| cores > 0));
    assert!(first.sample.disk.is_some());
    assert!(first.sample.uptime.is_some());
    let second = parse(&session.sample(limit).unwrap(), first.ticks).unwrap();
    assert!(
        second
            .sample
            .cpu
            .is_some_and(|cpu| (0.0..=100.).contains(&cpu))
    );
}

/// A real Windows SSH host, named by `SHEPRD_TEST_WINDOWS_SSH`, opened as the
/// worker opens any SSH host: its POSIX shell does not come up, and its
/// PowerShell answers instead, also after the GPU is turned on.
#[test]
#[ignore = "needs a Windows SSH host named in SHEPRD_TEST_WINDOWS_SSH"]
fn a_windows_ssh_host_is_sampled_through_powershell() {
    let target = std::env::var("SHEPRD_TEST_WINDOWS_SSH").expect("SHEPRD_TEST_WINDOWS_SSH");
    let started = Instant::now();
    let mut source = Source::open(&Host::Ssh(target), false, false).unwrap();
    eprintln!("opened in {:?}", started.elapsed());
    assert!(matches!(source, Source::Windows { .. }));
    for gpu in [false, false, true, true] {
        let started = Instant::now();
        let sample = source.sample(gpu).unwrap();
        eprintln!("gpu {gpu}: sampled in {:?}: {sample:?}", started.elapsed());
        assert!(sample.cpu.is_some_and(|cpu| (0.0..=100.).contains(&cpu)));
        assert!(
            sample
                .memory
                .is_some_and(|memory| memory.used <= memory.total)
        );
        assert!(sample.cores.is_some() && sample.disk.is_some() && sample.uptime.is_some());
        if !gpu {
            assert_eq!(sample.gpu, None, "no GPU probe without the setting");
        }
    }
}

/// Both GPU choices fit the command line `cmd.exe`, Windows OpenSSH's
/// default login shell, accepts.
#[test]
fn the_load_script_fits_the_command_line() {
    for gpu in [false, true] {
        let length = crate::windows_host::tests::remote_command_length(&script(gpu));
        assert!(length < crate::windows_host::COMMAND_LIMIT, "{length}");
    }
}
