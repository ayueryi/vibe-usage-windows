//! Shared app state (counterpart of AppState.swift's service wiring).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::time::Instant;
use vibe_core::config::ConfigManager;
use vibe_core::quota_product::{normalize_selection, ZCodeQuotaRegion};
use vibe_core::{ProviderRateLimit, RateLimitProvider};

pub const IS_DEV: bool = cfg!(debug_assertions);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    Light,
    Dark,
    #[default]
    #[serde(other)]
    System,
}

/// Persisted app settings — counterpart of the macOS UserDefaults keys
/// (showCostInMenuBar / showTokensInMenuBar / codexRateLimitEnabled /
/// claudeRateLimitEnabled).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub theme_preference: ThemePreference,
    pub show_cost_in_tray: bool,
    pub show_tokens_in_tray: bool,
    pub codex_rate_limit_enabled: bool,
    pub claude_rate_limit_enabled: bool,
    pub selected_quota_product_ids: Vec<RateLimitProvider>,
    pub quota_selection_initialized: bool,
    pub z_code_quota_region: ZCodeQuotaRegion,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme_preference: ThemePreference::System,
            show_cost_in_tray: true,
            show_tokens_in_tray: false,
            codex_rate_limit_enabled: true,
            claude_rate_limit_enabled: false,
            selected_quota_product_ids: Vec::new(),
            quota_selection_initialized: false,
            z_code_quota_region: ZCodeQuotaRegion::default(),
        }
    }
}

impl AppSettings {
    pub fn set_quota_selection(&mut self, selection: Vec<RateLimitProvider>) {
        self.selected_quota_product_ids = normalize_selection(selection);
        self.quota_selection_initialized = true;
        // Preserve downgrade behavior for the original two toggles.
        self.codex_rate_limit_enabled = self
            .selected_quota_product_ids
            .contains(&RateLimitProvider::Codex);
        self.claude_rate_limit_enabled = self
            .selected_quota_product_ids
            .contains(&RateLimitProvider::ClaudeCode);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncStatus {
    Idle,
    Syncing,
    Success,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncState {
    pub status: SyncStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// epoch millis of last successful sync
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync_at: Option<u64>,
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            status: SyncStatus::Idle,
            message: None,
            last_sync_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

#[derive(Default)]
pub struct RateLimitCache {
    pub snapshots: HashMap<RateLimitProvider, (ProviderRateLimit, Instant)>,
    zcode_generation: u64,
}

impl RateLimitCache {
    pub fn zcode_generation(&self) -> u64 {
        self.zcode_generation
    }

    /// Called while holding settings, before exposing a changed region/key.
    pub fn invalidate_zcode(&mut self) {
        self.zcode_generation += 1;
        self.snapshots.remove(&RateLimitProvider::ZCode);
    }

    pub fn store(&mut self, snapshot: ProviderRateLimit, zcode_generation: u64) {
        if snapshot.provider == RateLimitProvider::ZCode
            && zcode_generation != self.zcode_generation
        {
            return;
        }
        self.snapshots
            .insert(snapshot.provider, (snapshot, Instant::now()));
    }
}

pub struct AppCtx {
    pub config: ConfigManager,
    pub http: reqwest::Client,
    pub settings_path: PathBuf,
    pub settings: Mutex<AppSettings>,
    pub sync_state: Mutex<SyncState>,
    /// Mutual exclusion for the CLI subprocess (同步已在进行中 guard).
    pub sync_running: tokio::sync::Mutex<()>,
    /// Set when a sync is requested while one is already running so the
    /// in-flight run drains a follow-up instead of dropping the request.
    pub sync_pending: AtomicBool,
    pub device_link_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    pub scheduler_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    /// Prevent overlapping UI events from launching duplicate network/probe work.
    pub rate_limit_refresh: tokio::sync::Mutex<()>,
    pub rate_limits: Mutex<RateLimitCache>,
    pub update_info: Mutex<Option<UpdateInfo>>,
    /// (cost, tokens) for the active time range, pushed by the frontend.
    pub tray_stats: Mutex<Option<(f64, i64)>>,
}

impl AppCtx {
    pub fn new(app_config_dir: PathBuf) -> Self {
        let settings_path = app_config_dir.join("settings.json");
        let raw_settings = std::fs::read_to_string(&settings_path).ok();
        let had_legacy_selection = raw_settings
            .as_deref()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            .and_then(|value| value.as_object().cloned())
            .is_some_and(|object| {
                object.contains_key("codexRateLimitEnabled")
                    || object.contains_key("claudeRateLimitEnabled")
            });
        let mut settings: AppSettings = raw_settings
            .as_deref()
            .and_then(|raw| serde_json::from_str(raw).ok())
            .unwrap_or_default();
        let migrated_legacy_selection =
            !settings.quota_selection_initialized && had_legacy_selection;
        if migrated_legacy_selection {
            let selection = [
                settings
                    .codex_rate_limit_enabled
                    .then_some(RateLimitProvider::Codex),
                settings
                    .claude_rate_limit_enabled
                    .then_some(RateLimitProvider::ClaudeCode),
            ]
            .into_iter()
            .flatten()
            .collect();
            settings.set_quota_selection(selection);
            if let Ok(data) = serde_json::to_string_pretty(&settings) {
                let _ = vibe_core::config::atomic_write(&settings_path, data.as_bytes());
            }
        }
        Self {
            config: ConfigManager::new(IS_DEV),
            http: reqwest::Client::builder()
                .user_agent(format!("VibeUsageWindows/{}", env!("CARGO_PKG_VERSION")))
                // system-proxy feature honors the Windows proxy; a hung
                // connect must fail fast instead of pinning spinners.
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("http client"),
            settings_path,
            settings: Mutex::new(settings),
            sync_state: Mutex::new(SyncState::default()),
            sync_running: tokio::sync::Mutex::new(()),
            sync_pending: AtomicBool::new(false),
            device_link_task: Mutex::new(None),
            scheduler_task: Mutex::new(None),
            rate_limit_refresh: tokio::sync::Mutex::new(()),
            rate_limits: Mutex::new(RateLimitCache::default()),
            update_info: Mutex::new(None),
            tray_stats: Mutex::new(None),
        }
    }

    pub fn save_settings(&self) {
        // Serialize writes with the appearance patch as well as in-memory changes.
        // An older quota/tray snapshot must not overwrite a newer theme on disk.
        let settings = self.settings.lock().unwrap();
        if let Ok(data) = serde_json::to_string_pretty(&*settings) {
            let _ = vibe_core::config::atomic_write(&self.settings_path, data.as_bytes());
        }
    }

    /// Patch only appearance so a concurrent quota/tray update cannot be lost.
    /// Persist before publishing; a failed write leaves the active theme intact.
    pub fn set_theme_preference(&self, preference: ThemePreference) -> Result<AppSettings, String> {
        let mut current = self.settings.lock().unwrap();
        let mut next = current.clone();
        next.theme_preference = preference;
        let data = serde_json::to_vec_pretty(&next).map_err(|error| error.to_string())?;
        vibe_core::config::atomic_write(&self.settings_path, &data)
            .map_err(|error| error.to_string())?;
        *current = next.clone();
        Ok(next)
    }

    pub fn hostname() -> Option<String> {
        let raw = gethostname::gethostname().to_string_lossy().to_string();
        let cleaned = raw.trim().trim_end_matches(".local").to_string();
        if cleaned.is_empty() {
            None
        } else {
            Some(cleaned)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn legacy_and_unknown_theme_preferences_follow_system() {
        for json in [r#"{}"#, r#"{"themePreference":"unknown"}"#] {
            let settings: AppSettings = serde_json::from_str(json).unwrap();
            assert_eq!(settings.theme_preference, ThemePreference::System);
        }
    }

    #[test]
    fn theme_patch_survives_restart_and_preserves_other_preferences() {
        let directory = tempfile::tempdir().unwrap();
        let ctx = AppCtx::new(directory.path().to_path_buf());
        ctx.settings.lock().unwrap().show_tokens_in_tray = true;
        for preference in [
            ThemePreference::Light,
            ThemePreference::Dark,
            ThemePreference::System,
        ] {
            ctx.set_theme_preference(preference).unwrap();
            let saved: AppSettings =
                serde_json::from_slice(&fs::read(&ctx.settings_path).unwrap()).unwrap();
            assert_eq!(saved.theme_preference, preference);
            assert!(saved.show_tokens_in_tray);
        }
    }

    #[test]
    fn theme_write_failure_preserves_active_preference() {
        let directory = tempfile::tempdir().unwrap();
        let ctx = AppCtx::new(directory.path().to_path_buf());
        fs::create_dir(&ctx.settings_path).unwrap();
        assert!(ctx.set_theme_preference(ThemePreference::Light).is_err());
        assert_eq!(
            ctx.settings.lock().unwrap().theme_preference,
            ThemePreference::System
        );
    }

    #[test]
    fn region_or_key_change_discards_cached_and_in_flight_zcode_results() {
        use vibe_core::RateLimitStatus;
        let mut cache = RateLimitCache::default();
        let old_request = cache.zcode_generation();
        let mut old_account =
            ProviderRateLimit::empty(RateLimitProvider::ZCode, RateLimitStatus::Ok);
        old_account.plan_label = Some("old account".into());
        cache.store(old_account.clone(), old_request);
        cache.invalidate_zcode();
        assert!(!cache.snapshots.contains_key(&RateLimitProvider::ZCode));
        cache.store(old_account, old_request);
        assert!(!cache.snapshots.contains_key(&RateLimitProvider::ZCode));
        cache.store(
            ProviderRateLimit::empty(RateLimitProvider::Grok, RateLimitStatus::NoData),
            old_request,
        );
        assert!(cache.snapshots.contains_key(&RateLimitProvider::Grok));
        let current_request = cache.zcode_generation();
        cache.store(
            ProviderRateLimit::empty(RateLimitProvider::ZCode, RateLimitStatus::RetryableError),
            current_request,
        );
        assert_eq!(
            cache.snapshots[&RateLimitProvider::ZCode].0.status,
            RateLimitStatus::RetryableError
        );
    }

    #[test]
    fn refresh_failure_replaces_old_success_instead_of_reviving_expired_meters() {
        use vibe_core::RateLimitStatus;
        let mut cache = RateLimitCache::default();
        for provider in [
            RateLimitProvider::KimiCode,
            RateLimitProvider::ZCode,
            RateLimitProvider::Grok,
        ] {
            cache.store(ProviderRateLimit::empty(provider, RateLimitStatus::Ok), 0);
            cache.store(
                ProviderRateLimit::empty(provider, RateLimitStatus::RetryableError),
                0,
            );
            assert_eq!(
                cache.snapshots[&provider].0.status,
                RateLimitStatus::RetryableError
            );
        }
    }

    #[test]
    fn migrates_legacy_provider_toggles_once() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("settings.json"),
            r#"{"codexRateLimitEnabled":false,"claudeRateLimitEnabled":true}"#,
        )
        .unwrap();

        let ctx = AppCtx::new(directory.path().to_path_buf());
        let settings = ctx.settings.lock().unwrap().clone();
        assert!(settings.quota_selection_initialized);
        assert_eq!(
            settings.selected_quota_product_ids,
            vec![RateLimitProvider::ClaudeCode]
        );
    }

    #[test]
    fn preserves_an_intentionally_empty_new_selection() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("settings.json"),
            r#"{"selectedQuotaProductIds":[],"quotaSelectionInitialized":true}"#,
        )
        .unwrap();

        let ctx = AppCtx::new(directory.path().to_path_buf());
        let settings = ctx.settings.lock().unwrap().clone();
        assert!(settings.quota_selection_initialized);
        assert!(settings.selected_quota_product_ids.is_empty());
    }
}
