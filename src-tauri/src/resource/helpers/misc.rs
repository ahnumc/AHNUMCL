use sjmcl_types::error::{SJMCLError, SJMCLResult};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use strum::IntoEnumIterator;
use url::Url;

use crate::launcher_config::models::LauncherConfig;
use crate::resource::models::{
  ModUpdateQuery, OtherResourceInfo, OtherResourceVersionPack, ResourceError, ResourceType,
  SourceType,
};
use crate::utils::string::contains_chinese;

static SOURCE_PREFERENCE_PROBED: AtomicBool = AtomicBool::new(false);
static PREFER_OFFICIAL_SOURCE: AtomicBool = AtomicBool::new(false);

pub fn set_auto_source_preference(prefer_official: bool) {
  PREFER_OFFICIAL_SOURCE.store(prefer_official, AtomicOrdering::Relaxed);
  SOURCE_PREFERENCE_PROBED.store(true, AtomicOrdering::Relaxed);
}

pub fn get_minecraft_download_mirror(url: &Url) -> Option<Url> {
  let host = url.host_str()?;
  let path = url.path();
  let mirror_path = match host {
    "piston-data.mojang.com"
    | "piston-meta.mojang.com"
    | "launcher.mojang.com"
    | "launchermeta.mojang.com" => path.to_string(),
    "resources.download.minecraft.net" => format!("/assets{path}"),
    "libraries.minecraft.net" => format!("/maven{path}"),
    _ => return None,
  };

  let mut mirror = url.clone();
  mirror.set_host(Some("bmclapi2.bangbang93.com")).ok()?;
  mirror.set_path(&mirror_path);
  Some(mirror)
}

pub fn get_source_priority_list(launcher_config: &LauncherConfig) -> Vec<SourceType> {
  match launcher_config.download.source.strategy.as_str() {
    "official" => vec![SourceType::Official, SourceType::BMCLAPIMirror],
    "mirror" => vec![SourceType::BMCLAPIMirror, SourceType::Official],
    "auto" => {
      let prefer_official = if SOURCE_PREFERENCE_PROBED.load(AtomicOrdering::Relaxed) {
        PREFER_OFFICIAL_SOURCE.load(AtomicOrdering::Relaxed)
      } else {
        !launcher_config.basic_info.is_china_mainland_ip
      };
      if prefer_official {
        vec![SourceType::Official, SourceType::BMCLAPIMirror]
      } else {
        vec![SourceType::BMCLAPIMirror, SourceType::Official]
      }
    }
    _ => vec![SourceType::BMCLAPIMirror, SourceType::Official],
  }
}

// https://bmclapidoc.bangbang93.com/
pub fn get_download_api(source: SourceType, resource_type: ResourceType) -> SJMCLResult<Url> {
  match source {
    SourceType::Official => match resource_type {
      ResourceType::VersionManifest => Ok(Url::parse(
        "https://launchermeta.mojang.com/mc/game/version_manifest.json",
      )?),
      ResourceType::VersionManifestV2 => Ok(Url::parse(
        "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json",
      )?),
      ResourceType::LauncherMeta => Ok(Url::parse("https://launchermeta.mojang.com/")?),
      ResourceType::Launcher => Ok(Url::parse("https://launcher.mojang.com/")?),
      ResourceType::Assets => Ok(Url::parse("https://resources.download.minecraft.net/")?),
      ResourceType::Libraries => Ok(Url::parse("https://libraries.minecraft.net/")?),
      ResourceType::MojangJava => Ok(Url::parse(
        "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json",
      )?),
      ResourceType::ForgeMaven => Ok(Url::parse("https://files.minecraftforge.net/maven/")?),
      ResourceType::ForgeMavenNew => Ok(Url::parse("https://maven.minecraftforge.net")?),
      ResourceType::ForgeInstall => Ok(Url::parse(
        "https://maven.minecraftforge.net/net/minecraftforge/forge/",
      )?),
      ResourceType::ForgeMeta => Err(ResourceError::NoDownloadApi.into()), // https://github.com/HMCL-dev/HMCL/pull/3259/files
      ResourceType::Liteloader => Ok(Url::parse(
        "https://dl.liteloader.com/versions/versions.json",
      )?),
      ResourceType::OptiFine => Err(ResourceError::NoDownloadApi.into()), //
      ResourceType::AuthlibInjector => Ok(Url::parse("https://authlib-injector.yushi.moe/")?),
      ResourceType::FabricMeta => Ok(Url::parse("https://meta.fabricmc.net/")?),
      ResourceType::FabricMaven => Ok(Url::parse("https://maven.fabricmc.net/")?),
      // https://github.com/HMCL-dev/HMCL/blob/efd088e014bf1c113f7b3fdf73fb983087ae3f5e/HMCLCore/src/main/java/org/jackhuang/hmcl/download/neoforge/NeoForgeOfficialVersionList.java#L28
      ResourceType::NeoforgeMetaForge => Ok(Url::parse(
        "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/forge/",
      )?),
      ResourceType::NeoforgeMetaNeoforge => Ok(Url::parse(
        "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge/",
      )?),
      ResourceType::NeoforgeMaven | ResourceType::NeoforgeInstall => {
        Ok(Url::parse("https://maven.neoforged.net/releases/")?)
      }
      ResourceType::QuiltMaven => Ok(Url::parse("https://maven.quiltmc.org/repository/release/")?),
      ResourceType::QuiltMeta => Ok(Url::parse("https://meta.quiltmc.org/")?),
      ResourceType::CleanroomInstall => Ok(Url::parse(
        "https://hmcl.glavo.site/metadata/cleanroom/files/",
      )?),
      ResourceType::CleanroomMaven => Ok(Url::parse("https://maven.cleanroommc.com/")?),
      ResourceType::CleanroomMeta => Ok(Url::parse(
        "https://hmcl.glavo.site/metadata/cleanroom/index.json",
      )?),
    },
    SourceType::BMCLAPIMirror => match resource_type {
      ResourceType::VersionManifest => Ok(Url::parse(
        "https://bmclapi2.bangbang93.com/mc/game/version_manifest.json",
      )?),
      ResourceType::VersionManifestV2 => Ok(Url::parse(
        "https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json",
      )?),
      ResourceType::LauncherMeta => Ok(Url::parse("https://bmclapi2.bangbang93.com/")?),
      ResourceType::Launcher => Ok(Url::parse("https://bmclapi2.bangbang93.com/")?),
      ResourceType::Assets => Ok(Url::parse("https://bmclapi2.bangbang93.com/assets/")?),
      ResourceType::Libraries => Ok(Url::parse("https://bmclapi2.bangbang93.com/maven/")?),
      ResourceType::MojangJava => Ok(Url::parse(
        "https://bmclapi2.bangbang93.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json",
      )?),
      ResourceType::ForgeMaven | ResourceType::ForgeMavenNew | ResourceType::NeoforgeMaven => {
        Ok(Url::parse("https://bmclapi2.bangbang93.com/maven/")?)
      }
      ResourceType::ForgeInstall => Ok(Url::parse(
        "https://bmclapi2.bangbang93.com/forge/download/",
      )?),
      ResourceType::ForgeMeta => Ok(Url::parse("https://bmclapi2.bangbang93.com/forge/")?),
      ResourceType::Liteloader => Ok(Url::parse(
        "https://bmclapi.bangbang93.com/maven/com/mumfrey/liteloader/versions.json",
      )?),
      ResourceType::AuthlibInjector => Ok(Url::parse(
        "https://bmclapi2.bangbang93.com/mirrors/authlib-injector/",
      )?),
      ResourceType::FabricMeta => Ok(Url::parse("https://bmclapi2.bangbang93.com/fabric-meta/")?),
      ResourceType::FabricMaven => Ok(Url::parse("https://bmclapi2.bangbang93.com/maven/")?),
      ResourceType::NeoforgeMetaForge | ResourceType::NeoforgeMetaNeoforge => {
        Ok(Url::parse("https://bmclapi2.bangbang93.com/neoforge/")?)
      }
      ResourceType::NeoforgeInstall => Ok(Url::parse(
        "https://bmclapi2.bangbang93.com/neoforge/version/",
      )?),
      ResourceType::OptiFine => Ok(Url::parse("https://bmclapi2.bangbang93.com/optifine/")?),
      ResourceType::QuiltMaven => Ok(Url::parse("https://bmclapi2.bangbang93.com/maven/")?),
      ResourceType::QuiltMeta => Ok(Url::parse("https://bmclapi2.bangbang93.com/quilt-meta/")?), // seems 'not found'
      ResourceType::CleanroomInstall => Ok(Url::parse(
        "https://alist.8mi.tech/d/mirror/HMCL-Metadata/Auto/cleanroom/files/",
      )?),
      ResourceType::CleanroomMaven => Ok(Url::parse("https://maven.cleanroommc.com/")?),
      ResourceType::CleanroomMeta => Ok(Url::parse(
        "https://alist.8mi.tech/d/mirror/HMCL-Metadata/Auto/cleanroom/index.json",
      )?),
    },
  }
}

#[expect(dead_code, reason = "reserved for future use")]
pub fn convert_url_source_type(
  url: &Url,
  resource_type: &ResourceType,
  src_type: &SourceType,
  dst_type: &SourceType,
) -> SJMCLResult<Url> {
  let url_str = url.as_str();
  let src_api = get_download_api(*src_type, *resource_type)?;
  let dst_api = get_download_api(*dst_type, *resource_type)?;
  if url_str.starts_with(src_api.as_str()) {
    Ok(Url::parse(
      url_str
        .replacen(src_api.as_str(), dst_api.as_str(), 1)
        .as_str(),
    )?)
  } else {
    Err(ResourceError::NoDownloadApi.into())
  }
}

pub fn convert_url_to_target_source(
  url: &Url,
  resource_types: &[ResourceType],
  dst_type: &SourceType,
) -> SJMCLResult<Url> {
  let url_str = url.as_str();
  let resource_candidates = if resource_types.is_empty() {
    ResourceType::iter().collect::<Vec<_>>()
  } else {
    resource_types.to_vec()
  };

  for resource_type in resource_candidates {
    let dst_api = match get_download_api(*dst_type, resource_type) {
      Ok(api) => api,
      Err(_) => return Ok(url.clone()), // If destination API is not available, return the original URL
    };

    for src_type in SourceType::iter() {
      if &src_type == dst_type {
        continue;
      }

      if let Ok(src_api) = get_download_api(src_type, resource_type)
        && url_str.starts_with(src_api.as_str())
      {
        let new_url_str = url_str.replacen(src_api.as_str(), dst_api.as_str(), 1);
        return Ok(Url::parse(&new_url_str)?);
      }
    }
  }

  // If no replacement occurred, return the original URL
  Ok(url.clone())
}

pub fn version_pack_sort(a: &OtherResourceVersionPack, b: &OtherResourceVersionPack) -> Ordering {
  fn parse_version(version: &str) -> (Vec<u32>, String) {
    let mut version_numbers = Vec::new();
    let mut suffix = String::new();

    for part in version.split('.') {
      if let Some(dash_pos) = part.find('-') {
        let (num_part, suffix_part) = part.split_at(dash_pos);
        if let Ok(num) = num_part.parse::<u32>() {
          version_numbers.push(num);
          suffix = suffix_part.to_string();
        }
        break;
      } else if let Ok(num) = part.parse::<u32>() {
        version_numbers.push(num);
      }
    }

    (version_numbers, suffix)
  }

  fn compare_versions_with_suffix(
    v1: &[u32],
    suffix1: &str,
    v2: &[u32],
    suffix2: &str,
  ) -> Ordering {
    for (a, b) in v1.iter().zip(v2.iter()) {
      match a.cmp(b) {
        Ordering::Equal => continue,
        other => return other,
      }
    }

    match v1.len().cmp(&v2.len()) {
      Ordering::Equal => match (suffix1.is_empty(), suffix2.is_empty()) {
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        _ => suffix1.cmp(suffix2),
      },
      other => other,
    }
  }

  let (version_a, suffix_a) = parse_version(&a.name);
  let (version_b, suffix_b) = parse_version(&b.name);

  compare_versions_with_suffix(&version_a, &suffix_a, &version_b, &suffix_b).reverse()
}

pub(crate) fn levenshtein_distance(a: &str, b: &str) -> usize {
  let b_chars: Vec<char> = b.chars().collect();
  let mut prev: Vec<usize> = (0..=b_chars.len()).collect();

  for (i, a_ch) in a.chars().enumerate() {
    let mut current = Vec::with_capacity(b_chars.len() + 1);
    current.push(i + 1);

    for (j, b_ch) in b_chars.iter().enumerate() {
      let cost = if a_ch == *b_ch { 0 } else { 1 };
      let insertion = current[j] + 1;
      let deletion = prev[j + 1] + 1;
      let substitution = prev[j] + cost;
      current.push(insertion.min(deletion).min(substitution));
    }

    prev = current;
  }

  *prev.last().unwrap_or(&0)
}

pub fn sort_localized_search_results(list: &mut Vec<OtherResourceInfo>, search_query: &str) {
  const CONTAIN_CHINESE_WEIGHT: i64 = 10;

  let search_query = search_query
    .split_whitespace()
    .collect::<Vec<_>>()
    .join(" ");

  if search_query.is_empty() {
    return;
  }

  let mut translated_results = Vec::new();
  let mut untranslated_results = Vec::new();

  for resource in list.drain(..) {
    let contains_chinese_title = resource
      .translated_name
      .as_deref()
      .map_or_else(|| contains_chinese(&resource.name), contains_chinese);

    if contains_chinese_title {
      translated_results.push(resource);
    } else {
      untranslated_results.push(resource);
    }
  }

  translated_results.sort_by_key(|resource| {
    let display_name = resource
      .translated_name
      .as_deref()
      .unwrap_or(resource.name.as_str());

    let mut diff = levenshtein_distance(&search_query, display_name) as i64;
    for ch in search_query.chars() {
      if display_name.contains(ch) {
        diff -= CONTAIN_CHINESE_WEIGHT;
      }
    }

    diff
  });

  list.extend(translated_results);
  list.extend(untranslated_results);
}

#[cfg(test)]
mod tests {
  use super::get_minecraft_download_mirror;
  use url::Url;

  #[test]
  fn maps_mojang_download_hosts_to_bmclapi() {
    let cases = [
      (
        "https://piston-data.mojang.com/v1/objects/abc/client.json",
        "/v1/objects/abc/client.json",
      ),
      (
        "https://resources.download.minecraft.net/ab/cdef",
        "/assets/ab/cdef",
      ),
      (
        "https://libraries.minecraft.net/com/example/library.jar",
        "/maven/com/example/library.jar",
      ),
    ];

    for (source, expected_path) in cases {
      let mirror = get_minecraft_download_mirror(&Url::parse(source).unwrap()).unwrap();
      assert_eq!(mirror.host_str(), Some("bmclapi2.bangbang93.com"));
      assert_eq!(mirror.path(), expected_path);
    }
  }
}

pub fn resolve_mod_update_paths(
  mods_dir: &Path,
  queries: &[ModUpdateQuery],
) -> SJMCLResult<Vec<(PathBuf, PathBuf)>> {
  let canonical_mods_dir = mods_dir.canonicalize().map_err(|error| {
    SJMCLError(format!(
      "Failed to resolve mods directory {}: {}",
      mods_dir.display(),
      error
    ))
  })?;
  let mut targets = HashSet::new();
  let mut paths = Vec::with_capacity(queries.len());

  for query in queries {
    let old_file_path = PathBuf::from(&query.old_file_path);
    if !old_file_path.is_file() {
      return Err(SJMCLError(format!(
        "Old mod file does not exist: {}",
        old_file_path.display()
      )));
    }

    let old_parent = old_file_path.parent().ok_or_else(|| {
      SJMCLError(format!(
        "Old mod file has no parent directory: {}",
        old_file_path.display()
      ))
    })?;
    let canonical_old_parent = old_parent.canonicalize().map_err(|error| {
      SJMCLError(format!(
        "Failed to resolve old mod directory {}: {}",
        old_parent.display(),
        error
      ))
    })?;
    if !canonical_old_parent.starts_with(&canonical_mods_dir) {
      return Err(SJMCLError(format!(
        "Old mod file is outside the instance mods directory: {}",
        old_file_path.display()
      )));
    }

    let new_file_name = Path::new(&query.file_name);
    if new_file_name.file_name() != Some(new_file_name.as_os_str()) {
      return Err(SJMCLError(format!(
        "Invalid mod update file name: {}",
        query.file_name
      )));
    }

    let new_file_path = old_parent.join(new_file_name);
    let canonical_target = canonical_old_parent.join(new_file_name);
    if !targets.insert(canonical_target) {
      return Err(SJMCLError(format!(
        "Duplicate mod update target: {}",
        new_file_path.display()
      )));
    }
    if new_file_path != old_file_path && new_file_path.exists() {
      return Err(SJMCLError(format!(
        "Mod update target already exists: {}",
        new_file_path.display()
      )));
    }

    paths.push((old_file_path, new_file_path));
  }

  Ok(paths)
}
