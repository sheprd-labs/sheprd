#![allow(clippy::unwrap_used)]
use super::*;
use flate2::{Compression, write::DeflateEncoder};

const TEST_CLUSTER_BITS: u32 = 12; // 4096 bytes
const TEST_CLUSTER_SIZE: usize = 1 << TEST_CLUSTER_BITS;

/// Builds a minimal valid QCOW2 v3 image with 4 KiB clusters containing:
/// - cluster 0: uncompressed bytes from `c0`
/// - cluster 1: sparse (unallocated)
/// - cluster 2: raw-deflate compressed bytes from `c2`
/// - cluster 3: explicit zero-flagged cluster
fn build_fixture_qcow2(c0: &[u8; TEST_CLUSTER_SIZE], c2: &[u8; TEST_CLUSTER_SIZE]) -> Vec<u8> {
    let cluster = TEST_CLUSTER_SIZE;
    let virtual_size = (cluster * 4) as u64;
    let l1_offset = cluster as u64; // cluster 1
    let l2_offset = (cluster * 2) as u64; // cluster 2
    let c0_offset = (cluster * 3) as u64; // cluster 3
    let c2_offset = (cluster * 4) as u64 + 77; // intentionally unaligned compressed offset

    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(c2).unwrap();
    let compressed = encoder.finish().unwrap();

    let total_len = c2_offset as usize + compressed.len() + 512;
    let mut img = vec![0u8; total_len];

    // Header (v3)
    img[0..4].copy_from_slice(&MAGIC);
    img[4..8].copy_from_slice(&3u32.to_be_bytes());
    img[20..24].copy_from_slice(&TEST_CLUSTER_BITS.to_be_bytes());
    img[24..32].copy_from_slice(&virtual_size.to_be_bytes());
    img[36..40].copy_from_slice(&1u32.to_be_bytes()); // l1_size = 1
    img[40..48].copy_from_slice(&l1_offset.to_be_bytes());
    img[96..100].copy_from_slice(&4u32.to_be_bytes()); // refcount_order = 4 (16-bit)
    img[100..104].copy_from_slice(&104u32.to_be_bytes()); // header_length

    // L1 entry 0 -> L2 table
    img[l1_offset as usize..l1_offset as usize + 8].copy_from_slice(&l2_offset.to_be_bytes());

    // L2 entries
    let l2 = l2_offset as usize;
    // entry 0: standard uncompressed cluster
    img[l2..l2 + 8].copy_from_slice(&c0_offset.to_be_bytes());
    // entry 1: 0 (unallocated sparse)
    // entry 2: compressed cluster
    let csize_shift = 62 - (TEST_CLUSTER_BITS - 8);
    let end_offset = c2_offset + compressed.len() as u64;
    let start_sector = c2_offset / 512;
    let end_sector = end_offset.div_ceil(512);
    let nb_csectors = end_sector - start_sector;
    let compressed_entry = L2_COMPRESSED_FLAG | ((nb_csectors - 1) << csize_shift) | c2_offset;
    img[l2 + 16..l2 + 24].copy_from_slice(&compressed_entry.to_be_bytes());
    // entry 3: explicit zero cluster
    img[l2 + 24..l2 + 32].copy_from_slice(&L2_ZERO_FLAG.to_be_bytes());

    // Data
    img[c0_offset as usize..c0_offset as usize + cluster].copy_from_slice(c0);
    img[c2_offset as usize..c2_offset as usize + compressed.len()].copy_from_slice(&compressed);

    img
}

#[test]
fn converts_uncompressed_compressed_and_sparse_clusters_and_expands_target() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("ubuntu.img");
    let dst = dir.path().join("disk.img");

    let mut c0 = [0u8; TEST_CLUSTER_SIZE];
    c0[..12].copy_from_slice(b"HELLO-QCOW2!");
    c0[TEST_CLUSTER_SIZE - 1] = 0xAA;

    let mut c2 = [0x5Au8; TEST_CLUSTER_SIZE];
    c2[100..115].copy_from_slice(b"COMPRESSED-DATA");

    std::fs::write(&src, build_fixture_qcow2(&c0, &c2)).unwrap();

    let expanded = (TEST_CLUSTER_SIZE * 8) as u64;
    let written = convert_to_raw(&src, &dst, expanded, &AtomicBool::new(false)).unwrap();
    assert_eq!(written, expanded);

    let raw = std::fs::read(&dst).unwrap();
    assert_eq!(raw.len(), expanded as usize);
    assert_eq!(&raw[0..TEST_CLUSTER_SIZE], &c0);
    assert!(
        raw[TEST_CLUSTER_SIZE..TEST_CLUSTER_SIZE * 2]
            .iter()
            .all(|&b| b == 0)
    );
    assert_eq!(&raw[TEST_CLUSTER_SIZE * 2..TEST_CLUSTER_SIZE * 3], &c2);
    assert!(raw[TEST_CLUSTER_SIZE * 3..].iter().all(|&b| b == 0));
}

#[test]
fn rejects_non_qcow2_backing_file_encrypted_and_incompatible_images() {
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("out.raw");
    let cancel = AtomicBool::new(false);

    let bad_magic = dir.path().join("bad-magic.img");
    std::fs::write(&bad_magic, vec![0u8; 256]).unwrap();
    assert!(matches!(
        convert_to_raw(&bad_magic, &dst, 4096, &cancel),
        Err(Error::ImageFormat(_))
    ));

    let base = build_fixture_qcow2(&[1u8; TEST_CLUSTER_SIZE], &[2u8; TEST_CLUSTER_SIZE]);

    let mut with_backing = base.clone();
    with_backing[8..16].copy_from_slice(&64u64.to_be_bytes());
    let backing_path = dir.path().join("backing.img");
    std::fs::write(&backing_path, &with_backing).unwrap();
    assert!(matches!(
        convert_to_raw(&backing_path, &dst, 4096, &cancel),
        Err(Error::ImageFormat(_))
    ));

    let mut encrypted = base.clone();
    encrypted[32..36].copy_from_slice(&1u32.to_be_bytes());
    let enc_path = dir.path().join("encrypted.img");
    std::fs::write(&enc_path, &encrypted).unwrap();
    assert!(matches!(
        convert_to_raw(&enc_path, &dst, 4096, &cancel),
        Err(Error::ImageFormat(_))
    ));

    let mut zstd_incompat = base;
    zstd_incompat[72..80].copy_from_slice(&(1u64 << 3).to_be_bytes());
    let incompat_path = dir.path().join("zstd.img");
    std::fs::write(&incompat_path, &zstd_incompat).unwrap();
    assert!(matches!(
        convert_to_raw(&incompat_path, &dst, 4096, &cancel),
        Err(Error::ImageFormat(_))
    ));
}

#[test]
fn respects_cancellation_during_conversion() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("ubuntu.img");
    let dst = dir.path().join("disk.img");
    std::fs::write(
        &src,
        build_fixture_qcow2(&[1u8; TEST_CLUSTER_SIZE], &[2u8; TEST_CLUSTER_SIZE]),
    )
    .unwrap();

    let cancel = AtomicBool::new(true);
    let result = convert_to_raw(&src, &dst, 4096, &cancel);
    assert!(matches!(result, Err(Error::Cancelled)));
}

/// Opt-in test verifying our pure-Rust QCOW2 converter against a real Ubuntu
/// ARM64 cloud image specified by `SHEPRD_TEST_UBUNTU_QCOW2`.
#[test]
#[ignore = "requires SHEPRD_TEST_UBUNTU_QCOW2 pointing to a local QCOW2 cloud image"]
fn converts_real_ubuntu_qcow2_when_selected() {
    let source = std::path::PathBuf::from(std::env::var("SHEPRD_TEST_UBUNTU_QCOW2").unwrap());
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("disk.img");
    let size = convert_to_raw(&source, &dst, 16 << 30, &AtomicBool::new(false)).unwrap();
    assert_eq!(size, 16 << 30);
    // LBA 1 (offset 512) of an Ubuntu ARM64 cloud image holds the GPT header "EFI PART".
    let mut file = File::open(&dst).unwrap();
    let mut gpt = [0u8; 8];
    file.seek(SeekFrom::Start(512)).unwrap();
    file.read_exact(&mut gpt).unwrap();
    assert_eq!(&gpt, b"EFI PART");
}
