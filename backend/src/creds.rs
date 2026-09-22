//! 接入方式探测（§6.3）
//!
//! 只判断**本机配置了哪种接入方式**，用于灵动岛与主面板上的
//! 绿色 `Auth` / 蓝色 `API` 连接方式标签。
//!
//! 安全约束：本模块只检查文件是否存在、JSON 里**键是否存在且非空**，
//! 凭证仅在后端读取，候选只返回脱敏标识和配置地址。接入方式只有
//! 官方订阅（Auth）与 API Key 两种，不存在第三种。

use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub struct ConnectionInfo {
    /// "auth" | "api"，未检测到任何配置时为 None
    pub kind: Option<String>,
    /// 界面上的来源说明，不含任何凭证内容
    pub label: Option<String>,
}

pub(crate) fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn read_json(path: &PathBuf) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// 原生平台的凭证格式必须由其专用适配器验证，禁止猜测通用 token 键。
pub(crate) fn is_fetch_only_platform(platform: &str) -> bool { crate::platforms::native(platform) }

/// 本机 Codex Auth 仅在后端用于官方额度只读查询，不落库、不返回前端。
pub fn local_codex_auth() -> Option<(String, Option<String>)> {
    let dir = std::env::var_os("CODEX_HOME").map(PathBuf::from)
        .or_else(|| home().map(|p| p.join(".codex")))?;
    let v = read_json(&dir.join("auth.json"))?;
    Some((str_at(&v, &["tokens", "access_token"])?, str_at(&v, &["tokens", "account_id"])))
}

/// 键存在、非 null、且不是空字符串
fn has_value(v: &Value, key: &str) -> bool {
    match v.get(key) {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) => !s.trim().is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
        Some(_) => true,
    }
}

fn env_set(key: &str) -> bool {
    std::env::var(key).map(|v| !v.trim().is_empty()).unwrap_or(false)
}

/// Claude Code：API Key 优先于订阅（CLI 自身也是这个优先级）
fn claude() -> ConnectionInfo {
    if env_set("ANTHROPIC_API_KEY") || env_set("ANTHROPIC_AUTH_TOKEN") {
        return ConnectionInfo {
            kind: Some("api".into()),
            label: Some("API Key（环境变量）".into()),
        };
    }
    let Some(home) = home() else {
        return ConnectionInfo { kind: None, label: None };
    };

    if let Some(v) = read_json(&home.join(".claude").join("settings.json")) {
        let env_api = v
            .get("env")
            .map(|e| has_value(e, "ANTHROPIC_API_KEY") || has_value(e, "ANTHROPIC_AUTH_TOKEN"))
            .unwrap_or(false);
        if env_api || has_value(&v, "apiKeyHelper") {
            return ConnectionInfo {
                kind: Some("api".into()),
                label: Some("API Key（本机配置）".into()),
            };
        }
    }

    if let Some(v) = read_json(&home.join(".claude").join(".credentials.json")) {
        if has_value(&v, "claudeAiOauth") {
            return ConnectionInfo {
                kind: Some("auth".into()),
                label: Some("官方订阅".into()),
            };
        }
    }
    ConnectionInfo { kind: None, label: None }
}

/// Codex：`~/.codex/auth.json` 同时可能有两者，OPENAI_API_KEY 非空时以 API Key 为准
fn codex() -> ConnectionInfo {
    if env_set("OPENAI_API_KEY") {
        return ConnectionInfo {
            kind: Some("api".into()),
            label: Some("API Key（环境变量）".into()),
        };
    }
    let Some(home) = home() else {
        return ConnectionInfo { kind: None, label: None };
    };
    if let Some(v) = read_json(&home.join(".codex").join("auth.json")) {
        if has_value(&v, "OPENAI_API_KEY") {
            return ConnectionInfo {
                kind: Some("api".into()),
                label: Some("API Key（本机配置）".into()),
            };
        }
        if has_value(&v, "tokens") {
            return ConnectionInfo {
                kind: Some("auth".into()),
                label: Some("官方订阅".into()),
            };
        }
    }
    ConnectionInfo { kind: None, label: None }
}

pub fn detect(platform: &str) -> ConnectionInfo {
    match platform {
        "codex" => codex(),
        "claude" => claude(),
        other if crate::platforms::native(other) => ConnectionInfo {kind: None,label: Some("本机来源监控（非账号授权）".into())},
        _ => ConnectionInfo {kind:None,label:None},
    }
}

/// 取出 JSON 里的字符串值，空串视为缺失
fn str_at(v: &Value, path: &[&str]) -> Option<String> {
    let mut cur = v;
    for k in path {
        cur = cur.get(k)?;
    }
    let s = cur.as_str()?.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// 「获取」按钮的实际读取（§6.3）。
///
/// 这是**唯一**会把凭证原值读进内存的函数，调用方必须立刻脱敏后再落库，
/// 且不得把返回值写进日志或传给前端。
pub fn read_secret(platform: &str, kind: &str) -> Option<String> {
    let home = home()?;
    match (platform, kind) {
        ("claude", "api") => std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                std::env::var("ANTHROPIC_AUTH_TOKEN")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            })
            .or_else(|| {
                let v = read_json(&home.join(".claude").join("settings.json"))?;
                str_at(&v, &["env", "ANTHROPIC_API_KEY"])
                    .or_else(|| str_at(&v, &["env", "ANTHROPIC_AUTH_TOKEN"]))
            }),

        ("claude", _) => {
            let v = read_json(&home.join(".claude").join(".credentials.json"))?;
            str_at(&v, &["claudeAiOauth", "accessToken"])
        }

        ("codex", "api") => std::env::var("OPENAI_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                let v = read_json(&home.join(".codex").join("auth.json"))?;
                str_at(&v, &["OPENAI_API_KEY"])
            }),

        ("codex", _) => {
            let v = read_json(&home.join(".codex").join("auth.json"))?;
            str_at(&v, &["tokens", "access_token"])
        }

        _ => None,
    }
}

/// 本机可自动发现的连接候选。用于「+ 添加连接」第 2 步预填，
/// 让用户不必手抄 Key。**只返回是否存在与脱敏标识，不返回原值。**
#[derive(Debug, Serialize)]
pub struct Candidate {
    pub base_url: Option<String>,
    pub platform: String,
    pub kind: String,
    /// 建议的连接名称，用户可改
    pub suggested_name: String,
    /// 脱敏标识，让用户确认这是自己要的那个账号
    pub masked: String,
}

/// 与本机 Claude Key 配套的网关，避免把网关 Key 发往官方端点。
pub fn local_base_url(platform: &str, kind: &str) -> Option<String> {
    if platform != "claude" || kind != "api" { return None; }
    std::env::var("ANTHROPIC_BASE_URL").ok().filter(|s| !s.trim().is_empty())
        .or_else(|| str_at(&read_json(&home()?.join(".claude/settings.json"))?, &["env", "ANTHROPIC_BASE_URL"]))
        .map(|s| s.trim().trim_end_matches('/').to_string())
}

/// Codex 自定义供应商的网关地址（与 local_connections 同一来源）。
/// 配置缺失或暂时不可解析（写了一半）时返回 None，由调用方跳过本次。
pub fn codex_api_base_url() -> Option<String> {
    let dir = std::env::var_os("CODEX_HOME").map(PathBuf::from)
        .or_else(|| home().map(|p| p.join(".codex")))?;
    let text = std::fs::read_to_string(dir.join("config.toml")).ok()?;
    codex_provider(&text).ok()?.0
}

/* ─────────────────────── Grok CLI 凭证 ─────────────────────── */

/// Grok CLI 的 OAuth 凭证（~/.grok/auth.json）。
/// Ok(token)；Err 为可直接展示给用户的原因。
pub fn local_grok_auth() -> Result<String, String> {
    let path = home().map(|h| h.join(".grok").join("auth.json"))
        .filter(|p| p.exists())
        .ok_or_else(|| "未找到本机 Grok 凭证（~/.grok/auth.json），请先 grok login".to_string())?;
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取 Grok 凭证失败: {e}"))?;
    let now = chrono::Utc::now().timestamp();
    parse_grok_auth_json(&content, now)
}

/// 顶层是 scope → 条目的 map；选 `key` 非空的首选条目（OIDC `https://auth.x.ai::`
/// 前缀优先、legacy `https://accounts.x.ai/sign-in` 兜底），过期视为失效。
/// `now_secs` 注入以便确定性单测。
fn parse_grok_auth_json(content: &str, now_secs: i64) -> Result<String, String> {
    let root: std::collections::BTreeMap<String, serde_json::Value> = serde_json::from_str(content)
        .map_err(|e| format!("Grok auth.json 不是有效 JSON: {e}"))?;
    const OIDC_PREFIX: &str = "https://auth.x.ai::";
    const LEGACY_SCOPE: &str = "https://accounts.x.ai/sign-in";
    let mut oidc = None;
    let mut legacy = None;
    for (scope, value) in &root {
        let key = value.get("key").and_then(|v| v.as_str()).unwrap_or("");
        if key.is_empty() { continue; } // 残缺条目不能遮蔽健康条目
        if scope.starts_with(OIDC_PREFIX) { oidc = Some((key, value)); }
        else if scope == LEGACY_SCOPE || scope.contains("/sign-in") { legacy = Some((key, value)); }
    }
    let (key, entry) = oidc.or(legacy)
        .ok_or_else(|| "Grok auth.json 中没有可用的 access token".to_string())?;
    if let Some(expires_at) = entry.get("expires_at").and_then(|v| v.as_str()) {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(expires_at) {
            if dt.timestamp() < now_secs {
                return Err("Grok OAuth token 已过期，请重新 grok login".into());
            }
        }
    }
    Ok(key.to_string())
}

/// 「更新」与自动同步共用的单次读取：本机当前应用的凭证 + 地址。
/// 与 local_connections（添加）同源，保证两条路径口径一致；
/// 返回 None 表示本机没有可用的该类凭证。
pub fn read_local_pair(platform: &str, kind: &str) -> Option<(String, Option<String>)> {
    let secret = read_secret(platform, kind)?;
    let base_url = match (platform, kind) {
        ("codex", "api") => codex_api_base_url(),
        _ => local_base_url(platform, kind),
    };
    Some((secret, base_url))
}

pub fn discover() -> Vec<Candidate> {
    let mut out = Vec::new();
    for platform in ["claude", "codex"] {
        for kind in ["auth", "api"] {
            // 读到值就立刻脱敏，原值随作用域结束即丢弃
            if let Some(secret) = read_secret(platform, kind) {
                let name = match (platform, kind) {
                    ("claude", "auth") => "Claude 官方订阅",
                    ("claude", _) => "Claude API Key",
                    ("codex", "auth") => "Codex 官方订阅",
                    _ => "Codex API Key",
                };
                out.push(Candidate {
                    base_url: local_base_url(platform, kind),
                    platform: platform.into(),
                    kind: kind.into(),
                    suggested_name: name.into(),
                    masked: crate::connections::mask(&secret),
                });
            }
        }
    }
    out
}

fn codex_provider(text: &str) -> Result<(Option<String>, Option<String>), String> {
    let config: toml::Value = toml::from_str(text).map_err(|_| "Codex config.toml 格式无效，请在 CC Switch 中检查配置")?;
    let name = config.get("model_provider").and_then(toml::Value::as_str).unwrap_or("openai");
    let provider = config.get("model_providers").and_then(|p| p.get(name));
    let string = |key: &str| provider.and_then(|p| p.get(key)).and_then(toml::Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let base = string("base_url");
    if name != "openai" && base.is_none() { return Err("Codex 自定义供应商缺少 base_url，请在 CC Switch 中检查配置".into()); }
    Ok((base, string("env_key")))
}

/// 只读取 CC Switch 已应用到 CLI 的配置，凭证原值不跨越后端边界。
pub fn local_connections(platform: &str, kind: &str) -> Result<Vec<crate::connections::NewConnection>, String> {
    if !matches!(platform, "claude" | "codex") || !matches!(kind, "auth" | "api") {
        return Err("不支持的平台或连接类型".into());
    }
    let mut out = Vec::new();
    if platform == "claude" {
        if let Some(secret) = read_secret("claude", kind) {
            out.push(crate::connections::NewConnection {
                platform: platform.into(), kind: kind.into(), name: if kind == "auth" { "Claude 官方订阅" } else { "Claude API Key" }.into(),
                secret: Some(secret), base_url: local_base_url("claude", kind),
                model: None, effort: None, context_1m: None,
            });
        }
        return Ok(out);
    }
    let dir = std::env::var_os("CODEX_HOME").map(PathBuf::from).or_else(|| home().map(|p| p.join(".codex"))).ok_or("无法确定用户配置目录")?;
    let auth = read_json(&dir.join("auth.json"));
    // Auth 不依赖 API 提供商配置，错误的 API 地址不能阻断订阅凭证读取。
    if kind == "auth" {
        if let Some(secret) = auth.as_ref().and_then(|v| str_at(v, &["tokens", "access_token"])) {
            out.push(crate::connections::NewConnection { platform: platform.into(), kind: kind.into(), name: "Codex 官方订阅".into(), secret: Some(secret), base_url: None, model: None, effort: None, context_1m: None });
        }
        return Ok(out);
    }
    let (base_url, env_key) = match std::fs::read_to_string(dir.join("config.toml")) {
        Ok(text) => codex_provider(&text)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (None, None),
        Err(_) => return Err("无法读取 Codex config.toml".into()),
    };
    let key = std::env::var(env_key.as_deref().unwrap_or("OPENAI_API_KEY")).ok().filter(|s| !s.trim().is_empty())
        .or_else(|| auth.as_ref().and_then(|v| str_at(v, &["OPENAI_API_KEY"])));
    if let Some(secret) = key {
        out.push(crate::connections::NewConnection { platform: platform.into(), kind: kind.into(), name: "Codex API Key".into(), secret: Some(secret), base_url, model: None, effort: None, context_1m: None });
    }
    Ok(out)
}

#[cfg(test)]
mod provider_tests {
    use super::*;
    #[test]
    fn custom_provider_retains_endpoint_and_never_falls_back_to_official() {
        assert_eq!(codex_provider("model_provider='gateway'\n[model_providers.gateway]\nbase_url='https://example.test/v1'\nenv_key='CUSTOM_KEY'").unwrap(), (Some("https://example.test/v1".into()), Some("CUSTOM_KEY".into())));
        assert!(codex_provider("model_provider='gateway'").is_err());
        assert!(codex_provider("[").is_err());
        assert_eq!(codex_provider("").unwrap(), (None, None));
    }

    #[test]
    fn grok_auth_prefers_oidc_entry_and_reports_expiry() {
        assert_eq!(
            parse_grok_auth_json(r#"{
                "https://accounts.x.ai/sign-in": {"key": "legacy-token"},
                "https://auth.x.ai::client-abc": {"key": "oidc-token", "expires_at": "2099-01-01T00:00:00Z"}
            }"#, 0).unwrap(),
            "oidc-token"
        );
        let expired = r#"{"https://auth.x.ai::c": {"key": "t", "expires_at": "2000-01-01T00:00:00Z"}}"#;
        assert!(parse_grok_auth_json(expired, 1_500_000_000).unwrap_err().contains("过期"));
        let only_legacy = r#"{"https://accounts.x.ai/sign-in": {"key": "legacy"}}"#;
        assert_eq!(parse_grok_auth_json(only_legacy, 0).unwrap(), "legacy");
        // key 为空的残缺 OIDC 条目不遮蔽健康条目
        let broken = r#"{"https://auth.x.ai::c": {}, "https://accounts.x.ai/sign-in": {"key": "ok"}}"#;
        assert_eq!(parse_grok_auth_json(broken, 0).unwrap(), "ok");
        assert!(parse_grok_auth_json("{}", 0).is_err());
        assert!(parse_grok_auth_json("not-json", 0).is_err());
    }
}

/// 界面需要说明「扫描了哪些路径与环境变量」，不静默读取用户未预期的位置（§6.3）
pub fn scanned_locations(platform: &str, kind: &str) -> Vec<String> {
    let h = home()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "~".into());
    match (platform, kind) {
        ("claude", "api") => vec![
            "环境变量 ANTHROPIC_API_KEY".into(),
            "环境变量 ANTHROPIC_AUTH_TOKEN".into(),
            format!("{h}\\.claude\\settings.json 的 env 段"),
        ],
        ("claude", _) => vec![format!("{h}\\.claude\\.credentials.json")],
        ("codex", "api") => vec![
            "环境变量 OPENAI_API_KEY".into(),
            format!("{h}\\.codex\\auth.json"),
        ],
        ("codex", _) => vec![format!("{h}\\.codex\\auth.json")],
        _ => vec![],
    }
}
