use std::{
    borrow::Borrow,
    collections::HashMap,
    env,
    fs::File,
    io::prelude::*,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use bevy::{
    asset::{AssetIo, AssetIoError, ChangeWatcher},
    utils::BoxedFuture,
};
use miniz_oxide::inflate::decompress_to_vec;
use tar::Archive;

use crate::{asset_bundling_options::AssetBundlingOptions, path_info::ArchivePathInfo};

type ParentDirToPathInfo = HashMap<String, Vec<ArchivePathInfo>>;

#[derive(Default)]
pub struct BundledAssetIo {
    options: AssetBundlingOptions,
    parent_dir_to_path_info: Option<Arc<RwLock<ParentDirToPathInfo>>>,
}

impl From<AssetBundlingOptions> for BundledAssetIo {
    fn from(options: AssetBundlingOptions) -> Self {
        Self {
            options,
            parent_dir_to_path_info: None,
        }
    }
}

impl BundledAssetIo {
    pub fn ensure_loaded(&mut self) -> anyhow::Result<()> {
        if self.parent_dir_to_path_info.is_none() {
            let bundle_path = self.get_bundle_path()?;
            let file = File::open(bundle_path)?;
            let mut archive = Archive::new(file);
            let mut mappings: ParentDirToPathInfo = HashMap::new();
            for entry in archive.entries()?.flatten() {
                let path = entry.path()?;
                let decoded_path = if self.options.encode_file_names {
                    self.options.try_decode_path(path.borrow())?
                } else {
                    path.to_path_buf()
                };
                let mut parent_dir = decoded_path.clone();
                let parent_dir_str = if parent_dir.pop() {
                    normalize_path(&parent_dir)
                } else {
                    "".into()
                };
                let path_info = ArchivePathInfo::new(decoded_path);
                if let Some(vec) = mappings.get_mut(&parent_dir_str) {
                    vec.push(path_info);
                } else {
                    mappings.insert(parent_dir_str, vec![path_info]);
                }
            }
            self.parent_dir_to_path_info = Some(Arc::new(RwLock::new(mappings)));
            Ok(())
        } else {
            Err(anyhow::Error::msg("Entity file is not found"))
        }
    }

    fn get_bundle_path(&self) -> anyhow::Result<PathBuf, AssetIoError> {
        let mut bundle_path = env::current_exe().map_err(AssetIoError::Io)?;
        bundle_path.pop();
        bundle_path.push(self.options.asset_bundle_name.clone());
        Ok(bundle_path)
    }
}

impl AssetIo for BundledAssetIo {
    fn load_path<'a>(&'a self, path: &'a Path) -> BoxedFuture<'a, Result<Vec<u8>, AssetIoError>> {
        Box::pin(async move {
            let bundle_path = self.get_bundle_path()?;
            let file = File::open(bundle_path)?;
            let encoded_entry_path = if self.options.encode_file_names {
                self.options.try_encode_path(path).map_err(map_error)?
            } else {
                PathBuf::from(normalize_path(path))
            };
            let mut archive = Archive::new(file);
            for mut entry in archive.entries()?.flatten() {
                let entry_path = entry.path()?;
                if entry_path.eq(&encoded_entry_path) {
                    let mut vec = Vec::new();
                    entry.read_to_end(&mut vec)?;

                    if let Some(decrypted) = self.options.try_decrypt(&vec).map_err(map_error)? {
                        if self.options.compress_on {
                            let decompressed = decompress_to_vec(&decrypted).unwrap_or_default();
                            return Ok(decompressed);
                        }
                        return Ok(decrypted);
                    }
                    if self.options.compress_on {
                        let decompressed = decompress_to_vec(&vec).unwrap_or_default();
                        return Ok(decompressed);
                    }

                    return Ok(vec);
                }
            }
            Err(AssetIoError::NotFound(path.to_path_buf()))
        })
    }

    fn read_directory(&self, path: &Path) -> Result<Box<dyn Iterator<Item = PathBuf>>, AssetIoError> {
        if let Some(lock) = self.parent_dir_to_path_info.clone() {
            let mappings = lock.read().unwrap();
            let path_str = normalize_path(path);
            if let Some(entries) = mappings.get(&path_str) {
                let vec: Vec<_> = entries.iter().map(|e| e.path()).collect();
                return Ok(Box::new(vec.into_iter()));
            }
        }
        Err(AssetIoError::NotFound(path.to_path_buf()))
    }

    fn watch_path_for_changes(&self, _to_watch: &Path, _to_reload: Option<PathBuf>) -> Result<(), AssetIoError> {
        Ok(())
    }

    fn watch_for_changes(&self, _configuration: &ChangeWatcher) -> Result<(), AssetIoError> {
        Ok(())
    }

    fn get_metadata(&self, path: &Path) -> Result<bevy::asset::Metadata, AssetIoError> {
        if let Some(lock) = self.parent_dir_to_path_info.clone() {
            let mappings = lock.read().unwrap();
            let path_str = normalize_path(path);
            if mappings.contains_key(&path_str) {
                Ok(bevy::asset::Metadata::new(bevy::asset::FileType::Directory))
            } else {
                for v in mappings.values() {
                    for info in v {
                        if info.path() == path {
                            return Ok(bevy::asset::Metadata::new(bevy::asset::FileType::File));
                        }
                    }
                }
                Err(AssetIoError::NotFound(path.to_path_buf()))
            }
        } else {
            Err(AssetIoError::NotFound(path.to_path_buf()))
        }
    }
}

fn map_error(err: anyhow::Error) -> AssetIoError {
    AssetIoError::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("{}", err)))
}

fn normalize_path(path: &Path) -> String {
    path.to_str().unwrap_or("").replace('\\', "/")
}
