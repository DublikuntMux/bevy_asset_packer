use std::{collections::BTreeSet, path::PathBuf};

use aes_gcm::{
    Aes128Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use anyhow::{Context, Result, anyhow, bail, ensure};

pub const MAGIC: &[u8; 5] = b"BPACK";
pub const VERSION: u8 = 2;
pub const HEADER_LEN: usize = 27;
const FLAG_ENCRYPTED: u8 = 1;
pub const TAG_LEN: u64 = 16;

#[derive(Debug)]
pub struct Header {
    pub encrypted: bool,
    pub index_len: u64,
    pub nonce: [u8; 12],
}

impl Header {
    #[cfg(any(not(target_arch = "wasm32"), test))]
    pub const fn new(index_len: u64, encrypted: bool, nonce: [u8; 12]) -> Self {
        Self {
            encrypted,
            index_len,
            nonce,
        }
    }

    pub fn encode(&self) -> [u8; HEADER_LEN] {
        let mut bytes = [0; HEADER_LEN];
        bytes[..5].copy_from_slice(MAGIC);
        bytes[5] = VERSION;
        bytes[6] = u8::from(self.encrypted);
        bytes[7..15].copy_from_slice(&self.index_len.to_le_bytes());
        bytes[15..].copy_from_slice(&self.nonce);
        bytes
    }

    pub fn decode(bytes: &[u8], key_present: bool, total_len: u64) -> Result<Self> {
        ensure!(bytes.len() == HEADER_LEN, "invalid bundle header length");
        ensure!(&bytes[..5] == MAGIC, "invalid bundle magic");
        ensure!(
            bytes[5] == VERSION,
            "unsupported bundle version: {}",
            bytes[5]
        );
        ensure!(bytes[6] & !FLAG_ENCRYPTED == 0, "unsupported bundle flags");
        let encrypted = bytes[6] & FLAG_ENCRYPTED != 0;
        ensure!(
            !encrypted || key_present,
            "bundle requires an encryption key"
        );
        ensure!(
            encrypted || !key_present,
            "unencrypted bundle does not accept an encryption key"
        );
        let index_len = u64::from_le_bytes(bytes[7..15].try_into()?);
        let min_index_len = 4 + if encrypted { TAG_LEN } else { 0 };
        ensure!(index_len >= min_index_len, "invalid bundle index length");
        let payload_offset = (HEADER_LEN as u64)
            .checked_add(index_len)
            .context("bundle index length overflow")?;
        ensure!(
            payload_offset <= total_len,
            "bundle index exceeds archive length"
        );
        let nonce = bytes[15..].try_into()?;
        Ok(Self {
            encrypted,
            index_len,
            nonce,
        })
    }

    pub const fn payload_offset(&self) -> u64 {
        HEADER_LEN as u64 + self.index_len
    }
}

#[derive(Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub file: Option<FileEntry>,
}

#[derive(Debug)]
pub struct FileEntry {
    pub offset: u64,
    pub stored_len: u64,
    pub original_len: u64,
    pub compressed: bool,
}

impl Entry {
    pub fn associated_data(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.encode_into(&mut bytes)?;
        Ok(bytes)
    }

    fn encode_into(&self, bytes: &mut Vec<u8>) -> Result<()> {
        let path = self.path.to_str().context("asset paths must be UTF-8")?;
        validate_path(path)?;
        bytes.extend_from_slice(
            &u32::try_from(path.len())
                .context("asset path too long")?
                .to_le_bytes(),
        );
        bytes.extend_from_slice(path.as_bytes());
        bytes.push(u8::from(self.file.is_some()));
        if let Some(file) = &self.file {
            bytes.extend_from_slice(&file.offset.to_le_bytes());
            bytes.extend_from_slice(&file.stored_len.to_le_bytes());
            bytes.extend_from_slice(&file.original_len.to_le_bytes());
            bytes.push(u8::from(file.compressed));
        }
        Ok(())
    }
}

#[cfg(any(not(target_arch = "wasm32"), test))]
pub fn encode_index(entries: &[Entry]) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(
        &u32::try_from(entries.len())
            .context("too many bundle entries")?
            .to_le_bytes(),
    );
    for entry in entries {
        entry.encode_into(&mut bytes)?;
    }
    Ok(bytes)
}

pub fn decode_index(bytes: &[u8], payload_len: u64) -> Result<Vec<Entry>> {
    let mut cursor = Cursor { bytes };
    let count = usize::try_from(cursor.u32()?)?;
    ensure!(
        count <= cursor.bytes.len() / 6,
        "bundle entry count exceeds index length"
    );
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let path_len = usize::try_from(cursor.u32()?)?;
        let path =
            std::str::from_utf8(cursor.take(path_len)?).context("asset paths must be UTF-8")?;
        validate_path(path)?;
        let file = match cursor.byte()? {
            0 => None,
            1 => Some(FileEntry {
                offset: cursor.u64()?,
                stored_len: cursor.u64()?,
                original_len: cursor.u64()?,
                compressed: match cursor.byte()? {
                    0 => false,
                    1 => true,
                    _ => bail!("invalid asset compression flag"),
                },
            }),
            _ => bail!("invalid bundle entry kind"),
        };
        entries.push(Entry {
            path: PathBuf::from(path),
            file,
        });
    }
    ensure!(cursor.bytes.is_empty(), "trailing bytes in bundle index");
    validate_entries(&entries, payload_len)?;
    Ok(entries)
}

fn validate_path(path: &str) -> Result<()> {
    ensure!(!path.is_empty(), "empty asset path");
    ensure!(
        !path.contains(['\\', ':', '\0']),
        "unsafe asset path: {path}"
    );
    ensure!(
        path.split('/')
            .all(|component| !matches!(component, "" | "." | "..")),
        "noncanonical asset path: {path}"
    );
    Ok(())
}

fn validate_entries(entries: &[Entry], payload_len: u64) -> Result<()> {
    let mut paths = BTreeSet::new();
    let mut files = BTreeSet::new();
    let mut ranges = Vec::new();
    for entry in entries {
        let path = entry.path.to_str().context("asset paths must be UTF-8")?;
        ensure!(paths.insert(path), "duplicate asset path: {path}");
        if let Some(file) = &entry.file {
            files.insert(path);
            let end = file
                .offset
                .checked_add(file.stored_len)
                .context("asset payload range overflow")?;
            ensure!(
                end <= payload_len,
                "asset payload exceeds archive length: {path}"
            );
            ensure!(
                !file.compressed || file.stored_len > 0,
                "empty compressed asset payload: {path}"
            );
            if file.stored_len > 0 {
                ranges.push((file.offset, end));
            }
        }
    }
    for path in &paths {
        for (position, _) in path.match_indices('/') {
            ensure!(
                !files.contains(&path[..position]),
                "asset file used as a directory: {path}"
            );
        }
    }
    ranges.sort_unstable();
    ensure!(
        ranges.windows(2).all(|pair| pair[0].1 <= pair[1].0),
        "overlapping asset payloads"
    );
    Ok(())
}

struct Cursor<'a> {
    bytes: &'a [u8],
}

impl<'a> Cursor<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let (value, remaining) = self
            .bytes
            .split_at_checked(len)
            .context("truncated bundle index")?;
        self.bytes = remaining;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into()?))
    }
}

pub fn nonce_for_entry(base: &[u8; 12], ordinal: usize) -> Result<[u8; 12]> {
    let counter = u64::try_from(ordinal)?
        .checked_add(1)
        .context("too many encrypted bundle entries")?;
    let mut nonce = *base;
    for (byte, counter_byte) in nonce[4..].iter_mut().zip(counter.to_le_bytes()) {
        *byte ^= counter_byte;
    }
    Ok(nonce)
}

#[cfg(any(not(target_arch = "wasm32"), test))]
pub fn seal(data: &[u8], key: &[u8; 16], nonce: &[u8; 12], aad: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes128Gcm::new_from_slice(key).map_err(|_| anyhow!("invalid AES-128-GCM key"))?;
    cipher
        .encrypt(Nonce::from_slice(nonce), Payload { msg: data, aad })
        .map_err(|_| anyhow!("bundle encryption failed"))
}

pub fn open(data: &[u8], key: &[u8; 16], nonce: &[u8; 12], aad: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes128Gcm::new_from_slice(key).map_err(|_| anyhow!("invalid AES-128-GCM key"))?;
    cipher
        .decrypt(Nonce::from_slice(nonce), Payload { msg: data, aad })
        .map_err(|_| anyhow!("bundle authentication failed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, offset: u64, stored_len: u64) -> Entry {
        Entry {
            path: path.into(),
            file: Some(FileEntry {
                offset,
                stored_len,
                original_len: stored_len,
                compressed: false,
            }),
        }
    }

    #[test]
    fn rejects_invalid_header_bounds_and_flags() {
        let header = Header::new(4, false, [0; 12]);
        let bytes = header.encode();
        assert_eq!(
            Header::decode(&bytes, false, 31).unwrap().payload_offset(),
            31
        );
        assert!(Header::decode(&bytes, false, 30).is_err());
        assert!(Header::decode(&bytes, true, 31).is_err());
        assert!(Header::decode(&bytes[..26], false, 31).is_err());
        for position in [0, 5, 6] {
            let mut invalid = bytes;
            invalid[position] = 0xff;
            assert!(Header::decode(&invalid, false, 31).is_err());
        }
        assert!(
            Header::decode(
                &Header::new(u64::MAX, false, [0; 12]).encode(),
                false,
                u64::MAX
            )
            .is_err()
        );
        let encrypted = Header::new(20, true, [1; 12]).encode();
        assert!(Header::decode(&encrypted, false, 47).is_err());
        assert!(Header::decode(&encrypted, true, 47).is_ok());
    }

    #[test]
    fn rejects_unsafe_duplicate_and_conflicting_paths() {
        for path in [
            "/root",
            "../escape",
            "a/../b",
            "a//b",
            "a/",
            "C:/drive",
            "a\\b",
            "./a",
        ] {
            assert!(encode_index(&[file(path, 0, 0)]).is_err(), "{path}");
        }
        for entries in [
            vec![file("a", 0, 0), file("a", 0, 0)],
            vec![file("a/b", 0, 0), file("a", 0, 0)],
        ] {
            assert!(decode_index(&encode_index(&entries).unwrap(), 0).is_err());
        }
    }

    #[test]
    fn rejects_invalid_payload_ranges_and_truncation() {
        for entries in [
            vec![file("a", 2, 3)],
            vec![file("a", u64::MAX, 1)],
            vec![file("a", 0, 3), file("b", 2, 1)],
        ] {
            assert!(decode_index(&encode_index(&entries).unwrap(), 4).is_err());
        }
        let bytes = encode_index(&[file("a", 0, 3)]).unwrap();
        for len in 0..bytes.len() {
            assert!(decode_index(&bytes[..len], 3).is_err());
        }
        assert!(decode_index(&bytes, 3).is_ok());
    }

    #[test]
    fn authenticates_index_and_entry_metadata() {
        let key = [7; 16];
        let nonce = [3; 12];
        let entry_nonce = nonce_for_entry(&nonce, 0).unwrap();
        assert_ne!(nonce, entry_nonce);
        assert_ne!(entry_nonce, nonce_for_entry(&nonce, 1).unwrap());
        let data = seal(b"asset", &key, &entry_nonce, b"path").unwrap();
        assert_eq!(open(&data, &key, &entry_nonce, b"path").unwrap(), b"asset");
        assert!(open(&data, &key, &entry_nonce, b"other path").is_err());
        assert!(open(&data, &key, &nonce, b"path").is_err());
    }
}
