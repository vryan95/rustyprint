//! The files a document can refer to: stylesheets and images.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use crate::remote::{self, Remote};

/// Files a document can refer to by relative or root-relative URL, such as
/// `<link href="style.css">`, `url(img/logo.png)` or `/assets/app.css`, and `data:` URLs.
///
/// Nothing else is read: paths cannot leave [`Assets::directory`], and the renderer only
/// fetches from the network what [`Assets::remote`] allows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Assets {
    /// Files by the path the document uses, such as `img/logo.png`.
    pub files: BTreeMap<String, Vec<u8>>,
    /// A directory to read files that are not in [`Assets::files`] from.
    pub directory: Option<PathBuf>,
    /// Which absolute `http` and `https` URLs may be fetched; `None` fetches none.
    pub remote: Option<Remote>,
}

impl Assets {
    /// Assets read from a directory.
    pub fn directory(directory: impl Into<PathBuf>) -> Self {
        Self {
            files: BTreeMap::new(),
            directory: Some(directory.into()),
            remote: None,
        }
    }

    /// The contents of the file `url` refers to, or `None` if it is not an available asset.
    /// Remote URLs are not fetched here; see [`Loader`].
    pub fn get(&self, url: &str) -> Option<Vec<u8>> {
        let url = url.trim();
        if let Some(data) = url.strip_prefix("data:") {
            return data_url(data);
        }
        let path = asset_path(url)?;
        if let Some(bytes) = self.files.get(&path).or_else(|| self.files.get(url)) {
            return Some(bytes.clone());
        }
        let directory = self.directory.as_ref()?;
        std::fs::read(directory.join(&path)).ok()
    }
}

/// Loads the assets of one render. Remote files are fetched once each, as the remote policy
/// allows, and the reasons they could not be are kept as warnings.
pub(crate) struct Loader<'a> {
    assets: &'a Assets,
    fetched: RefCell<HashMap<String, Option<Vec<u8>>>>,
    warnings: RefCell<Vec<String>>,
}

impl<'a> Loader<'a> {
    pub fn new(assets: &'a Assets) -> Self {
        Self {
            assets,
            fetched: RefCell::new(HashMap::new()),
            warnings: RefCell::new(Vec::new()),
        }
    }

    /// The contents of the file `url` refers to: an asset, or a remote file.
    pub fn get(&self, url: &str) -> Option<Vec<u8>> {
        if !remote::is_remote(url) {
            return self.assets.get(url);
        }
        let policy = self.assets.remote.as_ref()?;
        let url = remote::absolute(url);
        if let Some(bytes) = self.fetched.borrow().get(&url) {
            return bytes.clone();
        }
        let bytes = if self.fetched.borrow().len() >= policy.max_fetches {
            self.warn(
                &url,
                &format!("more than {} files to fetch", policy.max_fetches),
            );
            None
        } else {
            policy
                .fetch(&url)
                .map_err(|reason| self.warn(&url, &reason))
                .ok()
        };
        self.fetched.borrow_mut().insert(url, bytes.clone());
        bytes
    }

    fn warn(&self, url: &str, reason: &str) {
        self.warnings
            .borrow_mut()
            .push(format!("could not fetch `{url}`: {reason}"));
    }

    /// Why remote files could not be fetched, in the order they were tried.
    pub fn warnings(&self) -> Vec<String> {
        self.warnings.borrow().clone()
    }
}

/// The asset path of a relative URL: percent-decoded, without its query, fragment and `.`
/// segments. A root-relative URL, such as `/assets/app.css` from a web app's view, starts at
/// the asset root. URLs with a scheme, protocol-relative URLs and paths with `..` have none.
fn asset_path(url: &str) -> Option<String> {
    let url = url.split(['?', '#']).next()?;
    if url.contains(':') || url.starts_with("//") || url.starts_with('\\') {
        return None;
    }
    let url = url.strip_prefix('/').unwrap_or(url);
    let decoded = percent_decode(url)?;
    let mut segments = Vec::new();
    for segment in decoded.split(['/', '\\']) {
        match segment {
            "" | "." => {}
            ".." => return None,
            segment => segments.push(segment),
        }
    }
    if segments.is_empty() {
        return None;
    }
    Some(segments.join("/"))
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = percent_decode_bytes(text.as_bytes())?;
    String::from_utf8(bytes).ok()
}

fn percent_decode_bytes(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some(out)
}

/// The contents of a `data:` URL (RFC 2397), given what follows `data:`.
fn data_url(data: &str) -> Option<Vec<u8>> {
    let (header, payload) = data.split_once(',')?;
    if header
        .rsplit(';')
        .next()
        .is_some_and(|last| last.trim().eq_ignore_ascii_case("base64"))
    {
        base64(&percent_decode_bytes(payload.as_bytes())?)
    } else {
        percent_decode_bytes(payload.as_bytes())
    }
}

/// Decodes standard or URL-safe base64, ignoring whitespace and padding.
fn base64(input: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0;
    for &byte in input {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b' ' | b'\t' | b'\n' | b'\r' => continue,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_are_normalized() {
        assert_eq!(asset_path("img/a.png").as_deref(), Some("img/a.png"));
        assert_eq!(
            asset_path("./img//a.png?v=2#x").as_deref(),
            Some("img/a.png")
        );
        assert_eq!(asset_path("my%20logo.png").as_deref(), Some("my logo.png"));
    }

    #[test]
    fn root_relative_paths_start_at_the_asset_root() {
        assert_eq!(
            asset_path("/assets/app-3f9a.css").as_deref(),
            Some("assets/app-3f9a.css")
        );
        assert_eq!(asset_path("/etc/passwd").as_deref(), Some("etc/passwd"));
    }

    #[test]
    fn paths_cannot_escape() {
        for url in [
            "../secret",
            "img/../../secret",
            "/../secret",
            "/",
            "//example.com/a.png",
            "C:\\x",
            "http://example.com/a.png",
            "file:///etc/passwd",
            "",
        ] {
            assert_eq!(asset_path(url), None, "{url}");
        }
    }

    #[test]
    fn files_are_found_by_path() {
        let mut assets = Assets::default();
        assets.files.insert("img/a.png".into(), vec![1, 2, 3]);
        assert_eq!(assets.get("./img/a.png"), Some(vec![1, 2, 3]));
        assert_eq!(assets.get("img/b.png"), None);
        assert_eq!(assets.get("/img/a.png"), Some(vec![1, 2, 3]));
    }

    #[test]
    fn data_urls_decode() {
        let assets = Assets::default();
        assert_eq!(
            assets.get("data:text/plain;base64,aGVsbG8="),
            Some(b"hello".to_vec())
        );
        assert_eq!(assets.get("data:,a%20b"), Some(b"a b".to_vec()));
        assert_eq!(assets.get("data:image/png;base64,!!"), None);
    }
}
