use lazy_static::lazy_static;
use sjmcl_types::error::SJMCLResult;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_http::reqwest;

use crate::instance::helpers::client_json::McClientInfo;
use crate::instance::helpers::misc::get_instance_subdir_path_by_id;
use crate::instance::models::misc::{InstanceSubdirType, ModLoaderType};
use crate::launcher_config::models::LauncherConfig;
use crate::resource::helpers::curseforge::{
  fetch_remote_resource_by_id_curseforge, fetch_remote_resource_by_local_curseforge,
  fetch_resource_list_by_name_curseforge, fetch_resource_version_packs_curseforge,
};
use crate::resource::helpers::loader_meta::cleanroom::get_cleanroom_meta_by_game_version;
use crate::resource::helpers::loader_meta::fabric::get_fabric_meta_by_game_version;
use crate::resource::helpers::loader_meta::forge::get_forge_meta_by_game_version;
use crate::resource::helpers::loader_meta::neoforge::get_neoforge_meta_by_game_version;
use crate::resource::helpers::loader_meta::optifine::get_optifine_meta_by_game_version;
use crate::resource::helpers::loader_meta::quilt::get_quilt_meta_by_game_version;
use crate::resource::helpers::misc::{get_source_priority_list, resolve_mod_update_paths};
use crate::resource::helpers::modrinth::{
  fetch_remote_resource_by_id_modrinth, fetch_remote_resource_by_local_modrinth,
  fetch_resource_list_by_name_modrinth, fetch_resource_version_packs_modrinth,
};
use crate::resource::helpers::version_manifest::get_game_version_manifest;
use crate::resource::models::{
  GameClientResourceInfo, ModLoaderResourceInfo, ModUpdateQuery, OptiFineResourceInfo,
  OtherResourceFileInfo, OtherResourceInfo, OtherResourceSearchQuery, OtherResourceSearchRes,
  OtherResourceSource, OtherResourceVersionPack, OtherResourceVersionPackQuery, ResourceError,
};
use crate::tasks::PTaskParam;
use crate::tasks::commands::schedule_progressive_task_group;
use crate::tasks::download::DownloadParam;

const SEARCH_CACHE_TTL: Duration = Duration::from_secs(120);
const RESOURCE_CACHE_TTL: Duration = Duration::from_secs(1800);
const RESOURCE_CACHE_LIMIT: usize = 128;

struct CachedValue<T> {
  inserted_at: Instant,
  value: T,
}

lazy_static! {
  static ref RESOURCE_SEARCH_CACHE: Mutex<HashMap<String, CachedValue<OtherResourceSearchRes>>> =
    Mutex::new(HashMap::new());
  static ref RESOURCE_VERSION_CACHE: Mutex<HashMap<String, CachedValue<Vec<OtherResourceVersionPack>>>> =
    Mutex::new(HashMap::new());
  static ref RESOURCE_PROJECT_CACHE: Mutex<HashMap<String, CachedValue<OtherResourceInfo>>> =
    Mutex::new(HashMap::new());
}

fn cache_get<T: Clone>(
  cache: &Mutex<HashMap<String, CachedValue<T>>>,
  key: &str,
  ttl: Duration,
) -> Option<T> {
  let mut cache = cache.lock().ok()?;
  if cache
    .get(key)
    .is_some_and(|entry| entry.inserted_at.elapsed() <= ttl)
  {
    return cache.get(key).map(|entry| entry.value.clone());
  }
  cache.remove(key);
  None
}

fn cache_insert<T>(cache: &Mutex<HashMap<String, CachedValue<T>>>, key: String, value: T) {
  if let Ok(mut cache) = cache.lock() {
    if cache.len() >= RESOURCE_CACHE_LIMIT {
      cache.clear();
    }
    cache.insert(
      key,
      CachedValue {
        inserted_at: Instant::now(),
        value,
      },
    );
  }
}

fn resource_cache_key<T: serde::Serialize>(source: &OtherResourceSource, value: &T) -> String {
  format!(
    "{:?}:{}",
    source,
    serde_json::to_string(value).unwrap_or_default()
  )
}

#[tauri::command]
pub async fn fetch_game_version_list(app: AppHandle) -> SJMCLResult<Vec<GameClientResourceInfo>> {
  let priority_list = {
    let launcher_config_state = app.state::<Mutex<LauncherConfig>>();
    let launcher_config = launcher_config_state.lock()?;
    get_source_priority_list(&launcher_config)
  };
  get_game_version_manifest(&app, &priority_list).await
}

#[tauri::command]
pub async fn fetch_game_version_specific(
  app: AppHandle,
  game_version: String,
) -> SJMCLResult<GameClientResourceInfo> {
  let all_versions = fetch_game_version_list(app.clone()).await?;

  all_versions
    .into_iter()
    .find(|item| item.id == game_version)
    .ok_or_else(|| ResourceError::ClientVersionNotFound.into())
}

#[tauri::command]
pub async fn fetch_mod_loader_version_list(
  app: AppHandle,
  game_version: String,
  mod_loader_type: ModLoaderType,
) -> SJMCLResult<Vec<ModLoaderResourceInfo>> {
  let priority_list = {
    let launcher_config_state = app.state::<Mutex<LauncherConfig>>();
    let launcher_config = launcher_config_state.lock()?;
    get_source_priority_list(&launcher_config)
  };
  match mod_loader_type {
    ModLoaderType::Forge | ModLoaderType::LegacyForge => {
      Ok(get_forge_meta_by_game_version(&app, &priority_list, &game_version).await?)
    }
    ModLoaderType::Cleanroom => Ok(get_cleanroom_meta_by_game_version(&app, &game_version).await?),
    ModLoaderType::Fabric => {
      Ok(get_fabric_meta_by_game_version(&app, &priority_list, &game_version).await?)
    }
    ModLoaderType::NeoForge => {
      Ok(get_neoforge_meta_by_game_version(&app, &priority_list, &game_version).await?)
    }
    ModLoaderType::Quilt => {
      Ok(get_quilt_meta_by_game_version(&app, &priority_list, &game_version).await?)
    }
    // TODO here
    _ => Err(ResourceError::NoDownloadApi.into()),
  }
}

#[tauri::command]
pub async fn fetch_optifine_version_list(
  app: AppHandle,
  game_version: String,
) -> SJMCLResult<Vec<OptiFineResourceInfo>> {
  let priority_list = {
    let launcher_config_state = app.state::<Mutex<LauncherConfig>>();
    let launcher_config = launcher_config_state.lock()?;
    get_source_priority_list(&launcher_config)
  };
  get_optifine_meta_by_game_version(&app, &priority_list, &game_version).await
}

#[tauri::command]
pub async fn fetch_resource_list_by_name(
  app: AppHandle,
  download_source: OtherResourceSource,
  query: OtherResourceSearchQuery,
) -> SJMCLResult<OtherResourceSearchRes> {
  let cache_key = resource_cache_key(&download_source, &query);
  if let Some(cached) = cache_get(&RESOURCE_SEARCH_CACHE, &cache_key, SEARCH_CACHE_TTL) {
    return Ok(cached);
  }

  let result: OtherResourceSearchRes = match download_source {
    OtherResourceSource::CurseForge => fetch_resource_list_by_name_curseforge(&app, &query).await,
    OtherResourceSource::Modrinth => fetch_resource_list_by_name_modrinth(&app, &query).await,
    _ => Err(ResourceError::NoDownloadApi.into()),
  }?;
  cache_insert(&RESOURCE_SEARCH_CACHE, cache_key, result.clone());
  Ok(result)
}

#[tauri::command]
pub async fn fetch_resource_version_packs(
  app: AppHandle,
  download_source: OtherResourceSource,
  query: OtherResourceVersionPackQuery,
) -> SJMCLResult<Vec<OtherResourceVersionPack>> {
  let cache_key = resource_cache_key(&download_source, &query);
  if let Some(cached) = cache_get(&RESOURCE_VERSION_CACHE, &cache_key, RESOURCE_CACHE_TTL) {
    return Ok(cached);
  }

  let result: Vec<OtherResourceVersionPack> = match download_source {
    OtherResourceSource::CurseForge => fetch_resource_version_packs_curseforge(&app, &query).await,
    OtherResourceSource::Modrinth => fetch_resource_version_packs_modrinth(&app, &query).await,
    _ => Err(ResourceError::NoDownloadApi.into()),
  }?;
  cache_insert(&RESOURCE_VERSION_CACHE, cache_key, result.clone());
  Ok(result)
}

#[tauri::command]
pub async fn download_game_server(
  app: AppHandle,
  client: State<'_, reqwest::Client>,
  resource_info: GameClientResourceInfo,
  dest: String,
) -> SJMCLResult<()> {
  let version_details = client
    .get(&resource_info.url)
    .send()
    .await
    .map_err(|_| ResourceError::NetworkError)?
    .json::<McClientInfo>()
    .await
    .map_err(|_| ResourceError::ParseError)?;

  let download_info = version_details
    .downloads
    .get("server")
    .ok_or(ResourceError::ParseError)?;

  schedule_progressive_task_group(
    app,
    format!("game-server?{}", resource_info.id),
    vec![PTaskParam::Download(DownloadParam {
      src: url::Url::parse(&download_info.url.clone()).map_err(|_| ResourceError::ParseError)?,
      dest: dest.clone().into(),
      filename: None,
      sha1: Some(download_info.sha1.clone()),
    })],
    true,
  )
  .await?;

  Ok(())
}

#[tauri::command]
pub async fn fetch_remote_resource_by_local(
  app: AppHandle,
  download_source: OtherResourceSource,
  file_path: String,
) -> SJMCLResult<OtherResourceFileInfo> {
  match download_source {
    OtherResourceSource::CurseForge => {
      Ok(fetch_remote_resource_by_local_curseforge(&app, &file_path).await?)
    }
    OtherResourceSource::Modrinth => {
      Ok(fetch_remote_resource_by_local_modrinth(&app, &file_path).await?)
    }
    _ => Err(ResourceError::NoDownloadApi.into()),
  }
}

#[tauri::command]
pub async fn update_mods(
  app: AppHandle,
  instance_id: String,
  queries: Vec<ModUpdateQuery>,
) -> SJMCLResult<()> {
  if queries.is_empty() {
    return Ok(());
  }

  let mods_dir = match get_instance_subdir_path_by_id(&app, &instance_id, &InstanceSubdirType::Mods)
  {
    Some(path) => path,
    None => return Ok(()),
  };

  let update_paths = resolve_mod_update_paths(&mods_dir, &queries)?;

  let mut download_tasks = Vec::new();
  for (query, (_, new_file_path)) in queries.iter().zip(&update_paths) {
    let download_param = DownloadParam {
      src: url::Url::parse(&query.url).map_err(|_| ResourceError::ParseError)?,
      dest: new_file_path.clone(),
      filename: None,
      sha1: Some(query.sha1.clone()),
    };
    download_tasks.push(PTaskParam::Download(download_param));
  }

  schedule_progressive_task_group(app, "mod-update".to_string(), download_tasks, true).await?;

  for (old_file_path, new_file_path) in update_paths {
    if old_file_path != new_file_path {
      let old_backup_path = format!("{}.old", old_file_path.display());
      if let Err(e) = std::fs::rename(&old_file_path, old_backup_path) {
        log::error!("Failed to rename old mod file: {}", e);
        return Err(ResourceError::FileOperationError.into());
      }
    }
  }

  Ok(())
}

#[tauri::command]
pub async fn fetch_remote_resource_by_id(
  app: AppHandle,
  download_source: OtherResourceSource,
  resource_id: String,
) -> SJMCLResult<OtherResourceInfo> {
  let cache_key = resource_cache_key(&download_source, &resource_id);
  if let Some(cached) = cache_get(&RESOURCE_PROJECT_CACHE, &cache_key, RESOURCE_CACHE_TTL) {
    return Ok(cached);
  }

  let result: OtherResourceInfo = match download_source {
    OtherResourceSource::CurseForge => {
      fetch_remote_resource_by_id_curseforge(&app, &resource_id).await
    }
    OtherResourceSource::Modrinth => fetch_remote_resource_by_id_modrinth(&app, &resource_id).await,
    _ => Err(ResourceError::NoDownloadApi.into()),
  }?;
  cache_insert(&RESOURCE_PROJECT_CACHE, cache_key, result.clone());
  Ok(result)
}
