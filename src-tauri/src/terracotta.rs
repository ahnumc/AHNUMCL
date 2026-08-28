use crate::launcher_config::models::LauncherConfig;
use crate::tasks::download::DownloadParam;
use crate::tasks::download::DownloadTask;
use crate::tasks::monitor::TaskMonitor;
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use sjmcl_types::error::{SJMCLError, SJMCLResult};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;
use tar::Archive;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_http::reqwest;

const TERRACOTTA_REPO: &str = "burningtnt/Terracotta";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerracottaState {
  pub installed: bool,
  pub installed_version: Option<String>,
  pub latest_version: Option<String>,
  pub update_available: bool,
  pub running: bool,
  pub status: String,
  pub room_code: Option<String>,
  pub server_address: Option<String>,
  pub players: Vec<TerracottaPlayer>,
  pub download_progress: Option<u8>,
  pub download_stage: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TerracottaApiState {
  state: String,
  #[serde(default)]
  room: Option<String>,
  #[serde(default)]
  url: Option<String>,
  #[serde(default)]
  profiles: Vec<TerracottaApiProfile>,
}

#[derive(Debug, Deserialize)]
struct TerracottaApiProfile {
  #[serde(default)]
  machine_id: String,
  #[serde(default)]
  name: String,
  #[serde(default)]
  kind: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerracottaPlayer {
  pub machine_id: String,
  pub name: String,
  pub kind: String,
}

struct Runtime {
  child: Option<Child>,
  port: Option<u16>,
}

#[derive(Clone, Default)]
struct DownloadStatus {
  progress: Option<u8>,
  stage: Option<String>,
}

static RUNTIME: LazyLock<Mutex<Runtime>> = LazyLock::new(|| {
  Mutex::new(Runtime {
    child: None,
    port: None,
  })
});

static DOWNLOAD_STATUS: LazyLock<Mutex<DownloadStatus>> =
  LazyLock::new(|| Mutex::new(DownloadStatus::default()));

fn set_download_status(progress: Option<u8>, stage: Option<&str>) -> SJMCLResult<()> {
  let mut status = DOWNLOAD_STATUS
    .lock()
    .map_err(|e| SJMCLError(e.to_string()))?;
  status.progress = progress;
  status.stage = stage.map(str::to_string);
  Ok(())
}

struct DownloadStatusGuard;

impl Drop for DownloadStatusGuard {
  fn drop(&mut self) {
    if let Ok(mut status) = DOWNLOAD_STATUS.lock() {
      status.progress = None;
      status.stage = None;
    }
  }
}

fn platform_key() -> &'static str {
  match (std::env::consts::OS, std::env::consts::ARCH) {
    ("windows", "x86_64") => "windows-x86_64",
    ("windows", "aarch64") => "windows-arm64",
    ("linux", "x86_64") => "linux-x86_64",
    ("linux", "aarch64") => "linux-arm64",
    ("macos", "x86_64") => "macos-x86_64",
    ("macos", "aarch64") => "macos-arm64",
    _ => "unsupported",
  }
}

fn binary_name() -> &'static str {
  if cfg!(windows) {
    "terracotta.exe"
  } else {
    "terracotta"
  }
}

fn terracotta_dir(app: &AppHandle) -> SJMCLResult<PathBuf> {
  Ok(app.path().app_data_dir()?.join("terracotta"))
}

fn binary_path(app: &AppHandle) -> SJMCLResult<PathBuf> {
  Ok(terracotta_dir(app)?.join(binary_name()))
}

fn version_path(app: &AppHandle) -> SJMCLResult<PathBuf> {
  Ok(terracotta_dir(app)?.join("version"))
}

fn installed_version(app: &AppHandle) -> Option<String> {
  std::fs::read_to_string(version_path(app).ok()?)
    .ok()
    .filter(|v| !v.trim().is_empty())
}

async fn latest_version(app: &AppHandle, client: &reqwest::Client) -> SJMCLResult<String> {
  #[derive(Deserialize)]
  struct Release {
    tag_name: String,
  }
  let github_url = tauri::Url::parse(&format!(
    "https://api.github.com/repos/{TERRACOTTA_REPO}/releases/latest"
  ))
  .map_err(|e| SJMCLError(e.to_string()))?;
  let github_url = crate::tasks::download::DownloadTask::resolve_github_url(app, &github_url).await;
  let release = client
    .get(github_url)
    .header("User-Agent", "AHNUMCL")
    .send()
    .await
    .map_err(|e| SJMCLError(e.to_string()))?
    .error_for_status()
    .map_err(|e| SJMCLError(e.to_string()))?
    .json::<Release>()
    .await
    .map_err(|e| SJMCLError(e.to_string()))?;
  Ok(release.tag_name.trim_start_matches('v').to_string())
}

#[tauri::command]
pub async fn terracotta_get_state(
  app: AppHandle,
  client: State<'_, reqwest::Client>,
) -> SJMCLResult<TerracottaState> {
  let installed = binary_path(&app)?.is_file();
  let installed_version = installed_version(&app);
  let latest = latest_version(&app, client.inner()).await.ok();
  let (running, port) = {
    let mut runtime = RUNTIME.lock().map_err(|e| SJMCLError(e.to_string()))?;
    let child_running = match runtime.child.as_mut() {
      Some(child) => match child.try_wait() {
        Ok(Some(_)) => false,
        Ok(None) => true,
        Err(_) => false,
      },
      None => false,
    };
    if !child_running {
      runtime.child = None;
    }
    (runtime.port.is_some() || child_running, runtime.port)
  };

  let download_status = DOWNLOAD_STATUS
    .lock()
    .map_err(|e| SJMCLError(e.to_string()))?
    .clone();

  let mut api_state = None;
  if running {
    if let Some(port) = port {
      let state_client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| SJMCLError(e.to_string()))?;
      if let Ok(response) = state_client
        .get(format!("http://127.0.0.1:{port}/state"))
        .send()
        .await
        && response.status().is_success()
      {
        api_state = response.json::<TerracottaApiState>().await.ok();
      }
    }
  }

  Ok(TerracottaState {
    installed,
    installed_version: installed_version.clone(),
    update_available: latest
      .as_ref()
      .is_some_and(|v| installed_version.as_deref() != Some(v)),
    latest_version: latest,
    running,
    status: api_state
      .as_ref()
      .map(|state| state.state.clone())
      .unwrap_or_else(|| if running { "running" } else { "idle" }.to_string()),
    room_code: api_state.as_ref().and_then(|state| state.room.clone()),
    server_address: api_state.as_ref().and_then(|state| state.url.clone()),
    players: api_state
      .as_ref()
      .map(|state| {
        state
          .profiles
          .iter()
          .map(|profile| TerracottaPlayer {
            machine_id: profile.machine_id.clone(),
            name: profile.name.clone(),
            kind: profile.kind.clone(),
          })
          .collect()
      })
      .unwrap_or_default(),
    download_progress: download_status.progress,
    download_stage: download_status.stage,
    ..Default::default()
  })
}

#[tauri::command]
pub async fn terracotta_download(
  app: AppHandle,
  client: State<'_, reqwest::Client>,
  version: Option<String>,
) -> SJMCLResult<()> {
  if RUNTIME
    .lock()
    .map_err(|e| SJMCLError(e.to_string()))?
    .child
    .is_some()
  {
    return Err(SJMCLError("Terracotta is running".into()));
  }
  set_download_status(Some(0), Some("downloading"))?;
  let _download_status_guard = DownloadStatusGuard;
  let version = match version {
    Some(v) if !v.trim().is_empty() => v,
    _ => latest_version(&app, client.inner()).await?,
  };
  if platform_key() == "unsupported" {
    return Err(SJMCLError("Unsupported platform".into()));
  }
  let artifact = format!("terracotta-{version}-{}-pkg.tar.gz", platform_key());
  let source = tauri::Url::parse(&format!(
    "https://github.com/{TERRACOTTA_REPO}/releases/download/v{version}/{artifact}"
  ))
  .map_err(|e| SJMCLError(e.to_string()))?;
  let cache_dir = crate::launcher_config::commands::retrieve_launcher_config(app.clone())?
    .download
    .cache
    .directory;
  let archive_name = format!("terracotta-{version}.tar.gz");
  let archive_path = cache_dir.join(&archive_name);
  let monitor = app.state::<std::pin::Pin<Box<TaskMonitor>>>();
  let task = DownloadTask::new(
    app.clone(),
    monitor.get_new_id(),
    Some("terracotta".to_string()),
    DownloadParam {
      src: source,
      dest: PathBuf::from(&archive_name),
      filename: Some(archive_name.clone()),
      sha1: None,
    },
    Duration::from_secs(1),
    false,
  );
  let (download_future, handle) = task
    .future(app.clone(), monitor.download_rate_limiter())
    .await?;
  let mut download_task = tokio::spawn(download_future);
  loop {
    tokio::select! {
      result = &mut download_task => {
        result.map_err(|e| SJMCLError(e.to_string()))??;
        break;
      }
      _ = tokio::time::sleep(Duration::from_millis(200)) => {
        let desc = handle.read().map_err(|e| SJMCLError(e.to_string()))?.desc.clone();
        if desc.total > 0 {
          set_download_status(Some(((desc.current.max(0) * 100 / desc.total).min(100)) as u8), Some("downloading"))?;
        }
      }
    }
  }
  set_download_status(Some(100), Some("extracting"))?;
  let dir = terracotta_dir(&app)?;
  std::fs::create_dir_all(&dir)?;
  let staging = dir.join(".staging");
  if staging.exists() {
    std::fs::remove_dir_all(&staging)?;
  }
  std::fs::create_dir_all(&staging)?;
  let data_vec = std::fs::read(&archive_path)?;
  let _ = std::fs::remove_file(&archive_path);
  let staging_clone = staging.clone();
  tokio::task::spawn_blocking(move || -> SJMCLResult<()> {
    let decoder = GzDecoder::new(data_vec.as_slice());
    Archive::new(decoder)
      .unpack(staging_clone)
      .map_err(|e| SJMCLError(e.to_string()))
  })
  .await
  .map_err(|e| SJMCLError(e.to_string()))??;
  let candidate = find_binary(&staging)
    .ok_or_else(|| SJMCLError("Terracotta binary not found in archive".into()))?;
  let destination = binary_path(&app)?;
  set_download_status(Some(100), Some("installing"))?;
  std::fs::copy(candidate, &destination)?;
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o755))?;
  }
  std::fs::write(version_path(&app)?, version.as_bytes())?;
  std::fs::remove_dir_all(staging)?;
  set_download_status(None, None)?;
  Ok(())
}

fn find_binary(dir: &Path) -> Option<PathBuf> {
  for entry in walkdir::WalkDir::new(dir).into_iter().flatten() {
    if !entry.file_type().is_file() {
      continue;
    }
    let name = entry.file_name().to_string_lossy();
    if name == binary_name() && is_terracotta_executable(entry.path()) {
      return Some(entry.path().to_path_buf());
    }
  }
  None
}

fn is_terracotta_executable(path: &Path) -> bool {
  let Ok(bytes) = std::fs::read(path) else {
    return false;
  };
  if cfg!(windows) {
    bytes.get(0..2) == Some(b"MZ")
  } else {
    bytes.get(0..4) == Some(b"\x7fELF")
      || bytes.get(0..4) == Some(&[0xcf, 0xfa, 0xed, 0xfe])
      || bytes.get(0..4) == Some(&[0xfe, 0xed, 0xfa, 0xcf])
  }
}

#[tauri::command]
pub async fn terracotta_update(
  app: AppHandle,
  client: State<'_, reqwest::Client>,
) -> SJMCLResult<()> {
  let version = latest_version(&app, client.inner()).await?;
  terracotta_download(app, client, Some(version)).await
}

#[tauri::command]
pub async fn terracotta_start(app: AppHandle) -> SJMCLResult<()> {
  let path = binary_path(&app)?;
  if !path.is_file() {
    return Err(SJMCLError("Terracotta is not installed".into()));
  }
  let existing_port = RUNTIME.lock().map_err(|e| SJMCLError(e.to_string()))?.port;
  if let Some(port) = existing_port {
    let client = reqwest::Client::builder()
      .no_proxy()
      .timeout(Duration::from_secs(2))
      .build()
      .map_err(|e| SJMCLError(e.to_string()))?;
    if client
      .get(format!("http://127.0.0.1:{port}/state"))
      .send()
      .await
      .is_ok_and(|response| response.status().is_success())
    {
      return Ok(());
    }
    let mut runtime = RUNTIME.lock().map_err(|e| SJMCLError(e.to_string()))?;
    runtime.port = None;
    runtime.child = None;
  }
  {
    let mut runtime = RUNTIME.lock().map_err(|e| SJMCLError(e.to_string()))?;
    let child_running = match runtime.child.as_mut() {
      Some(child) => matches!(child.try_wait(), Ok(None)),
      None => false,
    };
    if child_running {
      return Ok(());
    }
    runtime.child = None;
    runtime.port = None;
  }
  let port_file =
    std::env::temp_dir().join(format!("ahnumcl-terracotta-{}.json", std::process::id()));
  let _ = std::fs::remove_file(&port_file);
  let child = Command::new(path)
    .arg("--hmcl")
    .arg(&port_file)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .map_err(|e| SJMCLError(e.to_string()))?;
  RUNTIME.lock().map_err(|e| SJMCLError(e.to_string()))?.child = Some(child);
  for _ in 0..100 {
    if let Ok(text) = std::fs::read_to_string(&port_file)
      && let Ok(info) = serde_json::from_str::<serde_json::Value>(&text)
      && let Some(port) = info.get("port").and_then(|v| v.as_u64())
    {
      RUNTIME.lock().map_err(|e| SJMCLError(e.to_string()))?.port = Some(port as u16);
      let _ = std::fs::remove_file(port_file);
      return Ok(());
    }
    let child_exited = RUNTIME
      .lock()
      .ok()
      .and_then(|mut runtime| {
        runtime
          .child
          .as_mut()
          .and_then(|child| child.try_wait().ok())
      })
      .is_some_and(|status| status.is_some());
    if child_exited {
      break;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
  }
  if let Ok(mut runtime) = RUNTIME.lock() {
    if let Some(mut child) = runtime.child.take() {
      let _ = child.kill();
      let _ = child.wait();
    }
    runtime.port = None;
  }
  Err(SJMCLError("Terracotta startup timed out".into()))
}

async fn terracotta_request(app: &AppHandle, path: &str) -> SJMCLResult<()> {
  let port = RUNTIME
    .lock()
    .map_err(|e| SJMCLError(e.to_string()))?
    .port
    .ok_or_else(|| SJMCLError("Terracotta is not running".into()))?;
  let client = reqwest::Client::builder()
    .no_proxy()
    .timeout(Duration::from_secs(10))
    .build()
    .map_err(|e| SJMCLError(e.to_string()))?;
  let config = app.state::<std::sync::Mutex<LauncherConfig>>();
  let nodes = config
    .lock()
    .map_err(|e| SJMCLError(e.to_string()))?
    .terracotta_public_nodes
    .clone();
  let mut unique_nodes = Vec::new();
  for node in nodes
    .iter()
    .map(|node| node.trim())
    .filter(|node| !node.is_empty())
  {
    if !unique_nodes.iter().any(|existing: &&str| existing == &node) {
      unique_nodes.push(node);
    }
  }
  if unique_nodes.is_empty() {
    unique_nodes.push("wss://center.node.1tmc.top");
  }
  let mut url = format!("http://127.0.0.1:{port}{path}");
  let mut has_query = path.contains('?');
  for node in unique_nodes {
    url.push(if has_query { '&' } else { '?' });
    url.push_str("public_nodes=");
    url.push_str(&urlencoding::encode(node.trim()));
    has_query = true;
  }
  let request_url = url.clone();
  let response = client
    .get(url)
    .send()
    .await
    .map_err(|e| SJMCLError(e.to_string()))?;
  if !response.status().is_success() {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    return Err(SJMCLError(format!(
      "HTTP status {status} for Terracotta request {request_url}: {body}"
    )));
  }
  let _ = app;
  Ok(())
}

#[tauri::command]
pub async fn terracotta_host(
  app: AppHandle,
  room_code: Option<String>,
  player_name: String,
) -> SJMCLResult<()> {
  let room = room_code.unwrap_or_default().trim().to_string();
  let room = if room.is_empty() {
    String::new()
  } else {
    normalize_room_code(&room)?
  };
  terracotta_request(
    &app,
    &format!(
      "/state/scanning?room={}&player={}",
      urlencoding::encode(&room),
      urlencoding::encode(player_name.trim())
    ),
  )
  .await
}

#[tauri::command]
pub async fn terracotta_join(
  app: AppHandle,
  room_code: String,
  player_name: String,
) -> SJMCLResult<()> {
  let room_code = normalize_room_code(room_code.trim())?;
  terracotta_request(
    &app,
    &format!(
      "/state/guesting?room={}&player={}",
      urlencoding::encode(&room_code),
      urlencoding::encode(player_name.trim())
    ),
  )
  .await
}

#[tauri::command]
pub async fn terracotta_close_room() -> SJMCLResult<()> {
  let port = RUNTIME
    .lock()
    .map_err(|e| SJMCLError(e.to_string()))?
    .port
    .ok_or_else(|| SJMCLError("Terracotta is not running".into()))?;
  let client = reqwest::Client::builder()
    .no_proxy()
    .timeout(Duration::from_secs(5))
    .build()
    .map_err(|e| SJMCLError(e.to_string()))?;
  let response = client
    .get(format!("http://127.0.0.1:{port}/state/ide"))
    .send()
    .await
    .map_err(|e| SJMCLError(e.to_string()))?;
  if !response.status().is_success() {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    return Err(SJMCLError(format!(
      "HTTP status {status} for Terracotta room close: {body}"
    )));
  }
  Ok(())
}

fn normalize_room_code(code: &str) -> SJMCLResult<String> {
  let trimmed = code.trim();
  let Some(inner) = trimmed
    .strip_prefix("U/")
    .or_else(|| trimmed.strip_prefix("u/"))
  else {
    return Err(SJMCLError(
      "Invalid room code. Expected format: U/XXXX-XXXX-XXXX-XXXX".into(),
    ));
  };
  let segments: Vec<&str> = inner.split('-').collect();
  if segments.len() != 4
    || segments.iter().any(|segment| segment.len() != 4)
    || segments.iter().any(|segment| {
      !segment
        .chars()
        .all(|character| character.is_ascii_alphanumeric())
    })
  {
    return Err(SJMCLError(
      "Invalid room code. Expected format: U/XXXX-XXXX-XXXX-XXXX".into(),
    ));
  }
  Ok(format!("U/{}", inner.to_ascii_uppercase()))
}

#[tauri::command]
pub async fn terracotta_stop() -> SJMCLResult<()> {
  let port = RUNTIME.lock().map_err(|e| SJMCLError(e.to_string()))?.port;
  if let Some(port) = port {
    let client = reqwest::Client::builder()
      .no_proxy()
      .timeout(Duration::from_secs(3))
      .build()
      .map_err(|e| SJMCLError(e.to_string()))?;
    let _ = client
      .get(format!("http://127.0.0.1:{port}/panic?peaceful=true"))
      .send()
      .await;
  }
  let mut runtime = RUNTIME.lock().map_err(|e| SJMCLError(e.to_string()))?;
  if let Some(mut child) = runtime.child.take() {
    let _ = child.kill();
    let _ = child.wait();
  }
  runtime.port = None;
  Ok(())
}
