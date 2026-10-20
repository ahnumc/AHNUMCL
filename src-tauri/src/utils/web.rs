use reqwest_middleware::{ClientBuilder as ClientWithMiddlewareBuilder, ClientWithMiddleware};
use reqwest_retry::RetryTransientMiddleware;
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::{
  Retryable, RetryableStrategy, default_on_request_failure, default_on_request_success,
};
use std::sync::Mutex;
use std::time::Duration;
use tauri::http::StatusCode;
use tauri::{AppHandle, Manager};
use tauri_plugin_http::reqwest::header::HeaderMap;
use tauri_plugin_http::reqwest::{Client, ClientBuilder, Proxy};
use url::Url;

use crate::launcher_config::models::{LauncherConfig, ProxyType};

#[cfg(target_os = "windows")]
fn get_windows_system_http_proxy() -> Option<String> {
  use winreg::RegKey;
  use winreg::enums::HKEY_CURRENT_USER;

  let key = RegKey::predef(HKEY_CURRENT_USER)
    .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
    .ok()?;
  let enabled: u32 = key.get_value("ProxyEnable").ok()?;
  if enabled == 0 {
    return None;
  }
  let server: String = key.get_value("ProxyServer").ok()?;
  let address = server
    .split(';')
    .find_map(|entry| entry.strip_prefix("http="))
    .or_else(|| (!server.contains('=')).then_some(server.as_str()))?
    .trim();
  if address.is_empty() {
    None
  } else if address.starts_with("http://") {
    Some(address.to_string())
  } else {
    Some(format!("http://{address}"))
  }
}

#[cfg(not(target_os = "windows"))]
fn get_windows_system_http_proxy() -> Option<String> {
  None
}

/// Builds a reqwest client with AHNUMCL User-Agent and proxy support.
/// Defaults to a 10s connection timeout and a 60s read timeout.
///
/// # Arguments
///
/// * `app` - The Tauri AppHandle.
/// * `use_proxy` - Whether to use the proxy settings from the config.
///
/// TODO: support more custom config from reqwest::Config
/// FIXME: Seems like hyper will panic if this client is shared across threads.
///
/// # Returns
///
/// A reqwest::Client instance.
///
/// # Example
///
/// ```rust,ignore
/// let client = build_sjmcl_client(&app, true);
/// ```
pub fn build_sjmcl_client(app: &AppHandle, use_proxy: bool) -> Client {
  let mut builder = ClientBuilder::new()
    .connect_timeout(Duration::from_secs(10))
    .read_timeout(Duration::from_secs(60))
    .tcp_keepalive(Duration::from_secs(30));

  if let Ok(config) = app.state::<Mutex<LauncherConfig>>().lock() {
    // According to the User-Agent requirements of mozilla and BMCLAPI, the User-Agent is set to start with ${NAME}/${VERSION}
    // https://github.com/MCLF-CN/docs/issues/2
    // https://developer.mozilla.org/zh-CN/docs/Web/HTTP/Reference/Headers/User-Agent
    if let Ok(header_value) = format!("AHNUMCL/{}", &config.basic_info.launcher_version).parse() {
      let mut headers = HeaderMap::new();
      headers.insert("User-Agent", header_value);
      builder = builder.default_headers(headers);
    }

    if use_proxy {
      let proxy_url = if config.download.proxy.enabled {
        let proxy_cfg = &config.download.proxy;
        match proxy_cfg.selected_type {
          ProxyType::Http => Some(format!("http://{}:{}", proxy_cfg.host, proxy_cfg.port)),
          ProxyType::Socks => Some(format!("socks5h://{}:{}", proxy_cfg.host, proxy_cfg.port)),
        }
      } else {
        get_windows_system_http_proxy()
      };

      if let Some(proxy_url) = proxy_url
        && let Ok(proxy) = Proxy::all(&proxy_url)
      {
        builder = builder.proxy(proxy);
      }
    }
  }

  builder.build().unwrap_or_else(|_| Client::new())
}

struct SJMCLRetryableStrategy;

impl RetryableStrategy for SJMCLRetryableStrategy {
  fn handle(
    &self,
    res: &Result<tauri_plugin_http::reqwest::Response, reqwest_middleware::Error>,
  ) -> Option<reqwest_retry::Retryable> {
    match res {
      // retry if 403
      Ok(success) if success.status() == StatusCode::FORBIDDEN => Some(Retryable::Transient),
      // otherwise do not retry a successful request
      Ok(success) => default_on_request_success(success),
      // but maybe retry a request failure
      Err(error) if matches!(error.status(), Some(StatusCode::FORBIDDEN)) => {
        Some(Retryable::Transient)
      }
      Err(error) if error.is_request() => Some(Retryable::Transient),
      Err(error) => default_on_request_failure(error),
    }
  }
}

pub fn with_retry(client: Client) -> ClientWithMiddleware {
  with_retry_duration(client, Duration::from_secs(3600))
}

/// Builds a retrying client with a caller-specific retry budget.
pub fn with_retry_duration(client: Client, total_retry_duration: Duration) -> ClientWithMiddleware {
  ClientWithMiddlewareBuilder::new(client)
    .with(RetryTransientMiddleware::new_with_policy_and_strategy(
      ExponentialBackoff::builder().build_with_total_retry_duration(total_retry_duration),
      SJMCLRetryableStrategy {},
    ))
    .build()
}

/// Normalizes a URL string for semantic equality comparison, including:
/// - Lowercasing the scheme and the host (ref to RFC 3986, impl by Url::parse)
/// - Removing trailing slashes from paths (except for root `/`)
/// - Removing default ports (e.g. 80 for HTTP, 443 for HTTPS)
///
/// # Arguments
///
/// * `input` - The URL string to normalize.
///
/// # Returns
///
/// A normalized URL string suitable for direct string comparison.
/// If parsing fails, the original input string is returned unchanged.
pub fn normalize_url(input: &str) -> String {
  let url = match Url::parse(input) {
    Ok(url) if !url.cannot_be_a_base() && url.host_str().is_some() => url,
    _ => return input.to_string(),
  };

  // remove trailing slash except for root
  let mut path = url.path().to_string();
  if path != "/" {
    path = path.trim_end_matches('/').to_string();
  }

  // remove default port(e.g. 80 for HTTP, 443 for HTTPS)
  let port = match (url.port(), url.port_or_known_default()) {
    (Some(p), Some(default)) if p == default => None,
    (p, _) => p,
  };

  let mut normalized = match Url::parse(&format!("{}://{}", url.scheme(), url.host_str().unwrap()))
  {
    Ok(u) => u,
    Err(_) => return input.to_string(),
  };

  let _ = normalized.set_port(port);
  normalized.set_path(&path);
  normalized.set_query(url.query());
  normalized.set_fragment(url.fragment());

  normalized.to_string()
}
