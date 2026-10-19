use std::{
    future::ready,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::asset::io::{AssetReader, AssetReaderError, PathStream, Reader, VecReader};
use futures_lite::stream;

use crate::AssetBundle;

#[derive(Clone)]
pub struct BundledAssetReader {
    bundle: Arc<AssetBundle>,
}

impl BundledAssetReader {
    #[must_use]
    pub const fn new(bundle: Arc<AssetBundle>) -> Self {
        Self { bundle }
    }

    async fn read_path(&self, path: &Path) -> Result<VecReader, AssetReaderError> {
        let bytes = self
            .bundle
            .read(path)
            .await
            .map_err(|error| std::io::Error::other(format!("{error:#}")))?
            .ok_or_else(|| AssetReaderError::NotFound(path.to_path_buf()))?;
        Ok(VecReader::new(bytes))
    }
}

impl AssetReader for BundledAssetReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        self.read_path(path).await
    }

    async fn read_meta<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        let mut meta_name = path.as_os_str().to_os_string();
        meta_name.push(".meta");
        let meta_path = PathBuf::from(meta_name);
        self.read_path(&meta_path).await
    }

    fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = Result<Box<PathStream>, AssetReaderError>> {
        ready(
            self.bundle
                .read_directory(path)
                .map(|entries| Box::new(stream::iter(entries)) as Box<PathStream>)
                .ok_or_else(|| AssetReaderError::NotFound(path.to_path_buf())),
        )
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = Result<bool, AssetReaderError>> {
        ready(
            self.bundle
                .is_directory(path)
                .ok_or_else(|| AssetReaderError::NotFound(path.to_path_buf())),
        )
    }
}
