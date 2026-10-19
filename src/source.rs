use std::{borrow::Cow, ops::Range};

#[cfg(not(target_arch = "wasm32"))]
use std::{
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
};

#[cfg(any(target_arch = "wasm32", test))]
use anyhow::ensure;
use anyhow::{Context, Result};

#[derive(Debug)]
pub enum BundleSource {
    Memory(Cow<'static, [u8]>),
    #[cfg(not(target_arch = "wasm32"))]
    File(PathBuf),
    #[cfg(target_arch = "wasm32")]
    Http {
        url: String,
        total_len: u64,
    },
}

impl BundleSource {
    pub async fn read_range(&self, range: Range<u64>) -> Result<Vec<u8>> {
        let len = usize::try_from(
            range
                .end
                .checked_sub(range.start)
                .context("invalid byte range")?,
        )?;
        if len == 0 {
            return Ok(Vec::new());
        }
        match self {
            Self::Memory(bytes) => {
                let start = usize::try_from(range.start)?;
                let end = usize::try_from(range.end)?;
                Ok(bytes
                    .get(start..end)
                    .context("truncated asset payload")?
                    .to_vec())
            }
            #[cfg(not(target_arch = "wasm32"))]
            Self::File(path) => {
                let path = path.clone();
                blocking::unblock(move || {
                    let mut file = std::fs::File::open(&path)
                        .with_context(|| format!("open {}", path.display()))?;
                    file.seek(SeekFrom::Start(range.start))?;
                    let mut bytes = vec![0; len];
                    file.read_exact(&mut bytes)
                        .context("truncated asset payload")?;
                    Ok(bytes)
                })
                .await
            }
            #[cfg(target_arch = "wasm32")]
            Self::Http { url, total_len } => {
                let (bytes, actual_total) = read_http_range(url, range).await?;
                ensure!(
                    actual_total == *total_len,
                    "bundle changed since its index was loaded"
                );
                Ok(bytes)
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn read_http_range(url: &str, range: Range<u64>) -> Result<(Vec<u8>, u64)> {
    use js_sys::Uint8Array;
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Headers, RequestInit, Response};

    fn js_error(value: &JsValue) -> anyhow::Error {
        anyhow::anyhow!("browser request failed: {value:?}")
    }

    let end = range.end.checked_sub(1).context("empty HTTP byte range")?;
    ensure!(range.start <= end, "invalid HTTP byte range");
    let window = web_sys::window().context("HTTP bundles require a browser window")?;
    let headers = Headers::new().map_err(|error| js_error(&error))?;
    headers
        .set("Range", &format!("bytes={}-{end}", range.start))
        .map_err(|error| js_error(&error))?;
    let options = RequestInit::new();
    options.set_headers_headers(&headers);
    let response = JsFuture::from(window.fetch_with_str_and_init(url, &options))
        .await
        .map_err(|error| js_error(&error))?
        .dyn_into::<Response>()
        .map_err(|error| js_error(&error))?;

    // A server that ignores Range could otherwise download the entire pack.
    let validation = (|| {
        ensure!(
            response.status() == 206,
            "bundle server must return HTTP 206 for byte ranges (received {})",
            response.status()
        );
        let headers = response.headers();
        let encoding = headers
            .get("Content-Encoding")
            .map_err(|error| js_error(&error))?;
        ensure!(
            encoding.as_deref().is_none_or(|value| value == "identity"),
            "serve .pack files without HTTP content encoding"
        );
        let content_range = headers
            .get("Content-Range")
            .map_err(|error| js_error(&error))?
            .context("missing Content-Range; cross-origin servers must expose this header")?;
        let total_len = validate_content_range(&content_range, &range)?;
        if let Some(length) = headers
            .get("Content-Length")
            .map_err(|error| js_error(&error))?
        {
            ensure!(
                length.parse::<u64>()? == range.end - range.start,
                "incorrect HTTP range length"
            );
        }
        Ok(total_len)
    })();
    let total_len = match validation {
        Ok(total_len) => total_len,
        Err(error) => {
            if let Some(body) = response.body() {
                let _ = body.cancel();
            }
            return Err(error);
        }
    };
    let buffer = JsFuture::from(response.array_buffer().map_err(|error| js_error(&error))?)
        .await
        .map_err(|error| js_error(&error))?;
    let bytes = Uint8Array::new(&buffer).to_vec();
    ensure!(
        u64::try_from(bytes.len())? == range.end - range.start,
        "truncated HTTP range response"
    );
    Ok((bytes, total_len))
}

#[cfg(any(target_arch = "wasm32", test))]
fn validate_content_range(value: &str, requested: &Range<u64>) -> Result<u64> {
    let value = value
        .strip_prefix("bytes ")
        .context("invalid Content-Range unit")?;
    let (bounds, total) = value.split_once('/').context("invalid Content-Range")?;
    let (start, end) = bounds
        .split_once('-')
        .context("invalid Content-Range bounds")?;
    let start = start.parse::<u64>()?;
    let end = end.parse::<u64>()?;
    let total = total.parse::<u64>()?;
    ensure!(
        start == requested.start && end.checked_add(1) == Some(requested.end),
        "server returned the wrong byte range"
    );
    ensure!(start <= end && end < total, "invalid Content-Range size");
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::validate_content_range;

    #[test]
    fn validates_exact_http_ranges() {
        assert_eq!(
            validate_content_range("bytes 10-19/100", &(10..20)).unwrap(),
            100
        );
        for header in [
            "bytes 0-19/100",
            "bytes 10-20/100",
            "bytes 10-19/19",
            "bytes 10-19/*",
            "items 10-19/100",
            "bytes 10-18446744073709551615/100",
        ] {
            assert!(
                validate_content_range(header, &(10..20)).is_err(),
                "{header}"
            );
        }
    }
}
