use std::{fs, io::Write, path::PathBuf};

use anyhow::{Context, Result, bail, ensure};
use flate2::{Compression, write::ZlibEncoder};
use walkdir::WalkDir;

use crate::{
    AssetBundlingOptions,
    format::{Entry, FileEntry, Header, TAG_LEN, encode_index, nonce_for_entry, seal},
};

#[derive(Debug)]
pub struct AssetBundler {
    pub source_dir: PathBuf,
    pub output_file: PathBuf,
    pub options: AssetBundlingOptions,
}

impl AssetBundler {
    pub fn new(
        source_dir: impl Into<PathBuf>,
        output_file: impl Into<PathBuf>,
        options: AssetBundlingOptions,
    ) -> Self {
        Self {
            source_dir: source_dir.into(),
            output_file: output_file.into(),
            options,
        }
    }

    /// Builds the bundle and writes it to the output file.
    ///
    /// # Errors
    ///
    /// Returns an error if [`Self::build_bytes`] fails, the output directory
    /// cannot be created, or the output file cannot be written.
    pub fn build(&self) -> Result<()> {
        let bundle = self.build_bytes()?;
        if let Some(parent) = self
            .output_file
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        fs::write(&self.output_file, bundle)
            .with_context(|| format!("write {}", self.output_file.display()))?;
        Ok(())
    }

    /// Builds the indexed bundle in memory, compressing each file independently.
    ///
    /// # Errors
    ///
    /// Returns an error if the source directory is missing, an entry cannot
    /// be read or is unsupported (including symlinks and non-UTF-8 paths), or
    /// compression, encryption, or index encoding fails.
    pub fn build_bytes(&self) -> Result<Vec<u8>> {
        ensure!(
            fs::symlink_metadata(&self.source_dir).is_ok_and(|metadata| metadata.is_dir()),
            "assets directory not found or unsupported: {}",
            self.source_dir.display()
        );
        let encrypted = self.options.encryption_key.is_some();
        let nonce = if encrypted { rand::random() } else { [0; 12] };
        let mut entries = Vec::new();
        let mut payload = Vec::new();
        for item in WalkDir::new(&self.source_dir)
            .follow_links(false)
            .sort_by_file_name()
            .min_depth(1)
        {
            let item = item?;
            let source = item.path();
            let relative = source.strip_prefix(&self.source_dir)?;
            // A slash-separated index keeps bundles portable across platforms.
            let path = relative
                .components()
                .map(|component| {
                    component
                        .as_os_str()
                        .to_str()
                        .context("asset paths must be UTF-8")
                })
                .collect::<Result<Vec<_>>>()?
                .join("/");
            let mut entry = Entry {
                path: path.into(),
                file: None,
            };
            if item.file_type().is_file() {
                let original = fs::read(source)
                    .with_context(|| format!("read asset {}", relative.display()))?;
                let original_len = u64::try_from(original.len())?;
                let (mut stored, compressed) = self.compress_file(original)?;
                let stored_len = u64::try_from(stored.len())?
                    .checked_add(if encrypted { TAG_LEN } else { 0 })
                    .context("asset length overflow")?;
                let offset = u64::try_from(payload.len())?;
                offset
                    .checked_add(stored_len)
                    .context("bundle payload length overflow")?;
                entry.file = Some(FileEntry {
                    offset,
                    stored_len,
                    original_len,
                    compressed,
                });
                let aad = entry.associated_data()?;
                if let Some(key) = self.options.encryption_key {
                    stored = seal(
                        &stored,
                        &key,
                        &nonce_for_entry(&nonce, entries.len())?,
                        &aad,
                    )?;
                }
                payload.extend_from_slice(&stored);
            } else if !item.file_type().is_dir() {
                bail!(
                    "unsupported asset entry: {} (symlinks are not allowed)",
                    source.display()
                );
            }
            entries.push(entry);
        }
        let mut index = encode_index(&entries)?;
        let index_len = u64::try_from(index.len())?
            .checked_add(if encrypted { TAG_LEN } else { 0 })
            .context("bundle index length overflow")?;
        let header = Header::new(index_len, encrypted, nonce);
        header
            .payload_offset()
            .checked_add(u64::try_from(payload.len())?)
            .context("bundle length overflow")?;
        let mut bundle = header.encode().to_vec();
        if let Some(key) = self.options.encryption_key {
            index = seal(&index, &key, &nonce, &bundle)?;
        }
        bundle.extend_from_slice(&index);
        bundle.extend_from_slice(&payload);
        Ok(bundle)
    }

    fn compress_file(&self, original: Vec<u8>) -> Result<(Vec<u8>, bool)> {
        if self.options.compress {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&original)?;
            let compressed = encoder.finish()?;
            if compressed.len() < original.len() {
                return Ok((compressed, true));
            }
        }
        Ok((original, false))
    }
}
