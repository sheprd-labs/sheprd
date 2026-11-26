//! Pure-Rust QCOW2 v2/v3 to sparse raw disk image converter.
//!
//! Cloud images such as Ubuntu's ARM64 `.img` releases are distributed as
//! zlib-compressed QCOW2 files, whereas Apple Virtualization.framework attaches
//! raw disk images. Converting directly in safe Rust avoids requiring external
//! tools like `qemu-img` and enforces strict bounds on untrusted image headers.

use super::Error;
use flate2::read::DeflateDecoder;
use std::{
    fs::{File, OpenOptions},
    io::{Read as _, Seek as _, SeekFrom, Write as _},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

const MAGIC: [u8; 4] = *b"QFI\xfb";
const HEADER_V2_LEN: usize = 72;
const HEADER_V3_LEN: usize = 104;
/// Minimum cluster size: 512 bytes.
const MIN_CLUSTER_BITS: u32 = 9;
/// Maximum cluster size: 2 MiB, bounding per-cluster allocations.
const MAX_CLUSTER_BITS: u32 = 21;
/// Maximum supported virtual disk size: 64 GiB.
const MAX_VIRTUAL_SIZE: u64 = 64 << 30;
/// Maximum number of L1 table entries accepted from an untrusted header.
const MAX_L1_ENTRIES: usize = 65_536;
/// Bits 9..=55 hold cluster-aligned host offsets in standard L1/L2 entries.
const OFFSET_MASK: u64 = 0x00ff_ffff_ffff_fe00;
/// Bit 62 marks a compressed L2 cluster descriptor.
const L2_COMPRESSED_FLAG: u64 = 1 << 62;
/// Bit 0 marks a standard L2 cluster as reading all zeros (QCOW2 v3).
const L2_ZERO_FLAG: u64 = 1;
/// Bit 0 of `incompatible_features` is the dirty refcount bit (safe for read-only conversion).
const INCOMPAT_DIRTY_BIT: u64 = 1;

#[derive(Debug)]
struct Header {
    cluster_bits: u32,
    cluster_size: u64,
    virtual_size: u64,
    l1_size: usize,
    l1_table_offset: u64,
}

impl Header {
    fn parse(file: &mut File, file_len: u64) -> Result<Self, Error> {
        if file_len < HEADER_V2_LEN as u64 {
            return Err(format_error("image shorter than QCOW2 header"));
        }
        let mut raw = [0u8; HEADER_V3_LEN];
        let to_read = usize::try_from(file_len.min(HEADER_V3_LEN as u64)).unwrap_or(HEADER_V3_LEN);
        file.seek(SeekFrom::Start(0))
            .map_err(|source| format_error(format!("cannot seek header: {source}")))?;
        file.read_exact(&mut raw[..to_read])
            .map_err(|source| format_error(format!("cannot read header: {source}")))?;

        if raw[0..4] != MAGIC {
            return Err(format_error("not a QCOW2 image (bad magic)"));
        }
        let version = be_u32(&raw, 4)?;
        if !matches!(version, 2 | 3) {
            return Err(format_error(format!("unsupported QCOW2 version {version}")));
        }
        let backing_offset = be_u64(&raw, 8)?;
        let backing_size = be_u32(&raw, 16)?;
        if backing_offset != 0 || backing_size != 0 {
            return Err(format_error("QCOW2 backing files are not supported"));
        }
        let cluster_bits = be_u32(&raw, 20)?;
        if !(MIN_CLUSTER_BITS..=MAX_CLUSTER_BITS).contains(&cluster_bits) {
            return Err(format_error(format!(
                "unsupported QCOW2 cluster_bits {cluster_bits}"
            )));
        }
        let cluster_size = 1u64 << cluster_bits;
        let virtual_size = be_u64(&raw, 24)?;
        if virtual_size == 0 || virtual_size > MAX_VIRTUAL_SIZE {
            return Err(format_error(format!(
                "invalid QCOW2 virtual size {virtual_size}"
            )));
        }
        let crypt_method = be_u32(&raw, 32)?;
        if crypt_method != 0 {
            return Err(format_error("encrypted QCOW2 images are not supported"));
        }
        let l1_size = usize::try_from(be_u32(&raw, 36)?)
            .map_err(|_| format_error("QCOW2 L1 table size overflow"))?;
        let l1_table_offset = be_u64(&raw, 40)?;

        if version == 3 {
            if to_read < HEADER_V3_LEN {
                return Err(format_error("truncated QCOW2 v3 header"));
            }
            let incompatible = be_u64(&raw, 72)?;
            if incompatible & !INCOMPAT_DIRTY_BIT != 0 {
                return Err(format_error(format!(
                    "unsupported QCOW2 incompatible features 0x{incompatible:x}"
                )));
            }
            let header_len = be_u32(&raw, 100)?;
            if usize::try_from(header_len).unwrap_or(0) < HEADER_V3_LEN {
                return Err(format_error("invalid QCOW2 v3 header_length"));
            }
        }

        if l1_size == 0 || l1_size > MAX_L1_ENTRIES {
            return Err(format_error(format!("invalid QCOW2 L1 size {l1_size}")));
        }
        if !l1_table_offset.is_multiple_of(cluster_size) {
            return Err(format_error("unaligned QCOW2 L1 table offset"));
        }
        let l1_bytes = (l1_size as u64)
            .checked_mul(8)
            .ok_or_else(|| format_error("QCOW2 L1 table byte length overflow"))?;
        let l1_end = l1_table_offset
            .checked_add(l1_bytes)
            .ok_or_else(|| format_error("QCOW2 L1 table end overflow"))?;
        if l1_end > file_len {
            return Err(format_error("QCOW2 L1 table extends past end of file"));
        }

        Ok(Self {
            cluster_bits,
            cluster_size,
            virtual_size,
            l1_size,
            l1_table_offset,
        })
    }
}

/// Converts QCOW2 image `source` into a sparse raw disk at `target`, expanded
/// to at least `target_size` bytes. Returns the resulting raw disk size.
pub(super) fn convert_to_raw(
    source: &Path,
    target: &Path,
    target_size: u64,
    cancel: &AtomicBool,
) -> Result<u64, Error> {
    let mut input = File::open(source).map_err(|err| file_error(source, err))?;
    let file_len = input
        .metadata()
        .map_err(|err| file_error(source, err))?
        .len();
    let header = Header::parse(&mut input, file_len)?;
    let final_size = target_size.max(header.virtual_size);

    let mut output = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(target)
        .map_err(|err| file_error(target, err))?;

    let l1_entries = read_u64_table(&mut input, header.l1_table_offset, header.l1_size, source)?;
    let cluster_size_usize =
        usize::try_from(header.cluster_size).map_err(|_| format_error("cluster size overflow"))?;
    let l2_entries = cluster_size_usize / 8;
    let csize_shift = 62 - (header.cluster_bits - 8);
    let csize_mask = (1u64 << (header.cluster_bits - 8)) - 1;
    let coffset_mask = (1u64 << csize_shift) - 1;

    let mut cluster_buf = vec![0u8; cluster_size_usize];
    let mut compressed_buf = Vec::new();

    for (l1_idx, &l1_entry) in l1_entries.iter().enumerate() {
        check_cancel(cancel)?;
        let l2_offset = l1_entry & OFFSET_MASK;
        if l2_offset == 0 {
            continue;
        }
        if !l2_offset.is_multiple_of(header.cluster_size) {
            return Err(format_error("unaligned QCOW2 L2 table offset"));
        }
        let l2_end = l2_offset
            .checked_add(header.cluster_size)
            .ok_or_else(|| format_error("QCOW2 L2 table end overflow"))?;
        if l2_end > file_len {
            return Err(format_error("QCOW2 L2 table extends past end of file"));
        }
        let l2_table = read_u64_table(&mut input, l2_offset, l2_entries, source)?;
        for (l2_idx, &l2_entry) in l2_table.iter().enumerate() {
            if l2_idx.is_multiple_of(64) {
                check_cancel(cancel)?;
            }
            let cluster_index = (l1_idx as u64)
                .checked_mul(l2_entries as u64)
                .and_then(|base| base.checked_add(l2_idx as u64))
                .ok_or_else(|| format_error("QCOW2 cluster index overflow"))?;
            let guest_offset = cluster_index
                .checked_mul(header.cluster_size)
                .ok_or_else(|| format_error("QCOW2 guest offset overflow"))?;
            if guest_offset >= header.virtual_size {
                break;
            }
            let write_len =
                usize::try_from((header.virtual_size - guest_offset).min(header.cluster_size))
                    .unwrap_or(cluster_size_usize);

            if l2_entry & L2_COMPRESSED_FLAG != 0 {
                let coffset = l2_entry & coffset_mask;
                let nb_csectors = ((l2_entry >> csize_shift) & csize_mask) + 1;
                let max_csize = nb_csectors
                    .checked_mul(512)
                    .ok_or_else(|| format_error("compressed cluster size overflow"))?
                    .saturating_sub(coffset & 511);
                if coffset >= file_len
                    || max_csize == 0
                    || max_csize > header.cluster_size.saturating_mul(2)
                {
                    return Err(format_error("invalid QCOW2 compressed cluster descriptor"));
                }
                let actual_csize = usize::try_from(max_csize.min(file_len - coffset)).unwrap_or(0);
                compressed_buf.resize(actual_csize, 0);
                input
                    .seek(SeekFrom::Start(coffset))
                    .map_err(|err| file_error(source, err))?;
                input
                    .read_exact(&mut compressed_buf)
                    .map_err(|err| file_error(source, err))?;
                decompress_cluster(&compressed_buf, &mut cluster_buf, write_len)?;
            } else {
                if l2_entry & L2_ZERO_FLAG != 0 {
                    continue;
                }
                let host_offset = l2_entry & OFFSET_MASK;
                if host_offset == 0 {
                    continue;
                }
                if !host_offset.is_multiple_of(header.cluster_size) || host_offset >= file_len {
                    return Err(format_error("invalid QCOW2 cluster host offset"));
                }
                let avail = usize::try_from((file_len - host_offset).min(write_len as u64))
                    .unwrap_or(write_len);
                input
                    .seek(SeekFrom::Start(host_offset))
                    .map_err(|err| file_error(source, err))?;
                input
                    .read_exact(&mut cluster_buf[..avail])
                    .map_err(|err| file_error(source, err))?;
                cluster_buf[avail..write_len].fill(0);
            }

            if !is_all_zero(&cluster_buf[..write_len]) {
                output
                    .seek(SeekFrom::Start(guest_offset))
                    .map_err(|err| file_error(target, err))?;
                output
                    .write_all(&cluster_buf[..write_len])
                    .map_err(|err| file_error(target, err))?;
            }
        }
    }

    check_cancel(cancel)?;
    output
        .set_len(final_size)
        .map_err(|err| file_error(target, err))?;
    output.sync_all().map_err(|err| file_error(target, err))?;
    Ok(final_size)
}

fn decompress_cluster(compressed: &[u8], out: &mut [u8], required: usize) -> Result<(), Error> {
    let mut decoder = DeflateDecoder::new(compressed);
    let mut total = 0;
    while total < out.len() {
        let read = decoder.read(&mut out[total..]).map_err(|source| {
            format_error(format!("corrupt compressed QCOW2 cluster: {source}"))
        })?;
        if read == 0 {
            break;
        }
        total += read;
    }
    if total < required {
        return Err(format_error(format!(
            "truncated compressed QCOW2 cluster ({total} of {required} bytes)"
        )));
    }
    Ok(())
}

fn read_u64_table(
    file: &mut File,
    offset: u64,
    count: usize,
    path: &Path,
) -> Result<Vec<u64>, Error> {
    let mut bytes = vec![0u8; count.saturating_mul(8)];
    file.seek(SeekFrom::Start(offset))
        .map_err(|err| file_error(path, err))?;
    file.read_exact(&mut bytes)
        .map_err(|err| file_error(path, err))?;
    Ok(bytes
        .as_chunks::<8>()
        .0
        .iter()
        .map(|&chunk| u64::from_be_bytes(chunk))
        .collect())
}

fn be_u32(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    let slice: [u8; 4] = bytes
        .get(offset..offset + 4)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| format_error("QCOW2 header truncated"))?;
    Ok(u32::from_be_bytes(slice))
}

fn be_u64(bytes: &[u8], offset: usize) -> Result<u64, Error> {
    let slice: [u8; 8] = bytes
        .get(offset..offset + 8)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| format_error("QCOW2 header truncated"))?;
    Ok(u64::from_be_bytes(slice))
}

fn is_all_zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|&byte| byte == 0)
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), Error> {
    if cancel.load(Ordering::Relaxed) {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}

fn format_error(reason: impl Into<String>) -> Error {
    Error::ImageFormat(reason.into())
}

fn file_error(path: &Path, source: std::io::Error) -> Error {
    Error::File {
        path: path.to_owned(),
        source,
    }
}

#[cfg(test)]
mod tests;
