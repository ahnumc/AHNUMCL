use async_speed_limit::Limiter;
use futures::StreamExt;
use futures::stream::TryStreamExt;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use sjmcl_types::error::{SJMCLError, SJMCLResult};
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, Url};
use tauri_plugin_http::reqwest;
use tauri_plugin_http::reqwest::StatusCode;
use tauri_plugin_http::reqwest::header::{CONTENT_RANGE, RANGE, USER_AGENT};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio_util::bytes;
use tokio_util::compat::FuturesAsyncReadCompatExt;

use crate::launcher_config::commands::retrieve_launcher_config;
use crate::resource::helpers::curseforge::misc::{
  CURSEFORGE_BROWSER_USER_AGENT, get_curseforge_api_key, get_curseforge_download_mirror,
  get_modrinth_download_mirror, is_curseforge_authenticated_url,
};
use crate::resource::helpers::misc::get_minecraft_download_mirror;
use crate::tasks::streams::ProgressStream;
use crate::tasks::streams::desc::{PDesc, PStatus};
use crate::tasks::streams::reporter::Reporter;
use crate::tasks::*;
use crate::utils::fs::validate_sha1;
use crate::utils::web::with_retry;

const GITHUB_MIRRORS: [&str; 4] = [
  "",
  "https://ghfast.top/",
  "https://ghproxy.net/",
  "https://gh-proxy.com/",
];
static GITHUB_FASTEST: OnceLock<Mutex<Option<(Instant, String)>>> = OnceLock::new();
const DOWNLOAD_RETRY_COUNT: usize = 3;
const PARALLEL_CHUNK_THRESHOLD: i64 = 4 * 1024 * 1024;
const PARALLEL_CHUNK_COUNT: i64 = 4;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DownloadParam {
  pub src: Url,
  pub dest: PathBuf,
  pub filename: Option<String>,
  pub sha1: Option<String>,
}

pub struct DownloadTask {
  p_handle: PTaskHandle,
  param: DownloadParam,
  dest_path: PathBuf,
  report_interval: Duration,
  allow_parallel_chunks: bool,
}

impl DownloadTask {
  pub(crate) async fn resolve_github_url(app_handle: &AppHandle, url: &Url) -> Url {
    let Some(host) = url.host_str() else {
      return url.clone();
    };
    if host != "github.com" && host != "raw.githubusercontent.com" && host != "api.github.com" {
      return url.clone();
    }
    let config = match retrieve_launcher_config(app_handle.clone()) {
      Ok(config) => config.download.github,
      Err(_) => return url.clone(),
    };
    if !config.auto_select {
      return Self::apply_github_mirror(&config.mirror, url);
    }
    if config.mirror != "auto" {
      return Self::apply_github_mirror(&config.mirror, url);
    }
    let cache = GITHUB_FASTEST.get_or_init(|| Mutex::new(None));
    if let Some((at, mirror)) = cache.lock().unwrap().as_ref()
      && at.elapsed() < Duration::from_secs(600)
    {
      return Self::apply_github_mirror(mirror, url);
    }
    let client = with_retry(app_handle.state::<reqwest::Client>().inner().clone());
    let mut checks = futures::stream::FuturesUnordered::new();
    for mirror in GITHUB_MIRRORS {
      let target = Self::apply_github_mirror(mirror, url);
      let started = Instant::now();
      let client = client.clone();
      checks.push(async move {
        let result = client.get(target).send().await;
        result
          .ok()
          .filter(|response| {
            response.status().is_success() || response.status() == StatusCode::NOT_FOUND
          })
          .map(|_| (started.elapsed(), mirror.to_string()))
      });
    }
    let mut fastest: Option<(Duration, String)> = None;
    while let Some(result) = checks.next().await {
      if fastest.as_ref().is_none_or(|current| {
        result
          .as_ref()
          .is_some_and(|candidate| candidate.0 < current.0)
      }) {
        if let Some(candidate) = result {
          fastest = Some(candidate);
        }
      }
    }
    let mirror = fastest.map(|(_, mirror)| mirror).unwrap_or_default();
    *cache.lock().unwrap() = Some((Instant::now(), mirror.clone()));
    Self::apply_github_mirror(&mirror, url)
  }

  fn apply_github_mirror(mirror: &str, url: &Url) -> Url {
    if mirror.is_empty() || mirror == "auto" {
      return url.clone();
    }
    Url::parse(&format!("{mirror}{url}")).unwrap_or_else(|_| url.clone())
  }

  async fn get_download_candidates(
    app_handle: &AppHandle,
    url: &Url,
  ) -> (Vec<Url>, Option<Duration>) {
    // CurseForge can return mediafilez URLs for files which do not exist on
    // edge.forgecdn.net. Keep the API-provided URL as the official candidate.
    let official = url.clone();
    let Some(mirror) = get_curseforge_download_mirror(&official)
      .or_else(|| get_modrinth_download_mirror(&official))
      .or_else(|| get_minecraft_download_mirror(&official))
    else {
      return (vec![official], None);
    };
    let config = match retrieve_launcher_config(app_handle.clone()) {
      Ok(config) => config,
      Err(_) => return (vec![mirror, official], None),
    };

    match config.download.source.strategy.as_str() {
      "official" => (vec![official, mirror], None),
      "mirror" => (vec![mirror, official], None),
      // Race both routes in automatic mode. The mirror often redirects back to
      // ForgeCDN, so waiting for it to time out before trying the official URL
      // can multiply the total retry time.
      _ => (vec![official, mirror], Some(Duration::from_millis(750))),
    }
  }

  pub fn new(
    app_handle: AppHandle,
    task_id: u32,
    task_group: Option<String>,
    param: DownloadParam,
    report_interval: Duration,
    allow_parallel_chunks: bool,
  ) -> Self {
    let cache_dir = retrieve_launcher_config(app_handle.clone())
      .unwrap()
      .download
      .cache
      .directory;
    DownloadTask {
      p_handle: PTaskHandle::new(
        PDesc::<PTaskParam>::new(
          task_id,
          task_group.clone(),
          0,
          PTaskParam::Download(param.clone()),
          PStatus::InProgress,
        ),
        Duration::from_secs(1),
        cache_dir.clone().join(format!("task-{task_id}.json")),
        Reporter::new(
          0,
          Duration::from_secs(1),
          TauriEventSink::new(app_handle.clone()),
        ),
      ),
      param: param.clone(),
      dest_path: cache_dir.clone().join(param.dest.clone()),
      report_interval,
      allow_parallel_chunks,
    }
  }

  pub fn from_descriptor(
    app_handle: AppHandle,
    desc: PTaskDesc,
    report_interval: Duration,
    reset: bool,
  ) -> Self {
    let param = match &desc.payload {
      PTaskParam::Download(param) => param.clone(),
    };

    let cache_dir = retrieve_launcher_config(app_handle.clone())
      .unwrap()
      .download
      .cache
      .directory;
    let task_id = desc.task_id;
    let path = cache_dir.join(format!("task-{task_id}.json"));
    DownloadTask {
      p_handle: PTaskHandle::new(
        if reset {
          PTaskDesc {
            status: PStatus::Waiting,
            current: 0,
            ..desc
          }
        } else {
          PTaskDesc {
            status: PStatus::Waiting,
            ..desc
          }
        },
        Duration::from_secs(1),
        path,
        Reporter::new(
          desc.total,
          Duration::from_secs(1),
          TauriEventSink::new(app_handle.clone()),
        ),
      ),
      param: param.clone(),
      dest_path: cache_dir.clone().join(param.dest.clone()),
      report_interval,
      allow_parallel_chunks: true,
    }
  }

  async fn send_request(
    app_handle: &AppHandle,
    current: i64,
    param: &DownloadParam,
    source_offset: usize,
  ) -> SJMCLResult<reqwest::Response> {
    let state = app_handle.state::<reqwest::Client>();
    let client = state.inner().clone();
    let src = Self::resolve_github_url(app_handle, &param.src).await;
    let (mut candidates, hedge_delay) = Self::get_download_candidates(app_handle, &src).await;
    let candidate_count = candidates.len();
    if candidate_count > 1 {
      candidates.rotate_left(source_offset % candidate_count);
    }

    if let Some(delay) = hedge_delay {
      let mut requests = futures::stream::FuturesUnordered::new();
      for (index, src) in candidates.into_iter().enumerate() {
        let client = client.clone();
        let api_key = get_curseforge_api_key().map(str::to_owned);
        let wait = delay * index as u32;
        requests.push(async move {
          if !wait.is_zero() {
            tokio::time::sleep(wait).await;
          }
          let mut request = if current == 0 {
            client.get(src.clone())
          } else {
            client
              .get(src.clone())
              .header(RANGE, format!("bytes={current}-"))
          };
          if is_curseforge_authenticated_url(&src) {
            request = request.header(USER_AGENT, CURSEFORGE_BROWSER_USER_AGENT);
            if let Some(key) = api_key.as_deref() {
              request = request.header("x-api-key", key);
            }
          }
          let response = tokio::time::timeout(Duration::from_secs(30), request.send())
            .await
            .map_err(|_| SJMCLError("Download request timed out".into()))?
            .map_err(|_| SJMCLError("Download request failed".into()))?;
          response
            .error_for_status()
            .map_err(|_| SJMCLError("Download source returned an error status".into()))
        });
      }
      while let Some(result) = requests.next().await {
        if let Ok(response) = result {
          info!(
            "Download source selected (hedged): {} ({})",
            response.url(),
            response.status()
          );
          return Ok(response);
        }
      }
      return Err(SJMCLError("All download sources failed".into()));
    }

    let mut last_error = None;

    for src in candidates {
      let mut request = if current == 0 {
        client.get(src.clone())
      } else {
        client
          .get(src.clone())
          .header(RANGE, format!("bytes={current}-"))
      };

      if is_curseforge_authenticated_url(&src) {
        request = request.header(USER_AGENT, CURSEFORGE_BROWSER_USER_AGENT);
        if let Some(key) = get_curseforge_api_key() {
          request = request.header("x-api-key", key);
        }
      }

      match tokio::time::timeout(Duration::from_secs(15), request.send()).await {
        Ok(Ok(response)) if response.status().is_success() => {
          info!(
            "Download source selected: {} ({})",
            response.url(),
            response.status()
          );
          return Ok(response);
        }
        Ok(Ok(response)) => {
          last_error = Some(format!("{} returned HTTP {}", src, response.status()))
        }
        Ok(Err(error)) => last_error = Some(format!("{} failed: {}", src, error)),
        Err(_) => last_error = Some(format!("{} timed out", src)),
      }
    }

    Err(SJMCLError(format!(
      "All download sources failed: {}",
      last_error.unwrap_or_else(|| "unknown error".to_string())
    )))
  }

  async fn send_range_request(
    app_handle: &AppHandle,
    start: i64,
    end: i64,
    param: &DownloadParam,
    source_offset: usize,
  ) -> SJMCLResult<reqwest::Response> {
    let state = app_handle.state::<reqwest::Client>();
    let client = state.inner().clone();
    let src = Self::resolve_github_url(app_handle, &param.src).await;
    let (mut candidates, _) = Self::get_download_candidates(app_handle, &src).await;
    if candidates.len() > 1 {
      let candidate_count = candidates.len();
      candidates.rotate_left(source_offset % candidate_count);
    }

    let mut last_error = None;
    for src in candidates {
      let mut request = client
        .get(src.clone())
        .header(RANGE, format!("bytes={start}-{end}"));
      if is_curseforge_authenticated_url(&src) {
        request = request.header(USER_AGENT, CURSEFORGE_BROWSER_USER_AGENT);
        if let Some(key) = get_curseforge_api_key() {
          request = request.header("x-api-key", key);
        }
      }
      match tokio::time::timeout(Duration::from_secs(15), request.send()).await {
        Ok(Ok(response)) if response.status().is_success() => {
          info!(
            "Download range source selected: {} ({}, bytes={}-{})",
            response.url(),
            response.status(),
            start,
            end
          );
          return Ok(response);
        }
        Ok(Ok(response)) => last_error = Some(format!("HTTP {}", response.status())),
        Ok(Err(error)) => last_error = Some(error.to_string()),
        Err(_) => last_error = Some("request timed out".to_string()),
      }
    }
    Err(SJMCLError(format!(
      "All range sources failed: {}",
      last_error.unwrap_or_else(|| "unknown error".to_string())
    )))
  }

  async fn probe_range_size(app_handle: &AppHandle, param: &DownloadParam) -> Option<i64> {
    let response = Self::send_range_request(app_handle, 0, 0, param, 0)
      .await
      .ok()?;
    if response.status() != StatusCode::PARTIAL_CONTENT {
      return None;
    }
    response
      .headers()
      .get(CONTENT_RANGE)
      .and_then(|value| value.to_str().ok())
      .and_then(|value| value.rsplit_once('/'))
      .and_then(|(_, total)| total.parse::<i64>().ok())
  }

  async fn download_parallel_chunks(
    app_handle: &AppHandle,
    param: &DownloadParam,
    dest_path: &PathBuf,
    total: i64,
    task_handle: &Arc<RwLock<PTaskHandle>>,
  ) -> SJMCLResult<()> {
    let chunk_size = (total + PARALLEL_CHUNK_COUNT - 1) / PARALLEL_CHUNK_COUNT;
    task_handle.write().unwrap().set_total(total);
    let parent = dest_path
      .parent()
      .ok_or_else(|| SJMCLError("Download destination has no parent".into()))?;
    let task_id = task_handle.read().unwrap().desc.task_id;
    let mut chunks = Vec::new();
    for index in 0..PARALLEL_CHUNK_COUNT {
      let start = index * chunk_size;
      if start >= total {
        break;
      }
      let end = (start + chunk_size - 1).min(total - 1);
      chunks.push((
        index,
        start,
        end,
        parent.join(format!(".task-{task_id}.part-{index}")),
      ));
    }

    let mut jobs = futures::stream::FuturesUnordered::new();
    for (_, start, end, part_path) in chunks.iter().cloned() {
      let app = app_handle.clone();
      let param = param.clone();
      let handle = task_handle.clone();
      jobs.push(async move {
        // Keep all chunks on the preferred source. Rotating chunks across the
        // fallback URL can make one failed CDN cancel an otherwise healthy file.
        let response = Self::send_range_request(&app, start, end, &param, 0).await?;
        if response.status() != StatusCode::PARTIAL_CONTENT {
          return Err(SJMCLError(
            "Download source does not support range requests".into(),
          ));
        }
        let stream = ProgressStream::new_without_completion(
          response
            .bytes_stream()
            .map(|result| result.map_err(std::io::Error::other)),
          handle,
        );
        let mut file = tokio::fs::File::create(&part_path).await?;
        let received = tokio::io::copy(&mut stream.into_async_read().compat(), &mut file).await?;
        file.flush().await?;
        let expected = (end - start + 1) as u64;
        if received != expected {
          return Err(SJMCLError(format!(
            "Incomplete range download: expected {expected} bytes, received {received} bytes"
          )));
        }
        Ok::<(), SJMCLError>(())
      });
    }

    let mut failed = None;
    while let Some(result) = jobs.next().await {
      if let Err(error) = result {
        failed = Some(error);
      }
    }
    if let Some(error) = failed {
      for (_, _, _, part_path) in chunks {
        let _ = tokio::fs::remove_file(part_path).await;
      }
      return Err(error);
    }

    let mut output = tokio::fs::File::create(dest_path).await?;
    for (_, _, _, part_path) in chunks {
      let mut part = tokio::fs::File::open(&part_path).await?;
      tokio::io::copy(&mut part, &mut output).await?;
      let _ = tokio::fs::remove_file(part_path).await;
    }
    output.sync_data().await?;
    Ok(())
  }

  async fn create_resp_stream(
    app_handle: &AppHandle,
    current: i64,
    param: &DownloadParam,
    source_offset: usize,
  ) -> SJMCLResult<(
    impl Stream<Item = Result<bytes::Bytes, std::io::Error>> + Send + use<>,
    i64,
    bool,
  )> {
    let resp = Self::send_request(app_handle, current, param, source_offset).await?;
    let can_resume = current == 0 || resp.status() == StatusCode::PARTIAL_CONTENT;
    let total_progress = resp
      .headers()
      .get(CONTENT_RANGE)
      .and_then(|value| value.to_str().ok())
      .and_then(|value| value.rsplit_once('/'))
      .and_then(|(_, total)| total.parse::<i64>().ok())
      .or_else(|| {
        resp
          .content_length()
          .map(|length| length as i64 + if can_resume { current } else { 0 })
      })
      .unwrap_or(-1);
    Ok((
      resp
        .bytes_stream()
        .map(|res| res.map_err(std::io::Error::other)),
      total_progress,
      can_resume,
    ))
  }

  async fn future_impl(
    self,
    app_handle: AppHandle,
    limiter: Option<Limiter>,
  ) -> SJMCLResult<(
    impl Future<Output = SJMCLResult<()>> + Send,
    Arc<RwLock<PTaskHandle>>,
  )> {
    let handle = Arc::new(RwLock::new(self.p_handle));
    let task_handle = handle.clone();
    let param = self.param.clone();
    Ok((
      async move {
        tokio::fs::create_dir_all(&self.dest_path.parent().unwrap()).await?;

        if self.allow_parallel_chunks
          && task_handle.read().unwrap().desc.current == 0
          && let Some(total) = Self::probe_range_size(&app_handle, &param).await
          && total >= PARALLEL_CHUNK_THRESHOLD
        {
          match Self::download_parallel_chunks(
            &app_handle,
            &param,
            &self.dest_path,
            total,
            &task_handle,
          )
          .await
          {
            Ok(()) => {
              if let Some(truth) = param.sha1.clone() {
                validate_sha1(self.dest_path.clone(), truth)?;
              }
              task_handle.write().unwrap().mark_completed();
              return Ok(());
            }
            Err(error) => {
              warn!(
                "Parallel download failed, falling back to resumable download: {:?}",
                error
              );
              task_handle.write().unwrap().reset_progress();
              let _ = tokio::fs::remove_file(&self.dest_path).await;
            }
          }
        }

        let mut last_error = None;
        for attempt in 0..=DOWNLOAD_RETRY_COUNT {
          let current = task_handle.read().unwrap().desc.current;
          let response = Self::create_resp_stream(&app_handle, current, &param, attempt).await;
          let (resp, total_progress, can_resume) = match response {
            Ok(response) => response,
            Err(error) => {
              last_error = Some(error);
              if attempt < DOWNLOAD_RETRY_COUNT {
                tokio::time::sleep(Duration::from_millis(300 * (attempt + 1) as u64)).await;
                continue;
              }
              break;
            }
          };

          let current = if can_resume {
            current
          } else {
            task_handle.write().unwrap().reset_progress();
            0
          };
          if task_handle.read().unwrap().status().is_cancelled() {
            if tokio::fs::try_exists(&self.dest_path).await? {
              tokio::fs::remove_file(&self.dest_path).await?;
            }
            return Ok(());
          }
          {
            let mut handle = task_handle.write().unwrap();
            handle.set_total(total_progress);
            if !handle.status().is_in_progress() {
              handle.mark_started();
            }
          }
          let stream = ProgressStream::new(resp, task_handle.clone());
          let mut file = if current == 0 {
            tokio::fs::File::create(&self.dest_path).await?
          } else {
            let mut file = tokio::fs::OpenOptions::new()
              .write(true)
              .open(&self.dest_path)
              .await?;
            file.seek(std::io::SeekFrom::Start(current as u64)).await?;
            file
          };
          let result = if let Some(lim) = limiter.clone() {
            tokio::io::copy(&mut lim.limit(stream.into_async_read()).compat(), &mut file).await
          } else {
            tokio::io::copy(&mut stream.into_async_read().compat(), &mut file).await
          };
          file.sync_data().await?;

          match result {
            Ok(_) => {
              last_error = None;
              break;
            }
            Err(error) => {
              last_error = Some(error.into());
              if attempt < DOWNLOAD_RETRY_COUNT {
                tokio::time::sleep(Duration::from_millis(300 * (attempt + 1) as u64)).await;
              }
            }
          }
        }
        if let Some(error) = last_error {
          return Err(error);
        }
        if task_handle.read().unwrap().status().is_cancelled() {
          tokio::fs::remove_file(&self.dest_path).await?;
          Ok(())
        } else {
          match param.sha1 {
            Some(truth) => match validate_sha1(self.dest_path.clone(), truth) {
              Ok(()) => Ok(()),
              Err(error) => {
                let _ = tokio::fs::remove_file(&self.dest_path).await;
                Err(error)
              }
            },
            None => Ok(()),
          }
        }
      },
      handle,
    ))
  }

  pub async fn future(
    self,
    app_handle: AppHandle,
    limiter: Option<Limiter>,
  ) -> SJMCLResult<(
    impl Future<Output = SJMCLResult<()>> + Send,
    Arc<RwLock<PTaskHandle>>,
  )> {
    Self::future_impl(self, app_handle, limiter).await
  }
}
