use std::{
    collections::{BTreeSet, HashMap},
    fs,
    io::{Cursor, Read},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use aes_gcm::{
    Aes128Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use anyhow::{Context, Result, bail, ensure};
use flate2::read::ZlibDecoder;

use crate::bundler::{FLAG_COMPRESSED, FLAG_ENCRYPTED, HEADER_LEN, MAGIC, VERSION};

#[derive(Debug, Default)]
pub struct AssetBundle {
    files: HashMap<PathBuf, Arc<[u8]>>,
    directories: BTreeSet<PathBuf>,
    children: HashMap<PathBuf, Vec<PathBuf>>,
}

impl AssetBundle {
    /// Loads a bundle from its encoded bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the header, encryption key, authentication,
    /// compressed payload, or archive contents are invalid.
    pub fn from_bytes(bytes: &[u8], key: Option<[u8; 16]>) -> Result<Self> {
        ensure!(bytes.len() >= HEADER_LEN, "truncated asset bundle header");
        ensure!(&bytes[..MAGIC.len()] == MAGIC, "invalid asset bundle magic");
        ensure!(
            bytes[MAGIC.len()] == VERSION,
            "unsupported asset bundle version {}",
            bytes[MAGIC.len()]
        );
        let flags = bytes[MAGIC.len() + 1];
        ensure!(
            flags & !(FLAG_COMPRESSED | FLAG_ENCRYPTED) == 0,
            "unknown bundle flags"
        );
        ensure!(
            key.is_some() == (flags & FLAG_ENCRYPTED != 0),
            "bundle encryption key presence does not match header"
        );

        let mut data = bytes[HEADER_LEN..].to_vec();
        if let Some(key) = key {
            let cipher = Aes128Gcm::new_from_slice(&key)
                .map_err(|_| anyhow::anyhow!("invalid AES-128-GCM key"))?;
            data = cipher
                .decrypt(
                    Nonce::from_slice(&bytes[MAGIC.len() + 2..HEADER_LEN]),
                    Payload {
                        msg: &data,
                        aad: &bytes[..HEADER_LEN],
                    },
                )
                .map_err(|_| {
                    anyhow::anyhow!(
                        "asset bundle authentication failed (wrong key or modified data)"
                    )
                })?;
        }
        if flags & FLAG_COMPRESSED != 0 {
            let mut unpacked = Vec::new();
            ZlibDecoder::new(data.as_slice())
                .read_to_end(&mut unpacked)
                .context("invalid compressed asset bundle")?;
            data = unpacked;
        }

        let mut bundle = Self::default();
        bundle.directories.insert(PathBuf::new());
        let mut tar = tar::Archive::new(Cursor::new(data));
        for item in tar.entries().context("invalid TAR archive")? {
            let mut item = item?;
            let path = clean_path(item.path()?.as_ref())?;
            if path.as_os_str().is_empty() {
                continue;
            }
            if item.header().entry_type().is_dir() {
                bundle.directories.insert(path.clone());
            } else if item.header().entry_type().is_file() {
                let mut content = Vec::new();
                item.read_to_end(&mut content)
                    .with_context(|| format!("read asset {}", path.display()))?;
                ensure!(
                    bundle.files.insert(path.clone(), content.into()).is_none(),
                    "duplicate asset: {}",
                    path.display()
                );
            } else {
                bail!("unsupported entry in asset bundle: {}", path.display());
            }
            let mut parent = path.parent();
            while let Some(p) = parent {
                bundle.directories.insert(p.to_path_buf());
                parent = p.parent();
            }
        }

        let mut children: HashMap<PathBuf, BTreeSet<PathBuf>> = HashMap::new();
        for path in bundle.files.keys().chain(bundle.directories.iter()) {
            if path.as_os_str().is_empty() {
                continue;
            }
            if is_listable(path) {
                let parent = path.parent().unwrap_or_else(|| Path::new(""));
                children
                    .entry(parent.to_path_buf())
                    .or_default()
                    .insert(path.clone());
            }
        }
        bundle.children = children
            .into_iter()
            .map(|(path, entries)| (path, entries.into_iter().collect()))
            .collect();
        Ok(bundle)
    }

    /// Reads and loads a bundle from a file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or
    /// [`Self::from_bytes`] rejects the bundle.
    pub fn from_file(path: impl AsRef<Path>, key: Option<[u8; 16]>) -> Result<Self> {
        let path = path.as_ref();
        Self::from_bytes(
            &fs::read(path).with_context(|| format!("read {}", path.display()))?,
            key,
        )
    }

    pub fn get(&self, path: impl AsRef<Path>) -> Option<&[u8]> {
        let path = clean_path(path.as_ref()).ok()?;
        self.files.get(&path).map(std::convert::AsRef::as_ref)
    }

    #[cfg(feature = "runtime")]
    pub(crate) fn is_directory(&self, path: &Path) -> Option<bool> {
        let path = clean_path(path).ok()?;
        if self.directories.contains(&path) {
            Some(true)
        } else if self.files.contains_key(&path) {
            Some(false)
        } else {
            None
        }
    }

    #[cfg(feature = "runtime")]
    pub(crate) fn read_directory(&self, path: &Path) -> Option<Vec<PathBuf>> {
        let path = clean_path(path).ok()?;
        self.directories
            .contains(&path)
            .then(|| self.children.get(&path).cloned().unwrap_or_default())
    }
}

fn is_listable(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    if name.starts_with('.') {
        return false;
    }
    !path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("meta"))
}

fn clean_path(path: &Path) -> Result<PathBuf> {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => clean.push(segment),
            Component::CurDir => {}
            _ => bail!(
                "asset path must be relative and cannot contain '..': {}",
                path.display()
            ),
        }
    }
    Ok(clean)
}
