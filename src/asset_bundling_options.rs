use std::path::{Path, PathBuf};

use magic_crypt::{new_magic_crypt, MagicCrypt256, MagicCryptTrait};

use crate::{compress::compress, decompress::decompress};

#[derive(Debug, Clone)]
pub struct AssetBundlingOptions {
    pub encode_file_names: bool,
    pub encryption_on: bool,
    pub encryption_key: Option<String>,
    pub compress_on: bool,
    pub asset_bundle_name: String,
}

impl Default for AssetBundlingOptions {
    fn default() -> Self {
        Self {
            encode_file_names: false,
            encryption_on: false,
            encryption_key: None,
            compress_on: false,
            asset_bundle_name: "assets.bin".to_owned(),
        }
    }
}

impl AssetBundlingOptions {
    pub fn set_encryption_key(&mut self, key: String) -> &mut Self {
        self.encryption_on = true;
        self.encryption_key = Some(key);
        self
    }

    pub fn is_encryption_ready(&self) -> bool {
        self.encryption_on && self.encryption_key.is_some()
    }

    pub fn try_get_crypter_if_needed(&self) -> anyhow::Result<Option<MagicCrypt256>> {
        if self.encryption_on {
            if let Some(aes_key) = &self.encryption_key {
                return Ok(Some(new_magic_crypt!(aes_key, 256)));
            }
        }
        Ok(None)
    }

    pub fn try_encrypt(&self, plain: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
        if let Some(crypter) = self.try_get_crypter_if_needed()? {
            return Ok(Some(crypter.encrypt_to_bytes(plain)));
        }
        Ok(None)
    }

    pub fn try_decrypt(&self, encrypted: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
        if let Some(crypter) = self.try_get_crypter_if_needed()? {
            return Ok(Some(crypter.decrypt_bytes_to_bytes(encrypted)?));
        }
        Ok(None)
    }

    fn try_encode_string(&self, s: &str) -> anyhow::Result<String> {
        if self.is_encryption_ready() {
            let mut bytes = s.as_bytes();
            let compressed: Vec<u8>;

            if self.compress_on {
                compressed = compress(bytes);
                bytes = &compressed;
            }
            if let Some(encrypted) = self.try_encrypt(bytes)? {
                return Ok(bs58::encode(encrypted).into_string());
            }
        }

        Ok(bs58::encode(s).into_string())
    }

    fn try_decode_string(&self, s: &str) -> anyhow::Result<String> {
        let vec = bs58::decode(s).into_vec()?;
        if self.is_encryption_ready() {
            if let Some(decrypted) = self.try_decrypt(&vec)? {
                if self.compress_on {
                    let decompressed = decompress(&decrypted);
                    return Ok(String::from_utf8(decompressed.unwrap_or_default())?);
                }
                return Ok(String::from_utf8(decrypted)?);
            }
        }

        Ok(String::from_utf8(vec)?)
    }

    pub fn try_encode_path(&self, p: &Path) -> anyhow::Result<PathBuf> {
        Ok(p.to_str()
            .unwrap()
            .replace('\\', "/")
            .split('/')
            .map(|part| self.try_encode_string(part).unwrap())
            .collect())
    }

    pub fn try_decode_path(&self, p: &Path) -> anyhow::Result<PathBuf> {
        Ok(p.to_str()
            .unwrap()
            .replace('\\', "/")
            .split('/')
            .map(|part| self.try_decode_string(part).unwrap())
            .collect())
    }
}
