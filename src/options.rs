/// Build options for the custom v1 archive format.
///
/// When an encryption key is supplied, the entire compressed TAR stream,
/// including filenames, is protected with AES-128-GCM. The key must also be
/// supplied to [`crate::AssetBundle::from_bytes`] when loading.
#[derive(Clone)]
pub struct AssetBundlingOptions {
    /// Deflate-compress the TAR payload before encryption (enabled by default).
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
