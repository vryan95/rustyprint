//! The server's configuration, from environment variables.

use std::net::SocketAddr;
use std::num::NonZero;
use std::path::PathBuf;

use rustyprint::{Limits, RenderOptions};

/// What the server listens on and what it lets requests do.
///
/// | Variable | Default | |
/// | --- | --- | --- |
/// | `RUSTYPRINT_ADDR` | `0.0.0.0:8080` | Address to listen on |
/// | `RUSTYPRINT_CONCURRENCY` | the number of CPUs | Renders that run at once; others wait |
/// | `RUSTYPRINT_MAX_BODY_BYTES` | 52428800 (50 MB) | Largest request body |
/// | `RUSTYPRINT_ALLOWED_HOSTS` | none | Hosts requests may fetch from, comma-separated |
/// | `RUSTYPRINT_ALLOW_HTTP` | `false` | Whether requests may fetch plain `http` URLs |
/// | `RUSTYPRINT_ASSETS_DIR` | none | Directory relative URLs resolve against |
/// | `RUSTYPRINT_LIMITS` | the defaults | The highest limits, as the `limits` JSON option |
#[derive(Debug, Clone)]
pub struct Config {
    pub addr: SocketAddr,
    pub concurrency: usize,
    pub max_body_bytes: usize,
    pub allowed_hosts: Vec<String>,
    pub allow_http: bool,
    pub assets_dir: Option<PathBuf>,
    pub limits: Limits,
}

impl Config {
    /// The configuration from the process's environment.
    pub fn from_env() -> Result<Self, String> {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    /// The configuration from variables that `get` looks up by name.
    pub fn from_vars(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let var = |name: &str| {
            get(name)
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let invalid = |name: &str, value: &str, why: &str| format!("{name}=`{value}`: {why}");

        let addr = match var("RUSTYPRINT_ADDR") {
            Some(value) => value.parse().map_err(|_| {
                invalid(
                    "RUSTYPRINT_ADDR",
                    &value,
                    "not an address such as 0.0.0.0:8080",
                )
            })?,
            None => SocketAddr::from(([0, 0, 0, 0], 8080)),
        };
        let concurrency = match var("RUSTYPRINT_CONCURRENCY") {
            Some(value) => value
                .parse::<NonZero<usize>>()
                .map_err(|_| invalid("RUSTYPRINT_CONCURRENCY", &value, "not a number above 0"))?
                .get(),
            None => std::thread::available_parallelism().map_or(1, NonZero::get),
        };
        let max_body_bytes = match var("RUSTYPRINT_MAX_BODY_BYTES") {
            Some(value) => value.parse().map_err(|_| {
                invalid("RUSTYPRINT_MAX_BODY_BYTES", &value, "not a number of bytes")
            })?,
            None => 50 * 1024 * 1024,
        };
        let allowed_hosts = var("RUSTYPRINT_ALLOWED_HOSTS")
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|host| !host.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let allow_http = match var("RUSTYPRINT_ALLOW_HTTP").as_deref() {
            None | Some("false" | "0") => false,
            Some("true" | "1") => true,
            Some(value) => {
                return Err(invalid("RUSTYPRINT_ALLOW_HTTP", value, "not true or false"));
            }
        };
        let assets_dir = var("RUSTYPRINT_ASSETS_DIR").map(PathBuf::from);
        let limits = match var("RUSTYPRINT_LIMITS") {
            Some(value) => {
                RenderOptions::from_json(&format!(r#"{{"limits": {value}}}"#))
                    .map_err(|e| invalid("RUSTYPRINT_LIMITS", &value, &e.to_string()))?
                    .limits
            }
            None => Limits::default(),
        };
        Ok(Self {
            addr,
            concurrency,
            max_body_bytes,
            allowed_hosts,
            allow_http,
            assets_dir,
            limits,
        })
    }
}
