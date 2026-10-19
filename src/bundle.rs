use std::{
    borrow::Cow,
    collections::{BTreeSet, HashMap},
    fmt,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use flate2::{Decompress, FlushDecompress, Status};

use crate::{
    format::{Entry, HEADER_LEN, Header, TAG_LEN, decode_index, nonce_for_entry, open},
    source::BundleSource,
};

/// An indexed bundle whose asset payloads are read only when requested.
pub struct AssetBundle {
    source: BundleSource,
    header: Header,
    key: Option<[u8; 16]>,
    files: HashMap<PathBuf, (usize, Entry)>,
    directories: BTreeSet<PathBuf>,
    #[cfg(feature = "runtime")]
    children: HashMap<PathBuf, Vec<PathBuf>>,
}

impl fmt::Debug for AssetBundle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AssetBundle")
            .field("files", &self.files.len())
            .field("directories", &self.directories.len())
            .field("encrypted", &self.header.encrypted)
            .finish_non_exhaustive()
    }
}

impl AssetBundle {
    /// Opens an in-memory bundle without unpacking its asset payloads.
    ///
    /// Pass an owned `Vec<u8>` or a static slice such as
    /// `include_bytes!("assets.pack").as_slice()` to avoid copying the pack.
    ///
    /// # Errors
    ///
    /// Returns an error if the header, key, or index is invalid.
    /// Asset payloads are validated later by [`Self::read`].
    pub fn from_bytes(bytes: impl Into<Cow<'static, [u8]>>, key: Option<[u8; 16]>) -> Result<Self> {
        let bytes = bytes.into();
        let total_len = u64::try_from(bytes.len())?;
        let header = Header::decode(
            bytes.get(..HEADER_LEN).context("truncated bundle header")?,
            key.is_some(),
            total_len,
        )?;
        let entries = Self::parse_index(
            &header,
            &bytes[HEADER_LEN..usize::try_from(header.payload_offset())?],
            total_len,
            key,
        )?;
        Ok(Self::new(BundleSource::Memory(bytes), header, entries, key))
    }

    /// Opens a file and reads only its header and index.
    ///
    /// The file must remain at this path and unchanged while the bundle is used.
    /// Payload reads use asynchronous file I/O.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or its header, key, or index
    /// is invalid. Asset payloads are validated later by [`Self::read`].
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_file(path: impl AsRef<Path>, key: Option<[u8; 16]>) -> Result<Self> {
        use std::fs::{self, File};
        use std::io::Read;

        let path = fs::canonicalize(path.as_ref())?;
        let mut file = File::open(&path)?;
        let total_len = file.metadata()?.len();
        let mut header_bytes = [0; HEADER_LEN];
        file.read_exact(&mut header_bytes)
            .context("truncated bundle header")?;
        let header = Header::decode(&header_bytes, key.is_some(), total_len)?;
        let mut index = vec![0; usize::try_from(header.index_len)?];
        file.read_exact(&mut index)
            .context("truncated bundle index")?;
        let entries = Self::parse_index(&header, &index, total_len, key)?;
        Ok(Self::new(BundleSource::File(path), header, entries, key))
    }

    /// Opens a browser-hosted bundle using HTTP byte range requests.
    ///
    /// Only the header and index are downloaded initially. Each [`Self::read`]
    /// fetches one asset. Use an immutable, versioned URL. The server must
    /// return HTTP 206 with an exposed `Content-Range` header and serve the
    /// pack without HTTP content encoding.
    ///
    /// # Errors
    ///
    /// Returns an error if the browser request fails, the server does not
    /// support byte ranges, or the header, key, or index is invalid.
    #[cfg(target_arch = "wasm32")]
    pub async fn from_url(url: impl Into<String>, key: Option<[u8; 16]>) -> Result<Self> {
        use crate::source::read_http_range;

        let url = url.into();
        let (header_bytes, total_len) = read_http_range(&url, 0..HEADER_LEN as u64).await?;
        let header = Header::decode(&header_bytes, key.is_some(), total_len)?;
        let source = BundleSource::Http { url, total_len };
        let index = source
            .read_range(HEADER_LEN as u64..header.payload_offset())
            .await?;
        let entries = Self::parse_index(&header, &index, total_len, key)?;
        Ok(Self::new(source, header, entries, key))
    }

    /// Reads, authenticates, and decompresses a single asset on demand.
    ///
    /// Returns `None` for missing files, directories, or unsafe paths. The
    /// returned bytes belong to the caller; the bundle retains no decoded
    /// asset cache. A complete requested file is decoded, not individual
    /// chunks within a file.
    ///
    /// # Errors
    ///
    /// Returns an error if the source cannot be read or the requested payload
    /// is truncated, fails authentication, or has invalid compressed data.
    pub async fn read(&self, path: impl AsRef<Path>) -> Result<Option<Vec<u8>>> {
        let Some(path) = clean_path(path.as_ref()) else {
            return Ok(None);
        };
        let Some((ordinal, entry)) = self.files.get(&path) else {
            return Ok(None);
        };
        let file = entry.file.as_ref().context("missing asset metadata")?;
        let start = self.header.payload_offset() + file.offset;
        let mut data = self
            .source
            .read_range(start..start + file.stored_len)
            .await
            .with_context(|| format!("read asset {}", path.display()))?;
        if let Some(key) = self.key {
            data = open(
                &data,
                &key,
                &nonce_for_entry(&self.header.nonce, *ordinal)?,
                &entry.associated_data()?,
            )
            .with_context(|| format!("authenticate asset {}", path.display()))?;
        }
        if file.compressed {
            let mut decoder = Decompress::new(true);
            let mut unpacked = Vec::new();
            // One spare byte detects streams larger than their declared size;
            // decompression writes directly into uninitialized Vec capacity.
            unpacked.try_reserve_exact(
                usize::try_from(file.original_len)?
                    .checked_add(1)
                    .context("asset size overflow")?,
            )?;
            let status = decoder
                .decompress_vec(&data, &mut unpacked, FlushDecompress::Finish)
                .context("invalid compressed asset")?;
            ensure!(
                status == Status::StreamEnd,
                "incomplete compressed asset stream"
            );
            ensure!(
                decoder.total_in() == u64::try_from(data.len())?,
                "trailing compressed asset bytes"
            );
            data = unpacked;
        }
        ensure!(
            u64::try_from(data.len())? == file.original_len,
            "asset size does not match bundle index"
        );
        Ok(Some(data))
    }

    fn parse_index(
        header: &Header,
        index: &[u8],
        total_len: u64,
        key: Option<[u8; 16]>,
    ) -> Result<Vec<Entry>> {
        let index = if let Some(key) = key {
            Cow::Owned(open(index, &key, &header.nonce, &header.encode())?)
        } else {
            Cow::Borrowed(index)
        };
        let entries = decode_index(&index, total_len - header.payload_offset())?;
        for entry in &entries {
            if let Some(file) = &entry.file {
                let tag_len = if header.encrypted { TAG_LEN } else { 0 };
                let encoded_len = file
                    .stored_len
                    .checked_sub(tag_len)
                    .context("truncated encrypted asset")?;
                ensure!(file.original_len < u64::MAX, "asset size overflow");
                usize::try_from(file.original_len).context("asset too large for this platform")?;
                usize::try_from(file.stored_len)
                    .context("stored asset too large for this platform")?;
                ensure!(
                    if file.compressed {
                        encoded_len > 0 && encoded_len < file.original_len
                    } else {
                        encoded_len == file.original_len
                    },
                    "invalid asset sizes in index"
                );
            }
        }
        Ok(entries)
    }

    fn new(
        source: BundleSource,
        header: Header,
        entries: Vec<Entry>,
        key: Option<[u8; 16]>,
    ) -> Self {
        let mut files = HashMap::new();
        let mut directories = BTreeSet::from([PathBuf::new()]);
        for (ordinal, entry) in entries.into_iter().enumerate() {
            let mut parent = entry.path.parent();
            while let Some(path) = parent {
                directories.insert(path.to_path_buf());
                parent = path.parent();
            }
            if entry.file.is_some() {
                files.insert(entry.path.clone(), (ordinal, entry));
            } else {
                directories.insert(entry.path);
            }
        }
        #[cfg(feature = "runtime")]
        let mut children: HashMap<PathBuf, BTreeSet<PathBuf>> = HashMap::new();
        #[cfg(feature = "runtime")]
        for path in files.keys().chain(directories.iter()) {
            if !path.as_os_str().is_empty() && is_listable(path) {
                children
                    .entry(path.parent().unwrap_or_else(|| Path::new("")).to_path_buf())
                    .or_default()
                    .insert(path.clone());
            }
        }
        Self {
            source,
            header,
            key,
            files,
            directories,
            #[cfg(feature = "runtime")]
            children: children
                .into_iter()
                .map(|(path, entries)| (path, entries.into_iter().collect()))
                .collect(),
        }
    }

    #[cfg(feature = "runtime")]
    pub(crate) fn is_directory(&self, path: &Path) -> Option<bool> {
        let path = clean_path(path)?;
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
        let path = clean_path(path)?;
        self.directories
            .contains(&path)
            .then(|| self.children.get(&path).cloned().unwrap_or_default())
    }
}

#[cfg(feature = "runtime")]
fn is_listable(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    !name.starts_with('.')
        && !path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("meta"))
}

fn clean_path(path: &Path) -> Option<PathBuf> {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => clean.push(segment),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(clean)
}
