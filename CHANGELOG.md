**English** · [简体中文](docs/CHANGELOG.zh-Hans.md)

AHNUMCL follows [Semantic Versioning 2.0.0](https://semver.org/).

## 1.1.0

`2026-10-20`

- 🌟 Support installing and managing the Cleanroom loader, with matching Java runtime selection and download requirements.
- 🌟 Add liquid glass effects to launch-page buttons and support saving skin images from the skin viewer.
- 🌟 Support automatically closing the game crash notification window after a configurable delay.
- 🐛 Restore built-in WebP wallpapers and fix the window opacity slider by applying opacity to the wallpaper layer while keeping text and controls readable.
- 🐛 Support Minecraft 26.x world data and Quilt modpacks with optional metadata fields omitted.
- 🐛 Fix mod management in supported loader subdirectories, file paths after enabling or disabling mods, and the mod update dialog.
- 🐛 Fix local account custom skins in NeoForge instances and preserve resource-pack caches when world loading fails.
- 🐛 Fix startup dialog ordering and avatar layout for long player names, and open account settings when launching without an account.
- 🚀 Reduce unnecessary rendering with lazy accordion sections and incremental mod-version loading, and avoid duplicate mod metadata parsing.
- 🚀 Automatically clean up stale download caches on startup.
- 🛠 Replace the server-status query library and update Minecraft version lists and mod metadata.
- 📦 Update Tauri, Next.js, MCP and related dependencies, address Rust dependency security advisories, and raise the minimum Rust version to 1.98.1.
- 📦 Align the frontend and Rust clipboard and filesystem plugin versions to fix Tauri release builds.

## 1.0.6

`2026-09-04`

- 🌟 Add built-in Terracotta multiplayer support.
- 🌟 Add the AHNU Blocky light and dark wallpaper set and replace the former clock-tower wallpapers.
- 🐛 Use Modrinth as the default discover resource source.

## 1.0.5

`2026-08-19`

- 🌟 Restore AHNUMC server pack auto import in instance installation flows.
- 🚀 Improve responsiveness by avoiding blocking version and instance install operations and reducing nested modal rendering.
- 🐛 Preserve the nested dependency modal flow and defer side effects from state updates for more reliable resource installation.
- 🐛 Smooth bursty task speed reporting and keep download task behavior more stable under load.
- 🐛 Remove the LittleSkin login entry from account options.

## 1.0.4

`2026-08-17`

- 🚀 Improve download throughput with higher automatic concurrency, resilient retries, and HTTP Range resume support.
- 🚀 Select official and mirror sources using PCL-CE-style timeouts and fallback ordering, preferring fast direct connections when available.
- 🐛 Fix CurseForge API and CDN access, modpack imports without a proxy, and stalled or unknown-size download progress indicators.
- 🐛 Preserve CurseForge API keys containing special characters and add mirror fallback for Authlib-Injector metadata and JAR downloads.
- 🌟 Add a latest-log viewer in instance settings with search, copy, and direct file access.

## 1.0.3

`2026-08-16`

- 🌟 Add custom launcher primary colors with preset palettes and custom color input.
- 🌟 Add interface and log font selection, searchable custom fonts, and adjustable font size.
- 🌟 Add separate light and dark interface background palettes with custom colors and opacity controls.
- 🌟 Add window opacity settings and smooth slider dragging without continuous configuration writes.
- 🌟 Add the AHNU Flowy light and dark wallpaper set and make it the default background.
- 🐛 Hide inactive Windows titlebar controls to prevent overlapping controls from showing through transparent windows.
- 🐛 Replace remaining SJMCL labels and default data directory names with AHNUMCL branding.

## 1.0.2

`2026-08-16`

- 🌟 Support AHNUMC Skin login through Yggdrasil Connect device authorization.
- 🌟 Replace the built-in wallpapers with compressed WebP assets and set Zheshan Gate as the default background.
- 🌟 Restore the MUA community news source and improve retry handling for community news requests.
- 🐛 Fix unreliable expand and collapse behavior of the development toolbar.

## 1.0.1

`2026-08-15`

- 🌟 Add AHNUMC server list and one-click client modpack installation from the instance page.
- 🌟 Add AHNUMC third-party account login and remove the Microsoft-first login restriction.
- 🌟 Add GitHub download mirror settings, automatic latency testing, and fastest-mirror selection.
- 🛠 Move built-in server and news sources to the AHNUMC API.

## 1.0.0

`2026-08-14`

**The first stable release of AHNUMCL.**

- 🌟 Rename the launcher, configuration file, application identifiers, and user-facing branding from SJMCL/ANUMCL to AHNUMCL.
- 🌟 Provide an AHNUMC account center integration.
