#![allow(clippy::unwrap_used)]
use super::*;

fn name(text: &str) -> Name {
    Name::try_from(text.to_owned()).unwrap()
}

fn read_directory_entries(iso: &[u8], lba: u32) -> Vec<(Vec<u8>, u32, usize)> {
    let sector = &iso[lba as usize * SECTOR_SIZE..(lba as usize + 1) * SECTOR_SIZE];
    let mut offset = 0;
    let mut out = Vec::new();
    while offset < sector.len() {
        let len = sector[offset] as usize;
        if len == 0 {
            break;
        }
        let rec = &sector[offset..offset + len];
        let extent_lba = u32::from_le_bytes(rec[2..6].try_into().unwrap());
        let size = u32::from_le_bytes(rec[10..14].try_into().unwrap()) as usize;
        let id_len = rec[32] as usize;
        let id = rec[33..33 + id_len].to_vec();
        out.push((id, extent_lba, size));
        offset += len;
    }
    out
}

#[test]
fn instance_id_is_deterministic_and_unique_across_identities() {
    let a1 = instance_id(b"machine-identity-a");
    let a2 = instance_id(b"machine-identity-a");
    let b = instance_id(b"machine-identity-b");
    assert_eq!(a1, a2);
    assert_ne!(a1, b);
    assert!(a1.starts_with("sheprd-"));
    assert_eq!(a1.len(), "sheprd-".len() + 16);
}

#[test]
fn cidata_iso_contains_valid_pvd_joliet_svd_and_exact_file_contents() {
    let meta = meta_data(&name("linux-dev"), b"id-alpha");
    let user = provision_user_data(&name("linux-dev"));
    let iso = build_cidata_iso(meta.as_bytes(), user.as_bytes());
    assert_eq!(iso.len() % SECTOR_SIZE, 0);

    // PVD (sector 16)
    let pvd = &iso[16 * SECTOR_SIZE..17 * SECTOR_SIZE];
    assert_eq!(pvd[0], 1);
    assert_eq!(&pvd[1..6], b"CD001");
    assert_eq!(&pvd[40..46], b"CIDATA");

    // Joliet SVD (sector 17)
    let svd = &iso[17 * SECTOR_SIZE..18 * SECTOR_SIZE];
    assert_eq!(svd[0], 2);
    assert_eq!(&svd[1..6], b"CD001");
    assert_eq!(&svd[88..91], &[0x25, 0x2F, 0x45]);
    assert_eq!(&svd[40..52], &utf16be("cidata"));

    // Terminator (sector 18)
    let term = &iso[18 * SECTOR_SIZE..19 * SECTOR_SIZE];
    assert_eq!(term[0], 255);
    assert_eq!(&term[1..6], b"CD001");

    // Joliet root directory holds exact `meta-data;1` and `user-data;1`
    let entries = read_directory_entries(&iso, SVD_ROOT_LBA);
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[2].0, utf16be("meta-data;1"));
    assert_eq!(entries[3].0, utf16be("user-data;1"));

    let meta_start = entries[2].1 as usize * SECTOR_SIZE;
    let user_start = entries[3].1 as usize * SECTOR_SIZE;
    assert_eq!(&iso[meta_start..meta_start + entries[2].2], meta.as_bytes());
    assert_eq!(&iso[user_start..user_start + entries[3].2], user.as_bytes());
}

#[test]
fn provision_user_data_hardens_grub_and_configures_wayland_desktop() {
    let user = provision_user_data(&name("dev-box"));
    assert!(user.starts_with("#cloud-config\n"));
    assert!(user.contains("hostname: dev-box"));
    assert!(user.contains("GRUB_TIMEOUT=0"));
    assert!(user.contains("GRUB_RECORDFAIL_TIMEOUT=0"));
    assert!(user.contains("fsck.repair=yes"));
    assert!(user.contains("WLR_NO_HARDWARE_CURSORS=1"));
    assert!(user.contains("sway xwayland grim wtype foot epiphany-browser"));
    assert!(user.contains("fonts-noto-cjk fonts-noto-color-emoji"));
    assert!(user.contains("org.gnome.Epiphany ask-for-default false"));
    // Embeds Linux guest agent and runs it in the graphical systemd user session.
    assert!(user.contains("/usr/local/lib/sheprd-machine-agent/agent.py"));
    assert!(user.contains("/usr/local/lib/sheprd-machine-agent/wayland.py"));
    assert!(user.contains("sheprd-machine-agent.service"));
    assert!(user.contains("import-environment DISPLAY WAYLAND_DISPLAY SWAYSOCK"));
    // Non-blocking start prevents deadlocking cloud-final.service with After=cloud-final.service.
    assert!(user.contains("[systemctl, enable, --now, --no-block, sheprd-provision.service]"));
    // Does not pass console=hvc0 when serial console is unattached.
    assert!(!user.contains("console=hvc0"));
}

#[test]
fn clone_user_data_regenerates_machine_id_and_ssh_host_keys_without_apt() {
    let user = clone_user_data(&name("clone-2"));
    assert!(user.starts_with("#cloud-config\n"));
    assert!(user.contains("hostname: clone-2"));
    assert!(user.contains("ssh_deletekeys: true"));
    assert!(user.contains("systemd-machine-id-setup"));
    assert!(user.contains("ssh-keygen"));
    assert!(!user.contains("apt-get"));
}
