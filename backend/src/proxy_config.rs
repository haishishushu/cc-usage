//! CLI 本机配置的接管与还原（阶段二本地代理）。
//!
//! 原则：
//! - **只在网关模式下接管**：Claude 要求 `settings.json` 已有 `env.ANTHROPIC_BASE_URL`，
//!   Codex 要求 `config.toml` 已有自定义 provider 的 `base_url`；官方订阅直连一律不动
//!   （CLI 对「自定义 base_url + OAuth」的兼容性未验证，Codex 官方 auth 还涉及凭证注入）。
//! - **原值保存在应用设置里**（`proxy_*_upstream`），接管写代理地址，还原写回原值。
//! - **还原只清理自己的写入**：当前值不再是本机代理地址（外部已改，如 CC Switch 切换）
//!   时不覆盖，外部意图优先。
//! - **写入统一「临时文件 + rename」原子替换**；Codex 的 TOML 用 [`toml_edit`]
//!   保留注释与格式，Claude 的 JSON 用 `serde_json::Value` 往返保留全部字段。
//!
//! 路径解析依赖真实用户目录，难以单测；文件读写拆成纯函数（`*_read` / `*_write`），
//! 在 `tests` 里用临时文本验证保留性与边界。

use std::path::PathBuf;

use tauri::Manager;

/// 代理支持接管的两类 CLI 平台。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProxyPlatform {
    Claude,
    Codex,
}

impl ProxyPlatform {
    pub fn display_name(self) -> &'static str {
        match self {
            ProxyPlatform::Claude => "Claude",
            ProxyPlatform::Codex => "Codex",
        }
    }
}

/// 判断地址是否为本机代理自身（用于区分「自己写的」与「外部配置」）。
pub(crate) fn is_self_base(url: &str, port: u16) -> bool {
    url.trim().trim_end_matches('/') == format!("http://127.0.0.1:{port}")
}

fn claude_settings_path() -> Option<PathBuf> {
    let home = crate::creds::home()?;
    let dir = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"));
    Some(dir.join("settings.json"))
}

fn codex_config_path() -> Option<PathBuf> {
    crate::session_titles::codex_home()
        .or_else(|| crate::creds::home().map(|home| home.join(".codex")))
        .map(|dir| dir.join("config.toml"))
}

fn write_atomic(path: &std::path::Path, bytes: Vec<u8>) -> Result<(), String> {
    use std::io::Write;
    let temporary = path.with_extension("proxy.tmp");
    let write = || -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)
    };
    write().map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        format!("写入 {} 失败：{error}", path.display())
    })
}

/* ─────────────────────────── Claude：settings.json 的 env 段 ─────────────────────────── */

/// 读取当前 `env.ANTHROPIC_BASE_URL`（只看文件，不读环境变量——环境变量场景不接管）。
pub(crate) fn read_current_base(platform: ProxyPlatform) -> Result<Option<String>, String> {
    match platform {
        ProxyPlatform::Claude => {
            let Some(path) = claude_settings_path() else {
                return Err("未找到用户主目录".into());
            };
            let text = std::fs::read_to_string(&path)
                .map_err(|error| format!("读取 {} 失败：{error}", path.display()))?;
            claude_read_base(&text)
        }
        ProxyPlatform::Codex => {
            let Some(path) = codex_config_path() else {
                return Err("未找到用户主目录".into());
            };
            let text = std::fs::read_to_string(&path)
                .map_err(|error| format!("读取 {} 失败：{error}", path.display()))?;
            Ok(codex_read_base(&text)?.1)
        }
    }
}

fn claude_read_base(text: &str) -> Result<Option<String>, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|error| format!("settings.json 不是有效 JSON：{error}"))?;
    Ok(value
        .get("env")
        .and_then(|env| env.get("ANTHROPIC_BASE_URL"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string))
}

/// `value = Some(url)` 写入 / `None` 删除 `env.ANTHROPIC_BASE_URL`，保留其余字段。
fn claude_write_base(text: &str, value: Option<&str>) -> Result<String, String> {
    let mut root: serde_json::Value =
        serde_json::from_str(text).map_err(|error| format!("settings.json 不是有效 JSON：{error}"))?;
    let Some(object) = root.as_object_mut() else {
        return Err("settings.json 顶层不是对象".into());
    };
    let env = object
        .entry("env")
        .or_insert_with(|| serde_json::json!({}));
    let Some(env) = env.as_object_mut() else {
        return Err("settings.json 的 env 段不是对象".into());
    };
    match value {
        Some(url) => {
            env.insert("ANTHROPIC_BASE_URL".into(), serde_json::Value::String(url.to_string()));
        }
        None => {
            env.remove("ANTHROPIC_BASE_URL");
        }
    }
    serde_json::to_vec_pretty(&root)
        .map(|bytes| String::from_utf8(bytes).expect("JSON 序列化必为 UTF-8"))
        .map_err(|error| format!("settings.json 序列化失败：{error}"))
}

fn write_claude_file(value: Option<&str>) -> Result<(), String> {
    let Some(path) = claude_settings_path() else {
        return Err("未找到用户主目录".into());
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("读取 {} 失败：{error}", path.display()))?;
    let next = claude_write_base(&text, value)?;
    write_atomic(&path, next.into_bytes())
}

/* ─────────────────────────── Codex：config.toml 的 model_providers ─────────────────────────── */

/// 返回 (当前 model_provider 名, 该 provider 的 base_url)。
fn codex_read_base(text: &str) -> Result<(Option<String>, Option<String>), String> {
    let document = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("config.toml 格式无效：{error}"))?;
    let provider = document.get("model_provider").and_then(|item| item.as_str()).map(str::to_string);
    let base = provider
        .as_deref()
        .and_then(|name| document.get("model_providers").and_then(|item| item.get(name)))
        .and_then(|item| item.get("base_url"))
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Ok((provider, base))
}

/// 修改指定 provider 的 `base_url`（`None` = 删除该字段），保留注释与格式。
fn codex_write_base(text: &str, provider: &str, value: Option<&str>) -> Result<String, String> {
    let mut document = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("config.toml 格式无效：{error}"))?;
    let Some(table) = document
        .get_mut("model_providers")
        .and_then(|item| item.as_table_mut())
        .and_then(|table| table.get_mut(provider))
        .and_then(|item| item.as_table_mut())
    else {
        return Err(format!("config.toml 缺少 [model_providers.{provider}] 段"));
    };
    match value {
        Some(url) => {
            table["base_url"] = toml_edit::value(url);
        }
        None => {
            table.remove("base_url");
        }
    }
    Ok(document.to_string())
}

/// 当前生效的 provider 名（还原时按它写回原 provider 的 base_url）。
fn codex_provider_name(text: &str) -> Result<String, String> {
    codex_read_base(text)?
        .0
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| "config.toml 未设置 model_provider（官方直连），无法定位要还原的 provider".into())
}

/* ─────────────────────────── 接管 / 还原的对外入口（proxy.rs 调用） ─────────────────────────── */

/// 探测某平台是否可接管，返回要保存的原始上游地址；不可接管时给出如实原因。不写文件。
pub(crate) fn probe_original(platform: ProxyPlatform) -> Result<String, String> {
    match platform {
        ProxyPlatform::Claude => {
            if std::env::var("ANTHROPIC_BASE_URL")
                .ok()
                .is_some_and(|value| !value.trim().is_empty())
            {
                // 环境变量优先级高于 settings.json，接管文件字段无法覆盖它
                return Err("检测到环境变量 ANTHROPIC_BASE_URL，暂不支持接管环境变量方式的上游".into());
            }
            let Some(path) = claude_settings_path() else {
                return Err("未找到用户主目录".into());
            };
            let text = std::fs::read_to_string(&path)
                .map_err(|error| format!("读取 {} 失败：{error}", path.display()))?;
            claude_read_base(&text)?.ok_or_else(|| {
                "官方订阅直连（无网关 base_url）暂不支持接管，待实测 CLI 兼容性后开放".to_string()
            })
        }
        ProxyPlatform::Codex => {
            let Some(path) = codex_config_path() else {
                return Err("未找到用户主目录".into());
            };
            let text = std::fs::read_to_string(&path)
                .map_err(|error| format!("读取 {} 失败：{error}", path.display()))?;
            let (provider, base) = codex_read_base(&text)?;
            let provider = provider.ok_or_else(|| {
                "官方订阅直连（无自定义 provider）暂不支持接管：自定义 provider 不使用 auth.json 的 OAuth 凭证，需另行验证".to_string()
            })?;
            base.ok_or_else(|| format!("自定义 provider「{provider}」缺少 base_url，无法接管"))
        }
    }
}

/// 对设置里已保存上游的平台，把 CLI 配置改写为代理地址（接管动作）。
pub(crate) fn apply_takeover(app: &tauri::AppHandle, proxy_base: &str) -> Result<(), String> {
    let settings = app.state::<crate::Cfg>().0.get();
    let mut errors = Vec::new();
    if settings.proxy_claude_upstream.as_deref().is_some_and(|s| !s.trim().is_empty()) {
        if let Err(error) = write_claude_file(Some(proxy_base)) {
            errors.push(format!("Claude：{error}"));
        }
    }
    if settings.proxy_codex_upstream.as_deref().is_some_and(|s| !s.trim().is_empty()) {
        if let Err(error) = write_codex_current_provider(Some(proxy_base)) {
            errors.push(format!("Codex：{error}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("；"))
    }
}

/// 按 config.toml 当前的 model_provider 写 base_url。
fn write_codex_current_provider(value: Option<&str>) -> Result<(), String> {
    let Some(path) = codex_config_path() else {
        return Err("未找到用户主目录".into());
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("读取 {} 失败：{error}", path.display()))?;
    let provider = codex_provider_name(&text)?;
    let next = codex_write_base(&text, &provider, value)?;
    write_atomic(&path, next.into_bytes())
}

/// 供 on_config_changed 使用：直接把某平台的 base 写为代理地址（外部切换后重写）。
pub(crate) fn write_proxy_base(platform: ProxyPlatform, proxy_base: &str) -> Result<(), String> {
    match platform {
        ProxyPlatform::Claude => write_claude_file(Some(proxy_base)),
        ProxyPlatform::Codex => write_codex_current_provider(Some(proxy_base)),
    }
}

/// 还原某平台。`saved = Some(原始地址)` 写回；`None` 删除字段（外部已切官方直连的清理）。
/// 仅当当前值仍指向本机代理端口时才写：外部已改成别的地址时不覆盖，外部意图优先。
pub(crate) fn restore_platform(platform: ProxyPlatform, saved: Option<&str>, port: u16) -> Result<(), String> {
    let current = read_current_base(platform)?;
    let Some(current) = current else {
        return Ok(());
    };
    if !is_self_base(&current, port) {
        return Ok(());
    }
    match platform {
        ProxyPlatform::Claude => write_claude_file(saved),
        ProxyPlatform::Codex => write_codex_current_provider(saved),
    }
}

/// 还原设置里保存的全部平台（停止 / 回退 / 退出时调用）。
pub(crate) fn restore_all_saved(app: &tauri::AppHandle) -> Result<(), String> {
    let settings = app.state::<crate::Cfg>().0.get();
    let port = settings.proxy_port;
    let mut errors = Vec::new();
    if let Some(saved) = settings.proxy_claude_upstream.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if let Err(error) = restore_platform(ProxyPlatform::Claude, Some(saved), port) {
            errors.push(format!("Claude：{error}"));
        }
    }
    if let Some(saved) = settings.proxy_codex_upstream.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if let Err(error) = restore_platform(ProxyPlatform::Codex, Some(saved), port) {
            errors.push(format!("Codex：{error}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("；"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_base_write_keeps_other_fields_and_round_trips() {
        let original = r#"{
  "model": "claude-sonnet-4",
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "sk-test",
    "ANTHROPIC_BASE_URL": "https://gateway.example.com"
  },
  "includeCoAuthoredBy": false
}"#;
        assert_eq!(
            claude_read_base(original).unwrap().as_deref(),
            Some("https://gateway.example.com")
        );
        // 接管：只改 base_url，其余字段原样保留
        let taken = claude_write_base(original, Some("http://127.0.0.1:12731")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&taken).unwrap();
        assert_eq!(value["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:12731");
        assert_eq!(value["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-test");
        assert_eq!(value["includeCoAuthoredBy"], false);
        // 还原：写回原值
        let restored = claude_write_base(&taken, Some("https://gateway.example.com")).unwrap();
        assert_eq!(claude_read_base(&restored).unwrap().as_deref(), Some("https://gateway.example.com"));
    }

    #[test]
    fn codex_base_write_preserves_comments_and_tables() {
        let original = "# 由 CC Switch 管理\nmodel_provider = 'gateway'\nmodel = \"gpt-5\"\n\n[model_providers.gateway]\nname = 'My gateway'\nbase_url = 'https://gw.example.com/v1' # 网关地址\nenv_key = 'GATEWAY_KEY'\n\n[profiles.fast]\nmodel_provider = 'gateway'\n";
        let (provider, base) = codex_read_base(original).unwrap();
        assert_eq!(provider.as_deref(), Some("gateway"));
        assert_eq!(base.as_deref(), Some("https://gw.example.com/v1"));
        // 接管：注释与其它键保留，仅 base_url 变化
        let taken = codex_write_base(original, "gateway", Some("http://127.0.0.1:12731")).unwrap();
        assert!(taken.contains("# 由 CC Switch 管理"), "顶层注释必须保留");
        assert!(taken.contains("env_key = 'GATEWAY_KEY'"), "其他键必须保留");
        assert!(taken.contains("http://127.0.0.1:12731"));
        assert!(!taken.contains("gw.example.com"));
        // 还原：写回原地址
        let restored = codex_write_base(&taken, "gateway", Some("https://gw.example.com/v1")).unwrap();
        let (_, base) = codex_read_base(&restored).unwrap();
        assert_eq!(base.as_deref(), Some("https://gw.example.com/v1"));
    }

    #[test]
    fn codex_official_direct_and_missing_provider_are_reported_not_guessed() {
        // 官方直连：没有 model_provider 与 model_providers 段
        let official = "model = \"gpt-5\"\n";
        let (provider, base) = codex_read_base(official).unwrap();
        assert_eq!(provider, None);
        assert_eq!(base, None);
        // provider 存在但段缺失：写入时报错，不静默创建
        let broken = "model_provider = 'missing'\n";
        assert!(codex_write_base(broken, "missing", Some("http://127.0.0.1:1")).is_err());
    }

    #[test]
    fn self_base_detection_only_matches_local_proxy() {
        assert!(is_self_base("http://127.0.0.1:12731", 12731));
        assert!(!is_self_base("https://127.0.0.1:12731", 12731), "https 不是本机代理写法");
        assert!(!is_self_base("http://localhost:12731", 12731), "localhost 写法不是我们写入的");
        assert!(!is_self_base("http://127.0.0.1:12731/v1", 12731));
    }
}
