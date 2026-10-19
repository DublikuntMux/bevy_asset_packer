use std::{
    future::ready,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::asset::io::{
    AssetReader, AssetReaderError, AssetReaderFuture, PathStream, Reader, SliceReader,
};
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

    fn read_path<'a>(&'a self, path: &'a Path) -> Result<SliceReader<'a>, AssetReaderError> {
        let bytes = self
            .bundle
            .get(path)
            .ok_or_else(|| AssetReaderError::NotFound(path.to_path_buf()))?;
        Ok(SliceReader::new(bytes))
    }
}

impl AssetReader for BundledAssetReader {
    fn read<'a>(&'a self, path: &'a Path) -> impl AssetReaderFuture<Value: Reader + 'a> {
        ready(self.read_path(path))
    }

    fn read_meta<'a>(&'a self, path: &'a Path) -> impl AssetReaderFuture<Value: Reader + 'a> {
        let mut meta_name = path.as_os_str().to_os_string();
        meta_name.push(".meta");
        let meta_path = PathBuf::from(meta_name);
        ready(
            self.bundle
                .get(&meta_path)
                .map(SliceReader::new)
                .ok_or(AssetReaderError::NotFound(meta_path)),
        )
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
