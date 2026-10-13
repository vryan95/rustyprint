//! Fetching images and stylesheets over the network, from hosts the caller allows.
//!
//! Rendering never reaches the network unless [`crate::Assets::remote`] says so. Then only
//! `https` URLs (and `http` ones if allowed) on the allowed hosts are fetched, every redirect
//! must stay on those hosts, and each fetch is bounded in time and size.

use std::time::Duration;

use ureq::Agent;
use ureq::http::Uri;

/// Which URLs may be fetched while rendering, and the limits on fetching them.
#[derive(Debug, Clone, PartialEq)]
pub struct Remote {
    /// Hosts that URLs may be fetched from, such as `cdn.example.com`, or `*.example.com`
    /// for all of its subdomains. Redirects must lead to these hosts too.
    pub allowed_hosts: Vec<String>,
    /// Whether `http` URLs may be fetched as well as `https` ones.
    pub allow_http: bool,
    /// How long one fetch may take, redirects included.
    pub timeout: Duration,
    /// The largest file that is loaded, in bytes.
    pub max_bytes: u64,
    /// How many redirects a fetch follows.
    pub max_redirects: u32,
    /// How many URLs one render fetches at most; the rest are not loaded.
    pub max_fetches: usize,
}

impl Remote {
    /// Fetching from `hosts`, over `https`, with default limits: 10 seconds and 10 MB per
    /// file, 5 redirects, and 100 files per render.
    pub fn allowing(hosts: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            allowed_hosts: hosts.into_iter().map(Into::into).collect(),
            allow_http: false,
            timeout: Duration::from_secs(10),
            max_bytes: 10 * 1024 * 1024,
            max_redirects: 5,
            max_fetches: 100,
        }
    }

    /// Whether `host` is one of the allowed hosts.
    pub fn allows_host(&self, host: &str) -> bool {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        self.allowed_hosts.iter().any(|allowed| {
            let allowed = allowed.trim().trim_end_matches('.').to_ascii_lowercase();
            match allowed.strip_prefix("*.") {
                Some(domain) => host
                    .strip_suffix(domain)
                    .is_some_and(|sub| sub.len() > 1 && sub.ends_with('.')),
                None => host == allowed,
            }
        })
    }

    /// Checks that `uri` may be fetched.
    fn check(&self, uri: &Uri) -> Result<(), String> {
        match uri.scheme_str() {
            Some("https") => {}
            Some("http") if self.allow_http => {}
            Some(scheme) => return Err(format!("the `{scheme}` scheme is not allowed")),
            None => return Err("the URL has no scheme".into()),
        }
        let host = uri.host().ok_or("the URL has no host")?;
        if self.allows_host(host) {
            Ok(())
        } else {
            Err(format!("the host `{host}` is not allowed"))
        }
    }

    /// Fetches `url`, following redirects that stay on the allowed hosts.
    pub(crate) fn fetch(&self, url: &str) -> Result<Vec<u8>, String> {
        let agent: Agent = Agent::config_builder()
            .timeout_global(Some(self.timeout))
            .https_only(!self.allow_http)
            .http_status_as_error(false)
            // Redirects are followed here, to check where each one leads.
            .max_redirects(0)
            .build()
            .into();
        let mut url = url.to_owned();
        for _ in 0..=self.max_redirects {
            let uri: Uri = url.parse().map_err(|_| "the URL is not valid".to_owned())?;
            self.check(&uri)?;
            let response = agent.get(&url).call().map_err(|e| e.to_string())?;
            let status = response.status();
            if status.is_redirection() {
                let location = response
                    .headers()
                    .get("location")
                    .and_then(|value| value.to_str().ok())
                    .ok_or("a redirect has no location")?;
                url = resolve(&uri, location)?;
                continue;
            }
            if !status.is_success() {
                return Err(format!("the server answered {status}"));
            }
            return response
                .into_body()
                .with_config()
                .limit(self.max_bytes)
                .read_to_vec()
                .map_err(|e| e.to_string());
        }
        Err("too many redirects".into())
    }
}

/// Whether a URL refers to the network rather than to an asset.
pub(crate) fn is_remote(url: &str) -> bool {
    let url = url.trim();
    let scheme = |s: &str| {
        url.get(..s.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(s))
    };
    scheme("http://") || scheme("https://") || url.starts_with("//")
}

/// The absolute URL of a remote URL: scheme-relative ones (`//host/path`) use `https`.
pub(crate) fn absolute(url: &str) -> String {
    let url = url.trim();
    match url.strip_prefix("//") {
        Some(rest) => format!("https://{rest}"),
        None => url.to_owned(),
    }
}

/// The URL a redirect's `Location` leads to from `base`.
fn resolve(base: &Uri, location: &str) -> Result<String, String> {
    let location = location.trim();
    if is_remote(location) {
        return Ok(absolute(location));
    }
    let scheme = base.scheme_str().unwrap_or("https");
    let authority = base.authority().ok_or("the URL has no host")?;
    if location.starts_with('/') {
        return Ok(format!("{scheme}://{authority}{location}"));
    }
    // A path relative to the current one.
    let path = base.path();
    let directory = &path[..path.rfind('/').map_or(0, |i| i + 1)];
    Ok(format!("{scheme}://{authority}{directory}{location}"))
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    use super::*;

    #[test]
    fn hosts_match_exactly_or_by_wildcard() {
        let remote = Remote::allowing(["cdn.example.com", "*.blob.core.windows.net"]);
        assert!(remote.allows_host("cdn.example.com"));
        assert!(remote.allows_host("CDN.Example.com."));
        assert!(!remote.allows_host("example.com"));
        assert!(!remote.allows_host("evilcdn.example.com"));
        assert!(remote.allows_host("account.blob.core.windows.net"));
        assert!(!remote.allows_host("blob.core.windows.net"));
        assert!(!remote.allows_host("xblob.core.windows.net"));
    }

    #[test]
    fn only_allowed_schemes_and_hosts_are_fetched() {
        let remote = Remote::allowing(["cdn.example.com"]);
        let check = |url: &str| remote.check(&url.parse().unwrap());
        assert!(check("https://cdn.example.com/a.png").is_ok());
        assert!(check("http://cdn.example.com/a.png").is_err());
        assert!(check("https://other.example.com/a.png").is_err());
        assert!(check("ftp://cdn.example.com/a.png").is_err());
    }

    #[test]
    fn remote_urls_are_recognized() {
        assert!(is_remote("https://a/b.png"));
        assert!(is_remote(" HTTP://a/b.png"));
        assert!(is_remote("//a/b.png"));
        assert!(!is_remote("img/b.png"));
        assert!(!is_remote("data:,x"));
        assert_eq!(absolute("//a/b.png"), "https://a/b.png");
    }

    #[test]
    fn redirects_resolve() {
        let base: Uri = "https://a.com/x/y.png?q".parse().unwrap();
        assert_eq!(
            resolve(&base, "https://b.com/z").unwrap(),
            "https://b.com/z"
        );
        assert_eq!(resolve(&base, "/z").unwrap(), "https://a.com/z");
        assert_eq!(resolve(&base, "z.png").unwrap(), "https://a.com/x/z.png");
    }

    /// Serves `responses` in order to the connections it accepts, and returns its address.
    fn serve(responses: Vec<String>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        std::thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                    line.clear();
                }
                let _ = stream.write_all(response.as_bytes());
            }
        });
        addr
    }

    fn local() -> Remote {
        Remote {
            allow_http: true,
            ..Remote::allowing(["127.0.0.1"])
        }
    }

    #[test]
    fn fetches_follow_allowed_redirects() {
        // Each response says the connection closes, as `serve` closes it: a client that kept it
        // for the redirect could find it closed.
        let addr = serve(vec![
            "HTTP/1.1 302 Found\r\nLocation: /image\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .into(),
            "HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello".into(),
        ]);
        assert_eq!(
            local().fetch(&format!("http://{addr}/redirect")).unwrap(),
            b"hello"
        );
    }

    #[test]
    fn redirects_to_other_hosts_are_refused() {
        let addr = serve(vec![
            "HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/\r\nContent-Length: 0\r\n\
             Connection: close\r\n\r\n"
                .into(),
        ]);
        let error = local().fetch(&format!("http://{addr}/")).unwrap_err();
        assert!(error.contains("169.254.169.254"), "{error}");
    }

    #[test]
    fn large_files_and_errors_are_refused() {
        let addr = serve(vec![
            "HTTP/1.1 200 OK\r\nContent-Length: 20\r\nConnection: close\r\n\r\n01234567890123456789"
                .into(),
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
        ]);
        let small = Remote {
            max_bytes: 10,
            ..local()
        };
        assert!(small.fetch(&format!("http://{addr}/big")).is_err());
        let error = local()
            .fetch(&format!("http://{addr}/missing"))
            .unwrap_err();
        assert!(error.contains("404"), "{error}");
    }
}
