/// Build options for the indexed v2 archive format.
///
/// Files are compressed independently and loaded on demand. When a key is
/// supplied, the index (including filenames) and each file are independently
/// authenticated with AES-128-GCM. Supply the same key when opening the bundle.
#[derive(Clone)]
pub struct AssetBundlingOptions {
    /// Try zlib/DEFLATE for each file, keeping compression only when it saves space.
    /// Enabled by default; compression happens before encryption.
    pub compress: bool,
    /// AES-128-GCM key. A key embedded in the game binary is not secret.
    pub encryption_key: Option<[u8; 16]>,
}

impl std::fmt::Debug for AssetBundlingOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AssetBundlingOptions")
            .field("compress", &self.compress)
            .field("encrypted", &self.encryption_key.is_some())
            .finish()
    }
}

impl Default for AssetBundlingOptions {
    fn default() -> Self {
        Self {
            compress: true,
            encryption_key: None,
        }
    }
}
