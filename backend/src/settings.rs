//! 需要持久化的运行时设置
//!
//! 灵动岛、常规、外观与数据保留偏好在各窗口共享同一个值，因此集中放在这里，
//! 由 Rust 持有并落到 JSON，避免前端各存一份导致两处不一致。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// 仅 Windows 自动启动时不显示主面板；灵动岛仍按显示偏好运行。
    #[serde(default = "default_silent_startup")]
    pub silent_startup: bool,
    /// 灵动岛当前显示的平台。与设置里「灵动岛显示配置」是同一个值
    pub island_platform: String,
    #[serde(default = "default_kind")]
    pub island_kind: String,
    /// 灵动岛绑定的具体连接。None 表示尚未选择，不能静默取同平台第一条连接。
    #[serde(default)]
    pub island_connection_id: Option<String>,
    /// 最近一次选择时的连接名。连接被移除后仍用于说明是哪一个失效连接。
    #[serde(default)]
    pub island_connection_name: Option<String>,
    /// 本地统计目前按平台聚合；保留稳定来源 ID，后续支持多来源时无需迁移配置。
    #[serde(default)]
    pub island_source_id: Option<String>,
    #[serde(default = "default_topmost")]
    pub always_on_top: bool,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_opacity")]
    pub island_opacity: u8,
    #[serde(default = "default_scale")]
    pub island_scale: u16,
    /// 停靠条缩放百分比。仅作用于停靠条；探出/展开仍用 island_scale。
    #[serde(default = "default_shrink_scale")]
    pub island_shrink_scale: u16,
    #[serde(default = "default_refresh")]
    pub refresh_minutes: u32,
    #[serde(default)]
    pub balance_alert_threshold: Option<f64>,
    #[serde(default = "default_currency")]
    pub balance_alert_currency: String,
    /// 是否允许拖到屏幕边缘后自动吸附。
    #[serde(default = "default_dock_enabled")]
    pub dock_enabled: bool,
    /// None 表示不自动清理；其余值由设置页显式选择。
    #[serde(default)]
    pub retention_days: Option<u32>,
    /// 免打扰：只暂停提示与动效，**不停止采集与统计**（§2.6）
    pub dnd: bool,
    /// 灵动岛是否可见。隐藏后托盘是唯一入口
    pub island_visible: bool,
    /// 停靠状态：边缘与沿边偏移，重启后恢复（§2.1.3）
    #[serde(default)]
    pub dock: crate::dock::DockState,
    /// 本地代理（阶段二）：默认关闭，不开启时应用保持纯只读采集行为。
    #[serde(default)]
    pub proxy_enabled: bool,
    /// 代理监听端口（仅 127.0.0.1）。
    #[serde(default = "default_proxy_port")]
    pub proxy_port: u16,
    /// 上游连续不可达时自动还原 CLI 直连配置并停止代理。
    #[serde(default = "default_proxy_fallback")]
    pub proxy_fallback_direct: bool,
    /// 接管时保存的 Claude 真实上游（settings.json 原 ANTHROPIC_BASE_URL）。URL 非凭证。
    #[serde(default)]
    pub proxy_claude_upstream: Option<String>,
    /// 接管时保存的 Codex 真实上游（config.toml 原 provider base_url）。
    #[serde(default)]
    pub proxy_codex_upstream: Option<String>,
}

fn default_kind() -> String { "api".into() }
fn default_silent_startup() -> bool { true }
fn default_topmost() -> bool { true }
fn default_theme() -> String { "light".into() }
fn default_opacity() -> u8 { 100 }
fn default_scale() -> u16 { 100 }
fn default_shrink_scale() -> u16 { 100 }
fn default_refresh() -> u32 { 5 }
fn default_currency() -> String { "USD".into() }
fn default_dock_enabled() -> bool { true }
fn default_proxy_port() -> u16 { 12731 }
fn default_proxy_fallback() -> bool { true }

pub fn validate_display_preferences(opacity: u8, scale: u16, shrink_scale: u16, refresh_minutes: u32) -> Result<(), String> {
    if !(60..=100).contains(&opacity) || !matches!(scale, 85 | 100 | 115)
        || !matches!(shrink_scale, 75 | 100 | 125)
        || !matches!(refresh_minutes, 1 | 5 | 15 | 30) {
        return Err("透明度须为 60–100%，大小须为 85/100/115%，缩小后大小须为 75/100/125%，刷新间隔须为 1/5/15/30 分钟".into());
    }
    Ok(())
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            silent_startup: true,
            island_platform: "claude".into(),
            island_kind: default_kind(),
            island_connection_id: None,
            island_connection_name: None,
            island_source_id: None,
            always_on_top: true,
            theme: default_theme(),
            island_opacity: default_opacity(),
            island_scale: default_scale(),
            island_shrink_scale: default_shrink_scale(),
            refresh_minutes: default_refresh(),
            balance_alert_threshold: None,
            balance_alert_currency: default_currency(),
            dock_enabled: true,
            retention_days: None,
            dnd: false,
            island_visible: true,
            dock: Default::default(),
            proxy_enabled: false,
            proxy_port: default_proxy_port(),
            proxy_fallback_direct: default_proxy_fallback(),
            proxy_claude_upstream: None,
            proxy_codex_upstream: None,
        }
    }
}

pub struct Store {
    path: PathBuf,
    inner: Mutex<Settings>,
}

impl Store {
    pub fn load(dir: &std::path::Path) -> Self {
        let path = dir.join("settings.json");
        // 文件损坏或字段缺失时回落到默认值，不让应用起不来
        let inner = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
            .unwrap_or_default();
        Self {
            path,
            inner: Mutex::new(inner),
        }
    }

    pub fn get(&self) -> Settings {
        self.inner.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// 系统窗口回调使用：失败保留原快照并记录日志。
    pub fn update(&self, f: impl FnOnce(&mut Settings)) -> Settings {
        match self.try_update(f) {
            Ok(next) => next,
            Err(error) => {
                eprintln!("[设置] {error}");
                self.get()
            }
        }
    }

    /// 写入成功才发布内存快照；持锁覆盖整个保存过程，防止并发写入顺序颠倒。
    pub fn try_update(&self, f: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
        use std::io::Write;
        let mut guard = self.inner.lock().map_err(|e| format!("设置锁获取失败：{e}"))?;
        let mut next = guard.clone();
        f(&mut next);
        let json = serde_json::to_vec_pretty(&next).map_err(|e| format!("设置序列化失败：{e}"))?;
        let temporary = self.path.with_extension("json.tmp");
        let write = || -> std::io::Result<()> {
            if let Some(parent) = self.path.parent() { std::fs::create_dir_all(parent)?; }
            let mut file = std::fs::File::create(&temporary)?;
            file.write_all(&json)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temporary, &self.path)
        };
        if let Err(error) = write() {
            let _ = std::fs::remove_file(&temporary);
            return Err(format!("设置保存失败：{error}"));
        }
        *guard = next.clone();
        Ok(next)
    }
}

/* ─── 套餐查询辅助凭证（智谱团队版组织/项目 ID、火山 AK/SK）─── */
//
// **刻意不放进 [`Settings`]**：Settings 会整体序列化给前端并参与备份导出，
// 凭证原值不得出现在那里（同 [`crate::connections`] 的凭证纪律）。
// 这里单独落盘 `plan-query.json`，前端只拿到掩码视图 [`PlanQueryStatus`]。

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlanQuerySecrets {
    /// 智谱团队管理后台用量页 URL 中可见（团队版仅国内站）
    #[serde(default)]
    pub zhipu_team_organization_id: String,
    #[serde(default)]
    pub zhipu_team_project_id: String,
    /// 火山控制面 OpenAPI 凭证（与推理 Key 是两套凭据）
    #[serde(default)]
    pub volc_access_key_id: String,
    #[serde(default)]
    pub volc_secret_access_key: String,
}

/// 返回给前端的掩码视图：组织/项目 ID 原样回显（便于确认），AK 掩码、Secret 只回尾 4 位。
#[derive(Debug, Clone, Serialize)]
pub struct PlanQueryStatus {
    pub zhipu_team_organization_id: String,
    pub zhipu_team_project_id: String,
    pub volc_access_key_masked: String,
    pub has_volc_secret: bool,
    pub volc_secret_tail: String,
}

pub struct PlanQueryStore {
    path: PathBuf,
    inner: Mutex<PlanQuerySecrets>,
}

impl PlanQueryStore {
    pub fn load(dir: &std::path::Path) -> Self {
        let path = dir.join("plan-query.json");
        // 文件损坏或字段缺失时回落到默认值，行为与 settings.json 一致
        let inner = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<PlanQuerySecrets>(&s).ok())
            .unwrap_or_default();
        Self { path, inner: Mutex::new(inner) }
    }

    pub fn get(&self) -> PlanQuerySecrets {
        self.inner.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// `update_volc_secret` 的语义：None = 保持不变（前端不必重填），Some(v) = 覆盖（空串=清除）。
    pub fn update(
        &self,
        zhipu_team_organization_id: String,
        zhipu_team_project_id: String,
        volc_access_key_id: String,
        update_volc_secret: Option<String>,
    ) -> Result<PlanQuerySecrets, String> {
        use std::io::Write;
        let mut guard = self.inner.lock().map_err(|e| format!("套餐凭证锁获取失败：{e}"))?;
        let mut next = guard.clone();
        next.zhipu_team_organization_id = zhipu_team_organization_id.trim().to_string();
        next.zhipu_team_project_id = zhipu_team_project_id.trim().to_string();
        next.volc_access_key_id = volc_access_key_id.trim().to_string();
        if let Some(secret) = update_volc_secret {
            next.volc_secret_access_key = secret.trim().to_string();
        }
        let json = serde_json::to_vec_pretty(&next).map_err(|e| format!("套餐凭证序列化失败：{e}"))?;
        let temporary = self.path.with_extension("json.tmp");
        let write = || -> std::io::Result<()> {
            if let Some(parent) = self.path.parent() { std::fs::create_dir_all(parent)?; }
            let mut file = std::fs::File::create(&temporary)?;
            file.write_all(&json)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temporary, &self.path)
        };
        if let Err(error) = write() {
            let _ = std::fs::remove_file(&temporary);
            return Err(format!("套餐凭证保存失败：{error}"));
        }
        *guard = next.clone();
        Ok(next)
    }

    pub fn status(&self) -> PlanQueryStatus {
        let s = self.get();
        let secret = s.volc_secret_access_key.trim();
        PlanQueryStatus {
            zhipu_team_organization_id: s.zhipu_team_organization_id,
            zhipu_team_project_id: s.zhipu_team_project_id,
            volc_access_key_masked: if s.volc_access_key_id.is_empty() {
                String::new()
            } else {
                crate::connections::mask(&s.volc_access_key_id)
            },
            has_volc_secret: !secret.is_empty(),
            volc_secret_tail: if secret.len() >= 4 { secret[secret.len() - 4..].to_string() } else { String::new() },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Settings;

    #[test]
    fn plan_query_store_defaults_when_file_absent_and_masks_secrets() {
        let dir = std::env::temp_dir().join(format!("island-plan-query-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let store = super::PlanQueryStore::load(&dir);
        // 文件缺失 → 全空默认
        assert_eq!(store.get().zhipu_team_organization_id, "");
        assert!(!store.status().has_volc_secret);
        // 写入后：状态视图掩码、原值不外泄
        store.update("org-1".into(), "p-1".into(), "AKIDEXAMPLE123456".into(), Some("SECRETVALUE9876".into())).unwrap();
        let status = store.status();
        assert_eq!(status.zhipu_team_organization_id, "org-1");
        assert_eq!(status.volc_access_key_masked, "AKID****3456");
        assert!(status.has_volc_secret);
        assert_eq!(status.volc_secret_tail, "9876");
        assert!(!serde_json::to_string(&status).unwrap().contains("SECRETVALUE"));
        // Secret 传 None = 保持不变
        store.update("org-2".into(), String::new(), String::new(), None).unwrap();
        assert_eq!(store.get().volc_secret_access_key, "SECRETVALUE9876");
        assert_eq!(store.get().zhipu_team_organization_id, "org-2");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn failed_save_keeps_previous_settings() {
        let dir = std::env::temp_dir().join(format!("island-settings-failure-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(dir.join("settings.json")).unwrap();
        let store = super::Store::load(&dir);
        assert!(store.try_update(|s| s.theme = "dark".into()).is_err());
        assert_eq!(store.get().theme, "light", "失败写入不能伪装成已保存");
        std::fs::remove_dir(dir.join("settings.json")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn settings_roundtrip_preserves_all_preferences() {
        let dir = std::env::temp_dir().join(format!("island-settings-roundtrip-{}", std::process::id()));
        let store = super::Store::load(&dir);
        store.try_update(|s| {
            s.theme = "dark".into(); s.dnd = true; s.dock_enabled = false;
            s.always_on_top = false; s.silent_startup = false;
            s.island_opacity = 70; s.island_scale = 115; s.island_shrink_scale = 125; s.refresh_minutes = 15;
            s.retention_days = Some(90); s.balance_alert_threshold = Some(12.5);
            s.balance_alert_currency = "CNY".into();
            s.island_connection_id = Some("test-connection".into());
            s.island_source_id = Some("local:codex".into()); s.island_platform = "codex".into();
        }).unwrap();
        // 再次替换已有文件，覆盖 Windows 上的保存与重启恢复路径。
        store.try_update(|s| s.theme = "system".into()).unwrap();
        assert_eq!(serde_json::to_value(store.get()).unwrap(), serde_json::to_value(super::Store::load(&dir).get()).unwrap());
        std::fs::remove_file(dir.join("settings.json")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn concurrent_updates_keep_disk_and_memory_in_order() {
        let dir = std::env::temp_dir().join(format!("island-settings-concurrent-{}", std::process::id()));
        let store = std::sync::Arc::new(super::Store::load(&dir));
        let workers: Vec<_> = (0..4).map(|_| {
            let store = store.clone();
            std::thread::spawn(move || {
                for _ in 0..8 { store.try_update(|s| s.dock.offset += 1.0).unwrap(); }
            })
        }).collect();
        for worker in workers { worker.join().unwrap(); }
        assert_eq!(store.get().dock.offset, 32.0);
        assert_eq!(super::Store::load(&dir).get().dock.offset, 32.0);
        std::fs::remove_file(dir.join("settings.json")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn display_preferences_validate_and_old_settings_keep_defaults() {
        assert!(super::validate_display_preferences(60, 85, 100, 1).is_ok());
        assert!(super::validate_display_preferences(100, 115, 125, 30).is_ok());
        for (opacity, scale, shrink, refresh) in [(0, 100, 100, 5), (101, 100, 100, 5), (100, 0, 100, 5), (100, 100, 90, 5), (100, 100, 100, 0)] {
            assert!(super::validate_display_preferences(opacity, scale, shrink, refresh).is_err());
        }
        let old = r#"{"island_platform":"claude","dnd":false,"island_visible":true}"#;
        let settings: Settings = serde_json::from_str(old).unwrap();
        assert!(settings.silent_startup);
        assert_eq!((settings.island_opacity, settings.island_scale, settings.island_shrink_scale, settings.refresh_minutes), (100, 100, 100, 5));
    }

    #[test]
    fn older_settings_keep_docking_enabled_and_optional_connection_name_empty() {
        let settings: Settings = serde_json::from_str(
            r#"{"island_platform":"codex","island_kind":"auth","island_connection_id":null,"island_source_id":null,"always_on_top":true,"dnd":false,"island_visible":true,"dock":{"edge":null,"offset":0.0,"monitor":null}}"#,
        )
        .expect("旧设置应可迁移");
        assert!(settings.dock_enabled);
        assert_eq!(settings.theme, "light");
        assert_eq!(settings.island_connection_name, None);
    }

    #[test]
    fn silent_startup_choice_survives_reload_without_changing_island_visibility() {
        let dir = std::env::temp_dir().join(format!("island-startup-settings-{}", std::process::id()));
        let store = super::Store::load(&dir);
        for on in [false, true] {
            store.update(|settings| settings.silent_startup = on);
            let restored = super::Store::load(&dir).get();
            assert_eq!(restored.silent_startup, on);
            assert!(restored.island_visible);
        }
        std::fs::remove_file(dir.join("settings.json")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
