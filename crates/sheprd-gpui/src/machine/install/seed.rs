//! Pure-Rust NoCloud `cidata` ISO 9660 + Joliet generator for Linux guests.
//!
//! Attaching a read-only `seed.iso` lets Ubuntu's `cloud-init` provision the
//! desktop environment on initial creation and regenerate per-machine Linux
//! identity (`/etc/machine-id`, SSH host keys, hostname) whenever a saved
//! image is cloned — without requiring macOS to mount or mutate ext4 disks.

use crate::machine::Name;
use base64::Engine as _;
use sha2::{Digest as _, Sha256};
use std::{fs, io, path::Path};

const LINUX_AGENT_PY: &[u8] = include_bytes!("../../../../../linux/sheprd-machine-agent/agent.py");
const LINUX_WAYLAND_PY: &[u8] =
    include_bytes!("../../../../../linux/sheprd-machine-agent/wayland.py");

const SECTOR_SIZE: usize = 2048;
/// System area (sectors 0..16), PVD (16), Joliet SVD (17), Terminator (18),
/// path tables (19..23), PVD root dir (23), Joliet root dir (24), file extents (25..).
const PVD_L_PATH_LBA: u32 = 19;
const PVD_M_PATH_LBA: u32 = 20;
const SVD_L_PATH_LBA: u32 = 21;
const SVD_M_PATH_LBA: u32 = 22;
const PVD_ROOT_LBA: u32 = 23;
const SVD_ROOT_LBA: u32 = 24;
const FIRST_DATA_LBA: u32 = 25;

/// Deterministic `cloud-init` instance-id derived from a machine's unique
/// `identity.bin` bytes.
pub(crate) fn instance_id(identity: &[u8]) -> String {
    let digest = Sha256::digest(identity);
    format!(
        "sheprd-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7]
    ) // i18n: data
}

/// NoCloud `meta-data` payload for `name` and `identity`.
pub(crate) fn meta_data(name: &Name, identity: &[u8]) -> String {
    let id = instance_id(identity);
    format!("instance-id: {id}\nlocal-hostname: {name}\n") // i18n: data
}

/// Initial provisioning `user-data` (`#cloud-config`) for a fresh Ubuntu
/// Wayland desktop machine.
///
/// Uses a two-stage systemd oneshot (`sheprd-provision.service`) so that heavy
/// `apt-get` installation runs on a clean post-resize boot after cloud-init's
/// first-boot online `growpart`/`resize2fs` and GRUB hardening.
pub(crate) fn provision_user_data(name: &Name) -> String {
    let agent_b64 = base64::engine::general_purpose::STANDARD.encode(LINUX_AGENT_PY);
    let wayland_b64 = base64::engine::general_purpose::STANDARD.encode(LINUX_WAYLAND_PY);
    format!(
        r#"#cloud-config
hostname: {name}
manage_etc_hosts: true
users:
  - name: sheprd
    gecos: Sheprd Desktop
    groups: [sudo, video, render, input, tty]
    shell: /bin/bash
    sudo: ALL=(ALL) NOPASSWD:ALL
    lock_passwd: true
write_files:
  - path: /etc/default/grub.d/99-sheprd.cfg
    permissions: '0644'
    content: |
      GRUB_TIMEOUT=0
      GRUB_RECORDFAIL_TIMEOUT=0
      GRUB_CMDLINE_LINUX_DEFAULT="$GRUB_CMDLINE_LINUX_DEFAULT fsck.repair=yes"
  - path: /etc/systemd/system/getty@tty1.service.d/autologin.conf
    permissions: '0644'
    content: |
      [Service]
      ExecStart=
      ExecStart=-/sbin/agetty --noclear --autologin sheprd %I $TERM
  - path: /etc/cloud/cloud.cfg.d/99-sheprd-clone.cfg
    permissions: '0644'
    content: |
      ssh_deletekeys: true
      preserve_hostname: false
  - path: /usr/local/lib/sheprd-machine-agent/agent.py
    permissions: '0755'
    encoding: b64
    content: {agent_b64}
  - path: /usr/local/lib/sheprd-machine-agent/wayland.py
    permissions: '0644'
    encoding: b64
    content: {wayland_b64}
  - path: /usr/local/bin/sheprd-machine-agent
    permissions: '0755'
    content: |
      #!/usr/bin/env bash
      exec /usr/bin/python3 /usr/local/lib/sheprd-machine-agent/agent.py "$@"
  - path: /home/sheprd/.config/systemd/user/sheprd-machine-agent.service
    permissions: '0644'
    content: |
      [Unit]
      Description=Sheprd Linux Wayland guest agent
      After=default.target

      [Service]
      Type=simple
      ExecStart=/usr/local/bin/sheprd-machine-agent
      Restart=always
      RestartSec=1

      [Install]
      WantedBy=default.target
  - path: /home/sheprd/.config/sway/config
    permissions: '0644'
    content: |
      set $mod Mod4
      xwayland enable
      output * bg #1d2026 solid_color
      default_border pixel 2
      bindsym $mod+Return exec foot
      bindsym $mod+Shift+q kill
      exec systemctl --user import-environment DISPLAY WAYLAND_DISPLAY SWAYSOCK XDG_CURRENT_DESKTOP XDG_SESSION_TYPE && systemctl --user restart sheprd-machine-agent.service
      exec foot
  - path: /home/sheprd/.bash_profile
    permissions: '0644'
    content: |
      if [ -z "${{WAYLAND_DISPLAY:-}}" ] && [ "$(tty)" = "/dev/tty1" ] && command -v sway >/dev/null 2>&1; then
        export WLR_NO_HARDWARE_CURSORS=1
        export XDG_SESSION_TYPE=wayland
        export XDG_CURRENT_DESKTOP=sway
        export MOZ_ENABLE_WAYLAND=1
        exec sway >/tmp/sway.log 2>&1
      fi
  - path: /usr/local/sbin/sheprd-provision.sh
    permissions: '0755'
    content: |
      #!/usr/bin/env bash
      set -euo pipefail
      mkdir -p /var/lib/sheprd
      if [ -f /var/lib/sheprd/provisioned ]; then
        exit 0
      fi
      if [ ! -f /var/lib/sheprd/disk-resized ]; then
        update-grub || true
        touch /var/lib/sheprd/disk-resized
        sync
        systemctl reboot
        exit 0
      fi
      export DEBIAN_FRONTEND=noninteractive
      apt-get update
      apt-get install -y --no-install-recommends \
        sway xwayland grim wtype foot epiphany-browser \
        fonts-noto-core fonts-noto-cjk fonts-noto-color-emoji \
        dbus-user-session seatd libglib2.0-bin
      mkdir -p /home/sheprd/.config/systemd/user/default.target.wants
      ln -sf /home/sheprd/.config/systemd/user/sheprd-machine-agent.service \
        /home/sheprd/.config/systemd/user/default.target.wants/sheprd-machine-agent.service
      chown -R sheprd:sheprd /home/sheprd
      loginctl enable-linger sheprd || true
      su - sheprd -c 'dbus-run-session -- gsettings set org.gnome.Epiphany ask-for-default false' || true
      touch /var/lib/sheprd/provisioned
      systemctl disable sheprd-provision.service || true
      systemctl restart getty@tty1.service || true
  - path: /etc/systemd/system/sheprd-provision.service
    permissions: '0644'
    content: |
      [Unit]
      Description=Sheprd Linux Wayland desktop provisioning
      After=network-online.target cloud-final.service
      Wants=network-online.target
      ConditionPathExists=!/var/lib/sheprd/provisioned

      [Service]
      Type=oneshot
      ExecStart=/usr/local/sbin/sheprd-provision.sh

      [Install]
      WantedBy=multi-user.target
runcmd:
  - [chown, -R, 'sheprd:sheprd', /home/sheprd]
  - [systemctl, daemon-reload]
  - [systemctl, enable, --now, --no-block, sheprd-provision.service]
"#
    )
}

/// Per-instance clone `user-data` (`#cloud-config`) attached when cloning a
/// saved Linux image so the clone receives fresh Linux machine-id, SSH host
/// keys, and hostname on first boot without re-running package installation.
pub(crate) fn clone_user_data(name: &Name) -> String {
    format!(
        r#"#cloud-config
hostname: {name}
manage_etc_hosts: true
ssh_deletekeys: true
runcmd:
  - [rm, -f, /etc/machine-id, /var/lib/dbus/machine-id]
  - [systemd-machine-id-setup]
  - [ln, -sf, /etc/machine-id, /var/lib/dbus/machine-id]
  - [ssh-keygen, -A]
"#
    )
}

/// Writes a NoCloud `cidata` ISO for initial provisioning of `name`.
pub(crate) fn write_provision_iso(path: &Path, name: &Name, identity: &[u8]) -> io::Result<()> {
    let meta = meta_data(name, identity);
    let user = provision_user_data(name);
    let iso = build_cidata_iso(meta.as_bytes(), user.as_bytes());
    fs::write(path, iso)
}

/// Writes a NoCloud `cidata` ISO for cloning a saved Linux image as `name`.
pub(crate) fn write_clone_iso(path: &Path, name: &Name, identity: &[u8]) -> io::Result<()> {
    let meta = meta_data(name, identity);
    let user = clone_user_data(name);
    let iso = build_cidata_iso(meta.as_bytes(), user.as_bytes());
    fs::write(path, iso)
}

/// Builds a minimal ECMA-119 ISO 9660 + Joliet (UCS-2 Level 3) image with
/// volume label `cidata` containing `meta-data` and `user-data` in its root.
pub(crate) fn build_cidata_iso(meta_bytes: &[u8], user_bytes: &[u8]) -> Vec<u8> {
    let meta_sectors = sectors_for(meta_bytes.len());
    let user_sectors = sectors_for(user_bytes.len());
    let meta_lba = FIRST_DATA_LBA;
    let user_lba = meta_lba + meta_sectors;
    let total_sectors = user_lba + user_sectors;

    let pvd_root = build_directory(
        PVD_ROOT_LBA,
        &[
            (b"META_DAT.;1".to_vec(), meta_lba, meta_bytes.len()),
            (b"USER_DAT.;1".to_vec(), user_lba, user_bytes.len()),
        ],
    );
    let svd_root = build_directory(
        SVD_ROOT_LBA,
        &[
            (utf16be("meta-data;1"), meta_lba, meta_bytes.len()),
            (utf16be("user-data;1"), user_lba, user_bytes.len()),
        ],
    );

    let mut image = vec![0u8; total_sectors as usize * SECTOR_SIZE];

    // Sector 16: Primary Volume Descriptor
    write_volume_descriptor(
        sector_mut(&mut image, 16),
        VolumeDescriptor {
            kind: 1,
            volume_id: pad_ascii("CIDATA", 32),
            escape: None,
            total_sectors,
            l_path_lba: PVD_L_PATH_LBA,
            m_path_lba: PVD_M_PATH_LBA,
            root_lba: PVD_ROOT_LBA,
            root_len: pvd_root.len(),
        },
    );

    // Sector 17: Supplementary Volume Descriptor (Joliet UCS-2 Level 3)
    write_volume_descriptor(
        sector_mut(&mut image, 17),
        VolumeDescriptor {
            kind: 2,
            volume_id: pad_utf16be("cidata", 32),
            escape: Some([0x25, 0x2F, 0x45]),
            total_sectors,
            l_path_lba: SVD_L_PATH_LBA,
            m_path_lba: SVD_M_PATH_LBA,
            root_lba: SVD_ROOT_LBA,
            root_len: svd_root.len(),
        },
    );

    // Sector 18: Volume Descriptor Set Terminator
    let term = sector_mut(&mut image, 18);
    term[0] = 255;
    term[1..6].copy_from_slice(b"CD001");
    term[6] = 1;

    // Sectors 19..22: Path tables (root entry only, 10 bytes each)
    write_path_table(sector_mut(&mut image, PVD_L_PATH_LBA), PVD_ROOT_LBA, false);
    write_path_table(sector_mut(&mut image, PVD_M_PATH_LBA), PVD_ROOT_LBA, true);
    write_path_table(sector_mut(&mut image, SVD_L_PATH_LBA), SVD_ROOT_LBA, false);
    write_path_table(sector_mut(&mut image, SVD_M_PATH_LBA), SVD_ROOT_LBA, true);

    // Sectors 23..24: Root directories
    sector_mut(&mut image, PVD_ROOT_LBA)[..pvd_root.len()].copy_from_slice(&pvd_root);
    sector_mut(&mut image, SVD_ROOT_LBA)[..svd_root.len()].copy_from_slice(&svd_root);

    // File extents
    write_extent(&mut image, meta_lba, meta_bytes);
    write_extent(&mut image, user_lba, user_bytes);

    image
}

struct VolumeDescriptor {
    kind: u8,
    volume_id: [u8; 32],
    escape: Option<[u8; 3]>,
    total_sectors: u32,
    l_path_lba: u32,
    m_path_lba: u32,
    root_lba: u32,
    root_len: usize,
}

fn write_volume_descriptor(sector: &mut [u8], desc: VolumeDescriptor) {
    sector[0] = desc.kind;
    sector[1..6].copy_from_slice(b"CD001");
    sector[6] = 1; // version
    sector[8..40].fill(b' ');
    sector[40..72].copy_from_slice(&desc.volume_id);
    write_both_u32(&mut sector[80..88], desc.total_sectors);
    if let Some(escape) = desc.escape {
        sector[88..91].copy_from_slice(&escape);
    }
    write_both_u16(&mut sector[120..124], 1); // volume set size
    write_both_u16(&mut sector[124..128], 1); // volume sequence number
    write_both_u16(&mut sector[128..132], SECTOR_SIZE as u16);
    write_both_u32(&mut sector[132..140], 10); // root-only path table size
    sector[140..144].copy_from_slice(&desc.l_path_lba.to_le_bytes());
    sector[148..152].copy_from_slice(&desc.m_path_lba.to_be_bytes());
    let root_record = directory_record(&[0], desc.root_lba, desc.root_len, 0x02);
    sector[156..156 + root_record.len()].copy_from_slice(&root_record);
    sector[190..813].fill(b' ');
    // Timestamps (17 bytes each, zeros = unspecified)
    sector[881] = 1; // file structure version
}

fn write_path_table(sector: &mut [u8], root_lba: u32, big_endian: bool) {
    sector[0] = 1; // directory identifier length
    sector[1] = 0; // extended attribute record length
    if big_endian {
        sector[2..6].copy_from_slice(&root_lba.to_be_bytes());
        sector[6..8].copy_from_slice(&1u16.to_be_bytes());
    } else {
        sector[2..6].copy_from_slice(&root_lba.to_le_bytes());
        sector[6..8].copy_from_slice(&1u16.to_le_bytes());
    }
    sector[8] = 0; // root identifier
    sector[9] = 0; // padding
}

fn build_directory(self_lba: u32, files: &[(Vec<u8>, u32, usize)]) -> Vec<u8> {
    // First pass computes exact directory byte length so `.` and `..` record it.
    let mut entries_len = directory_record(&[0], self_lba, 0, 0x02).len()
        + directory_record(&[1], self_lba, 0, 0x02).len();
    for (name, lba, size) in files {
        entries_len += directory_record(name, *lba, *size, 0x00).len();
    }
    let dir_len = entries_len.max(SECTOR_SIZE);
    let mut out = Vec::with_capacity(entries_len);
    out.extend_from_slice(&directory_record(&[0], self_lba, dir_len, 0x02));
    out.extend_from_slice(&directory_record(&[1], self_lba, dir_len, 0x02));
    for (name, lba, size) in files {
        out.extend_from_slice(&directory_record(name, *lba, *size, 0x00));
    }
    out
}

fn directory_record(identifier: &[u8], lba: u32, size: usize, flags: u8) -> Vec<u8> {
    let id_len = identifier.len();
    let pad = usize::from(id_len.is_multiple_of(2));
    let len = 33 + id_len + pad;
    let mut rec = vec![0u8; len];
    rec[0] = u8::try_from(len).unwrap_or(u8::MAX);
    write_both_u32(&mut rec[2..10], lba);
    write_both_u32(&mut rec[10..18], u32::try_from(size).unwrap_or(u32::MAX));
    // Recording date: 2026-01-01 00:00:00 UTC
    rec[18] = 126; // years since 1900
    rec[19] = 1;
    rec[20] = 1;
    rec[25] = flags;
    write_both_u16(&mut rec[28..32], 1); // volume sequence number
    rec[32] = u8::try_from(id_len).unwrap_or(u8::MAX);
    rec[33..33 + id_len].copy_from_slice(identifier);
    rec
}

fn write_extent(image: &mut [u8], lba: u32, data: &[u8]) {
    let start = lba as usize * SECTOR_SIZE;
    image[start..start + data.len()].copy_from_slice(data);
}

fn sector_mut(image: &mut [u8], lba: u32) -> &mut [u8] {
    let start = lba as usize * SECTOR_SIZE;
    &mut image[start..start + SECTOR_SIZE]
}

fn sectors_for(len: usize) -> u32 {
    let count = len.max(1).div_ceil(SECTOR_SIZE);
    u32::try_from(count).unwrap_or(u32::MAX)
}

fn write_both_u16(dst: &mut [u8], value: u16) {
    dst[0..2].copy_from_slice(&value.to_le_bytes());
    dst[2..4].copy_from_slice(&value.to_be_bytes());
}

fn write_both_u32(dst: &mut [u8], value: u32) {
    dst[0..4].copy_from_slice(&value.to_le_bytes());
    dst[4..8].copy_from_slice(&value.to_be_bytes());
}

fn pad_ascii(text: &str, len: usize) -> [u8; 32] {
    let mut out = [b' '; 32];
    let bytes = text.as_bytes();
    let count = bytes.len().min(len).min(32);
    out[..count].copy_from_slice(&bytes[..count]);
    out
}

fn pad_utf16be(text: &str, len: usize) -> [u8; 32] {
    let mut out = [0u8; 32];
    for chunk in out.as_chunks_mut::<2>().0 {
        chunk.copy_from_slice(&0x0020u16.to_be_bytes());
    }
    let encoded = utf16be(text);
    let count = encoded.len().min(len).min(32);
    out[..count].copy_from_slice(&encoded[..count]);
    out
}

fn utf16be(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

#[cfg(test)]
mod tests;
