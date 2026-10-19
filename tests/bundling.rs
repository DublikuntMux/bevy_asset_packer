#![cfg(not(target_arch = "wasm32"))]

use std::fs;

#[cfg(feature = "runtime")]
use std::path::Path;

use bevy_asset_packer::{AssetBundle, AssetBundler, AssetBundlingOptions};
use futures_lite::future::block_on;

const KEY: [u8; 16] = *b"sample-key-16byt";

fn sample(options: AssetBundlingOptions) -> (tempfile::TempDir, Vec<u8>) {
    let root = tempfile::tempdir().unwrap();
    let assets = root.path().join("assets");
    fs::create_dir_all(assets.join("nested/empty")).unwrap();
    fs::write(assets.join("nested/hello.txt"), b"hello world").unwrap();
    fs::write(assets.join("nested/hello.txt.meta"), b"(meta: value)").unwrap();
    fs::write(assets.join(".hidden"), b"hidden").unwrap();
    fs::write(assets.join("empty.txt"), []).unwrap();
    fs::write(assets.join("z-last.txt"), vec![b'z'; 64 * 1024]).unwrap();
    let pack = AssetBundler::new(assets, root.path().join("output/assets.pack"), options);
    pack.build().unwrap();
    let bytes = fs::read(&pack.output_file).unwrap();
    (root, bytes)
}

#[test]
fn round_trip_all_compression_and_encryption_options() {
    for compress in [false, true] {
        for encryption_key in [None, Some(KEY)] {
            let (_, bytes) = sample(AssetBundlingOptions {
                compress,
                encryption_key,
            });
            assert_eq!(&bytes[..5], b"BPACK");
            let bundle = AssetBundle::from_bytes(bytes, encryption_key).unwrap();
            block_on(async {
                assert_eq!(
                    bundle.read("nested/hello.txt").await.unwrap().unwrap(),
                    b"hello world"
                );
                assert_eq!(
                    bundle.read("nested/hello.txt.meta").await.unwrap().unwrap(),
                    b"(meta: value)"
                );
                assert_eq!(bundle.read(".hidden").await.unwrap().unwrap(), b"hidden");
                assert_eq!(bundle.read("empty.txt").await.unwrap(), Some(Vec::new()));
                assert_eq!(
                    bundle.read("z-last.txt").await.unwrap().unwrap(),
                    vec![b'z'; 64 * 1024]
                );
                for path in [
                    "missing.txt",
                    "../nested/hello.txt",
                    "/nested/hello.txt",
                    "nested/empty",
                ] {
                    assert!(bundle.read(path).await.unwrap().is_none());
                }
            });
        }
    }
}

#[test]
fn encrypted_index_rejects_missing_or_incorrect_key() {
    let (_, bytes) = sample(AssetBundlingOptions {
        compress: true,
        encryption_key: Some(KEY),
    });
    assert!(AssetBundle::from_bytes(bytes.clone(), Some([42; 16])).is_err());
    assert!(AssetBundle::from_bytes(bytes.clone(), None).is_err());
    assert!(
        !bytes
            .windows(b"nested/hello.txt".len())
            .any(|window| window == b"nested/hello.txt")
    );

    let (_, plain) = sample(AssetBundlingOptions::default());
    assert!(AssetBundle::from_bytes(plain, Some(KEY)).is_err());
}

#[test]
fn corrupted_asset_fails_only_when_requested() {
    let (_, mut bytes) = sample(AssetBundlingOptions {
        compress: true,
        encryption_key: Some(KEY),
    });
    *bytes.last_mut().unwrap() ^= 1;
    let bundle = AssetBundle::from_bytes(bytes, Some(KEY)).unwrap();
    block_on(async {
        assert_eq!(
            bundle.read("nested/hello.txt").await.unwrap().unwrap(),
            b"hello world"
        );
        assert!(bundle.read("z-last.txt").await.is_err());
    });
}

#[test]
fn external_pack_reads_only_requested_assets_and_retains_no_cache() {
    let (root, bytes) = sample(AssetBundlingOptions {
        compress: false,
        encryption_key: None,
    });
    let path = root.path().join("output/assets.pack");
    let bundle = AssetBundle::from_file(&path, None).unwrap();
    let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    // Removing the final asset after opening must not affect an earlier asset.
    file.set_len(u64::try_from(bytes.len() - 1).unwrap())
        .unwrap();
    block_on(async {
        assert_eq!(
            bundle.read("nested/hello.txt").await.unwrap().unwrap(),
            b"hello world"
        );
        assert!(bundle.read("z-last.txt").await.is_err());
        // A previously read asset must be fetched again, rather than cached.
        file.set_len(0).unwrap();
        assert!(bundle.read("nested/hello.txt").await.is_err());
    });
}

#[test]
fn invalid_headers_are_rejected() {
    let (_, bytes) = sample(AssetBundlingOptions::default());
    assert!(AssetBundle::from_bytes(bytes[..5].to_vec(), None).is_err());
    let mut invalid_magic = bytes.clone();
    invalid_magic[0] = 0;
    assert!(AssetBundle::from_bytes(invalid_magic, None).is_err());
    let mut invalid_version = bytes.clone();
    invalid_version[5] = 0xff;
    assert!(AssetBundle::from_bytes(invalid_version, None).is_err());
    let mut invalid_flags = bytes;
    invalid_flags[6] = 0xff;
    assert!(AssetBundle::from_bytes(invalid_flags, None).is_err());
}

#[cfg(feature = "runtime")]
#[test]
fn bevy_reader_reads_seeks_and_lists_assets_and_metadata() {
    use bevy::asset::io::{AssetReader, AssetReaderError, Reader as _};
    use bevy_asset_packer::BundledAssetReader;
    use futures_lite::{AsyncSeekExt, StreamExt};
    use std::{io::SeekFrom, sync::Arc};

    let (_, bytes) = sample(AssetBundlingOptions {
        compress: true,
        encryption_key: Some(KEY),
    });
    let reader =
        BundledAssetReader::new(Arc::new(AssetBundle::from_bytes(bytes, Some(KEY)).unwrap()));
    block_on(async {
        let mut stream = reader.read(Path::new("nested/hello.txt")).await.unwrap();
        let mut actual = Vec::new();
        stream.read_to_end(&mut actual).await.unwrap();
        assert_eq!(&actual, b"hello world");
        stream
            .seekable()
            .unwrap()
            .seek(SeekFrom::Start(6))
            .await
            .unwrap();
        actual.clear();
        stream.read_to_end(&mut actual).await.unwrap();
        assert_eq!(&actual, b"world");

        let mut metadata = reader
            .read_meta(Path::new("nested/hello.txt"))
            .await
            .unwrap();
        actual.clear();
        metadata.read_to_end(&mut actual).await.unwrap();
        assert_eq!(&actual, b"(meta: value)");

        assert!(
            reader
                .is_directory(Path::new("nested/empty"))
                .await
                .unwrap()
        );
        assert!(
            !reader
                .is_directory(Path::new("nested/hello.txt"))
                .await
                .unwrap()
        );
        let mut listing = reader.read_directory(Path::new("nested")).await.unwrap();
        let mut names = Vec::new();
        while let Some(path) = listing.next().await {
            names.push(path);
        }
        assert!(names.contains(&Path::new("nested/hello.txt").to_path_buf()));
        assert!(names.contains(&Path::new("nested/empty").to_path_buf()));
        assert!(!names.contains(&Path::new("nested/hello.txt.meta").to_path_buf()));
        assert!(
            reader
                .read_directory(Path::new("nested/empty"))
                .await
                .unwrap()
                .next()
                .await
                .is_none()
        );
        let mut root_listing = reader.read_directory(Path::new("")).await.unwrap();
        while let Some(path) = root_listing.next().await {
            assert_ne!(path, Path::new(".hidden"));
        }
        assert!(matches!(
            reader.read(Path::new("missing")).await,
            Err(AssetReaderError::NotFound(_))
        ));
    });
}
