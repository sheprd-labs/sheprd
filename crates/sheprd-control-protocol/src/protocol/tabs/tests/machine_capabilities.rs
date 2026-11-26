use super::*;

#[test]
fn older_machine_info_defaults_to_macos_arm64_capabilities() {
    let info: MachineInfo = serde_json::from_str(
        r#"{"name":"work","state":"stopped","cpus":4,"memory_gib":8,"display":"fit","disk_bytes":1024}"#,
    )
    .unwrap();
    assert_eq!(info.os, GuestOs::Macos);
    assert_eq!(info.arch, GuestArch::Arm64);
    assert_eq!(info.image, None);
    assert_eq!(info.capabilities, MachineCapabilities::macos());
}

#[test]
fn older_machine_image_info_defaults_to_macos_arm64() {
    let image: MachineImageInfo =
        serde_json::from_str(r#"{"name":"base","disk_bytes":4096}"#).unwrap();
    assert_eq!(image.os, GuestOs::Macos);
    assert_eq!(image.arch, GuestArch::Arm64);
}

#[test]
fn linux_machine_info_and_image_round_trip() {
    let info = MachineInfo {
        reservation: None,
        name: "linux-dev".into(),
        state: MachineState::Running,
        os: GuestOs::Linux,
        arch: GuestArch::Arm64,
        image: Some("ubuntu-24.04-wayland".into()),
        capabilities: MachineCapabilities::linux_apple(),
        cpus: 4,
        memory_gib: 4,
        display: "1280x800".into(),
        disk_bytes: 2_500_000_000,
    };
    let wire = serde_json::to_string(&info).unwrap();
    assert_eq!(serde_json::from_str::<MachineInfo>(&wire).unwrap(), info);
    assert!(!info.capabilities.save_restore);
    assert!(info.capabilities.graceful_shutdown);
    assert!(!info.capabilities.dynamic_display);

    let image = MachineImageInfo {
        name: "ubuntu-wayland".into(),
        os: GuestOs::Linux,
        arch: GuestArch::Arm64,
        disk_bytes: 2_000_000_000,
    };
    let wire = serde_json::to_string(&image).unwrap();
    assert_eq!(
        serde_json::from_str::<MachineImageInfo>(&wire).unwrap(),
        image
    );
}

#[test]
fn agent_info_falls_back_to_macos_permissions_when_capabilities_absent() {
    let legacy: MachineAgentInfo = serde_json::from_str(
        r#"{"version":1,"width":2560,"height":1600,"scale":2,"accessibility":true,"screen_recording":false}"#,
    )
    .unwrap();
    assert_eq!(legacy.capabilities, None);
    assert_eq!(
        legacy.effective_capabilities(),
        AgentCapabilities {
            exec: true,
            screenshot: false,
            pointer: true,
            keyboard: true,
            text: true,
            desktop_ready: true,
        }
    );

    let explicit = AgentCapabilities {
        exec: true,
        screenshot: true,
        pointer: true,
        keyboard: true,
        text: true,
        desktop_ready: false,
    };
    let linux = MachineAgentInfo {
        version: 1,
        width: 1280,
        height: 800,
        scale: 1,
        accessibility: false,
        screen_recording: false,
        capabilities: Some(explicit),
    };
    let wire = serde_json::to_string(&linux).unwrap();
    let decoded: MachineAgentInfo = serde_json::from_str(&wire).unwrap();
    assert_eq!(decoded.effective_capabilities(), explicit);
}

#[test]
fn guest_os_and_arch_names_and_parsers_match() {
    for os in [GuestOs::Macos, GuestOs::Linux] {
        assert_eq!(GuestOs::parse(os.name()), Some(os));
        assert_eq!(
            serde_json::to_string(&os).unwrap(),
            format!("\"{}\"", os.name())
        );
    }
    assert_eq!(GuestOs::parse("mac"), Some(GuestOs::Macos));
    assert_eq!(GuestOs::parse("dos"), None);

    for arch in [GuestArch::Arm64, GuestArch::X86_64] {
        assert_eq!(GuestArch::parse(arch.name()), Some(arch));
        assert_eq!(
            serde_json::to_string(&arch).unwrap(),
            format!("\"{}\"", arch.name())
        );
    }
    assert_eq!(GuestArch::parse("aarch64"), Some(GuestArch::Arm64));
    assert_eq!(GuestArch::parse("amd64"), Some(GuestArch::X86_64));
    assert_eq!(GuestArch::parse("mips"), None);
}
