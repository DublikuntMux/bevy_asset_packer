use std::{fs, io::Write, path::PathBuf};

use aes_gcm::{
    Aes128Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use anyhow::{Context, Result, bail};
use flate2::{Compression, write::ZlibEncoder};
use walkdir::WalkDir;

use crate::AssetBundlingOptions;

pub const MAGIC: &[u8; 5] = b"BPACK";
pub const VERSION: u8 = 1;
pub const HEADER_LEN: usize = MAGIC.len() + 1 + 1 + 12;
pub const FLAG_COMPRESSED: u8 = 1;
pub const FLAG_ENCRYPTED: u8 = 2;

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

    /// Builds the encoded bundle in memory.
    ///
    /// # Errors
    ///
    /// Returns an error if the source directory is missing, an entry cannot
    /// be read or is unsupported (including symlinks), or archiving,
    /// compression, or encryption fails.
    pub fn build_bytes(&self) -> Result<Vec<u8>> {
        if !self.source_dir.is_dir() {
            bail!("assets directory not found: {}", self.source_dir.display());
        }

        let mut tar = tar::Builder::new(Vec::new());
        for item in WalkDir::new(&self.source_dir)
            .follow_links(false)
            .sort_by_file_name()
        {
            let item = item?;
            let source = item.path();
            let relative = source.strip_prefix(&self.source_dir)?;
            if relative.as_os_str().is_empty() {
                continue;
            }
            if item.file_type().is_dir() {
                tar.append_dir(relative, source)
                    .with_context(|| format!("append directory {}", relative.display()))?;
            } else if item.file_type().is_file() {
                tar.append_path_with_name(source, relative)
                    .with_context(|| format!("append asset {}", relative.display()))?;
            } else {
                bail!(
                    "unsupported asset entry: {} (symlinks are not allowed)",
                    source.display()
                );
            }
        }
        let mut data = tar.into_inner()?;
        let mut flags = 0u8;
        if self.options.compress {
            flags |= FLAG_COMPRESSED;
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&data)?;
            data = encoder.finish()?;
        }

        let mut header = Vec::with_capacity(HEADER_LEN);
        header.extend_from_slice(MAGIC);
        header.push(VERSION);
        if self.options.encryption_key.is_some() {
            flags |= FLAG_ENCRYPTED;
        }
        header.push(flags);

        // Random nonces must never be reused with the same AES-GCM key. The
        // full header is authenticated as associated data.
        let nonce: [u8; 12] = if self.options.encryption_key.is_some() {
            rand::random()
        } else {
            [0; 12]
        };
        header.extend_from_slice(&nonce);
        if let Some(key) = self.options.encryption_key {
            let cipher = Aes128Gcm::new_from_slice(&key)
                .map_err(|_| anyhow::anyhow!("invalid AES-128-GCM key"))?;
            data = cipher
                .encrypt(
                    Nonce::from_slice(&nonce),
                    Payload {
                        msg: &data,
                        aad: &header,
                    },
                )
                .map_err(|_| anyhow::anyhow!("bundle encryption failed"))?;
        }
        header.extend_from_slice(&data);
        Ok(header)
    }
}
