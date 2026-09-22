//! 连接管理（§6.3）
//!
//! 连接只有两种：官方订阅（`auth`）与 API Key（`api`）。
//! **本地会话记录不是连接**——它是统计来源，由后台自动发现，不入此表。
//!
//! ## 凭证纪律
//!
//! `secret` 只在后端查额度/余额时读取；所有跨进程边界的结构体（`ConnectionDto`）
//! 只带 `masked`，前端拿不到原值，日志也不打印原值。
//! 脱敏格式保留前缀与末 4 位，足以区分多个 Key 又不泄露内容。

use rusqlite::{params, Connection as Sql, OptionalExtension};
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod manual_models_tests {
    use super::*;

    #[test]
    fn models_endpoint_normalizes_base_and_rejects_non_http() {
        assert_eq!(models_endpoint("https://gw.test", None).unwrap(), "https://gw.test/v1/models");
        assert_eq!(models_endpoint("https://gw.test/", None).unwrap(), "https://gw.test/v1/models");
        assert_eq!(models_endpoint("https://gw.test/v1", None).unwrap(), "https://gw.test/v1/models");
        assert_eq!(models_endpoint("https://gw.test/v1/", Some("claude-sonnet-4-5")).unwrap(), "https://gw.test/v1/models/claude-sonnet-4-5");
        assert!(models_endpoint("ftp://gw.test", None).is_err());
        assert!(models_endpoint("not a url", None).is_err());
    }
}

#[cfg(test)]
mod gateway_tests {
    use super::*;

    #[test]
    fn official_base_urls_are_not_misrouted_to_gateway_usage() {
        for (platform, host) in [("claude", "api.anthropic.com"), ("codex", "api.openai.com")] {
            assert_eq!(normalized_base(platform, "api", Some(format!("https://{host}/v1"))), None);
            let custom = format!("https://{host}.example.test/v1");
            assert_eq!(normalized_base(platform, "api", Some(custom.clone())), Some(custom));
            assert_eq!(normalized_base(platform, "auth", Some("https://gateway.test".into())), None);
        }
    }

    #[test]
    fn local_registration_is_deduplicated_and_does_not_enable_connections() {
        let db = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        let input = |key: &str| NewConnection { platform: "claude".into(), kind: "api".into(), name: "local".into(), secret: Some(key.into()), base_url: Some("https://example.test".into()), model: None, effort: None, context_1m: None };
        let (id, added) = register_local(&db, input("sk-first-1234")).unwrap();
        assert!(added);
        assert!(selectable_creds(&db, &id).is_err());
        assert_eq!(register_local(&db, input("sk-first-1234")).unwrap(), (id.clone(), false));
        let (other, added) = register_local(&db, input("sk-other-1234")).unwrap();
        assert!(added); assert_ne!(other, id);
        assert_eq!(list(&db).unwrap().len(), 2);
        set_paused(&db, &id, false).unwrap();
        register_local(&db, input("sk-first-1234")).unwrap();
        assert!(selectable_creds(&db, &id).is_ok());
        set_paused(&db, &id, true).unwrap();
        register_local(&db, input("sk-first-1234")).unwrap();
        assert!(creds_of(&db, &id).unwrap().unwrap().paused);
    }

    #[test]
    fn paused_connection_cannot_be_reactivated_by_late_query_results() {
        let db = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        let id = add(&db, NewConnection {
            platform: "claude".into(), kind: "api".into(), name: "pause test".into(),
            secret: Some("synthetic-pause-key".into()), base_url: None,
            model: None, effort: None, context_1m: None,
        }).unwrap();
        db.execute("UPDATE connections SET status = 'paused' WHERE id = ?1", [&id]).unwrap();
        touch_sync(&db, &id).unwrap();
        assert_eq!(list(&db).unwrap()[0].status, "paused");
        assert!(!record_query_status(&db, &id, true).unwrap());
        assert_eq!(list(&db).unwrap()[0].status, "paused");
        set_paused(&db, &id, false).unwrap();
        assert!(record_query_status(&db, &id, false).unwrap());
        assert!(!record_query_status(&db, &id, false).unwrap());
        assert!(record_query_status(&db, &id, true).unwrap());
        set_paused(&db, &id, true).unwrap();
        mark_expired(&db, &id).unwrap();
        assert_eq!(list(&db).unwrap()[0].status, "paused");
    }

    #[test]
    fn pause_preserves_saved_credentials_and_blocks_queries_until_resumed() {
        let db = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        let id = add(&db, NewConnection {
            platform: "codex".into(), kind: "api".into(), name: "saved connection".into(),
            secret: Some("synthetic-preserved-key".into()), base_url: Some("https://example.test".into()),
            model: None, effort: None, context_1m: None,
        }).unwrap();
        set_paused(&db, &id, true).unwrap();
        assert!(query_creds(&db, &id).is_err());
        let saved = creds_of(&db, &id).unwrap().unwrap();
        assert!(saved.paused);
        assert_eq!(saved.secret.as_deref(), Some("synthetic-preserved-key"));
        assert_eq!(saved.base_url.as_deref(), Some("https://example.test"));
        assert!(!fetch_local(&db, &id).unwrap().ok);
        assert_eq!(list(&db).unwrap().len(), 1);
        set_paused(&db, &id, false).unwrap();
        assert!(query_creds(&db, &id).unwrap().is_some());
        assert_eq!(list(&db).unwrap()[0].status, "connected");
        assert!(set_paused(&db, "missing", true).is_err());
    }

    #[test]
    fn official_native_keys_require_explicit_credentials_and_supported_platform() {
        let input=|platform: &str, kind: &str, secret: Option<&str>| NewConnection {
            platform:platform.into(), kind:kind.into(), name:"official test".into(),
            secret:secret.map(str::to_string),base_url:None,model:None,effort:None,context_1m:None,
        };
        for platform in ["gemini","grok"] {
            assert!(prepare_new(input(platform,"api",Some("synthetic-key"))).is_ok());
            assert!(prepare_new(input(platform,"api",None)).is_err());
            assert!(prepare_new(input(platform,"auth",Some("synthetic-key"))).is_err());
        }
        assert!(prepare_new(input("trae","api",Some("synthetic-key"))).is_err());
    }

    #[test]
    fn gateway_accepts_http_and_https_domains_but_rejects_other_schemes() {
        let connection = |base: &str| NewConnection {
            platform: "claude".into(), kind: "api".into(), name: "HTTP gateway test".into(),
            secret: Some("synthetic-test-key".into()), base_url: Some(base.into()),
            model: None, effort: None, context_1m: None,
        };
        for base in ["http://api.example.com", "http://192.168.1.2:8080", "http://localhost:8000", "https://api.example.com", "HTTP://api.example.com"] {
            assert!(prepare_new(connection(base)).is_ok(), "should accept {base}");
        }
        for base in ["ftp://api.example.com", "file:///tmp/key", "not-a-url", "http://"] {
            assert!(prepare_new(connection(base)).is_err(), "should reject {base}");
        }
    }
}

#[cfg(test)]
mod rename_tests {
    use super::*;

    fn db() -> rusqlite::Connection {
        crate::db::open(std::path::Path::new(":memory:")).unwrap()
    }

    fn claude_api(name: &str, secret: &str) -> NewConnection {
        NewConnection {
            platform: "claude".into(),
            kind: "api".into(),
            name: name.into(),
            secret: Some(secret.into()),
            base_url: None,
            model: None,
            effort: None,
            context_1m: None,
        }
    }

    #[test]
    fn rename_updates_name_only_and_keeps_credentials() {
        let db = db();
        let id = add(&db, claude_api("旧名称", "sk-rename-1234")).unwrap();
        rename_connection(&db, &id, "  新名称  ").unwrap();
        let row = &list(&db).unwrap()[0];
        assert_eq!(row.name, "新名称");
        // 改名只动名称：凭证脱敏标识与状态保持不变
        assert_eq!(row.masked, mask("sk-rename-1234"));
        assert_eq!(row.status, "connected");
        // 改名是纯元数据操作，断开的连接同样允许改名
        set_paused(&db, &id, true).unwrap();
        rename_connection(&db, &id, "断开也能改名").unwrap();
        assert_eq!(list(&db).unwrap()[0].name, "断开也能改名");
    }

    #[test]
    fn rename_rejects_blank_overlong_and_missing_connections() {
        let db = db();
        let id = add(&db, claude_api("原名", "sk-rename-5678")).unwrap();
        assert!(rename_connection(&db, &id, "   ").is_err());
        assert!(rename_connection(&db, &id, &"长".repeat(81)).is_err());
        assert!(rename_connection(&db, "missing", "任意").is_err());
        // 校验失败后原名保持不变
        assert_eq!(list(&db).unwrap()[0].name, "原名");
    }

    #[test]
    fn custom_name_overrides_local_inputs_only_when_valid() {
        let input = |name: &str| NewConnection {
            platform: "claude".into(),
            kind: "api".into(),
            name: name.into(),
            secret: Some("sk-custom-1234".into()),
            base_url: None,
            model: None,
            effort: None,
            context_1m: None,
        };
        // 用户填了名称：覆盖自动生成名，并去首尾空白
        let renamed = with_custom_name(vec![input("Claude API Key")], Some("  我的连接  ".into())).unwrap();
        assert_eq!(renamed[0].name, "我的连接");
        // 只填空白：视为未填，保留自动生成名
        let kept = with_custom_name(vec![input("Claude API Key")], Some("   ".into())).unwrap();
        assert_eq!(kept[0].name, "Claude API Key");
        // 未填：原样返回
        let none = with_custom_name(vec![input("Claude API Key")], None).unwrap();
        assert_eq!(none[0].name, "Claude API Key");
        // 超长名称：整体拒绝，不让半合法输入落到库里
        assert!(with_custom_name(vec![input("Claude API Key")], Some("长".repeat(81))).is_err());
    }
}

#[cfg(test)]
mod local_sync_tests {
    use super::*;

    fn db() -> rusqlite::Connection {
        crate::db::open(std::path::Path::new(":memory:")).unwrap()
    }

    fn claude_api(name: &str, secret: &str, base: Option<&str>) -> NewConnection {
        NewConnection {
            platform: "claude".into(),
            kind: "api".into(),
            name: name.into(),
            secret: Some(secret.into()),
            base_url: base.map(str::to_string),
            model: None,
            effort: None,
            context_1m: None,
        }
    }

    #[test]
    fn update_refreshes_secret_and_base_url_in_place() {
        let db = db();
        let id = add(&db, claude_api("旧中转", "sk-old-1234", Some("https://old.example.test"))).unwrap();
        let result = apply_local(
            &db,
            &id,
            Some(LocalPair { secret: "sk-new-4567".into(), base_url: Some("https://new.example.test".into()) }),
            vec![],
        )
        .unwrap();
        assert!(result.ok && result.changed, "{result:?}");
        assert_eq!(result.masked.as_deref(), Some("sk-****4567"));
        let row = &list(&db).unwrap()[0];
        assert_eq!(row.masked, "sk-****4567");
        assert_eq!(row.base_url.as_deref(), Some("https://new.example.test"));
        assert_eq!(row.status, "connected");
    }

    #[test]
    fn update_reports_no_change_when_local_matches_after_normalization() {
        let db = db();
        let id = add(&db, claude_api("同一中转", "sk-same-1234", Some("https://same.example.test"))).unwrap();
        let result = apply_local(
            &db,
            &id,
            Some(LocalPair { secret: "sk-same-1234".into(), base_url: Some("https://same.example.test/".into()) }),
            vec![],
        )
        .unwrap();
        assert!(result.ok && !result.changed, "{result:?}");
    }

    #[test]
    fn update_without_local_secret_keeps_connection_untouched() {
        let db = db();
        let id = add(&db, claude_api("不变化", "sk-keep-1234", Some("https://keep.example.test"))).unwrap();
        let result = apply_local(&db, &id, None, vec![]).unwrap();
        assert!(!result.ok && !result.changed, "{result:?}");
        let row = &list(&db).unwrap()[0];
        assert_eq!(row.masked, mask("sk-keep-1234"));
        assert_eq!(row.base_url.as_deref(), Some("https://keep.example.test"));
    }

    #[test]
    fn update_refuses_when_another_same_kind_connection_holds_a_different_key() {
        let db = db();
        add(&db, claude_api("A", "sk-aaa-1111", None)).unwrap();
        let b = add(&db, claude_api("B", "sk-bbb-2222", None)).unwrap();
        let result = apply_local(&db, &b, Some(LocalPair { secret: "sk-ccc-3333".into(), base_url: None }), vec![]).unwrap();
        assert!(!result.ok && !result.changed, "{result:?}");
        let row = list(&db).unwrap().into_iter().find(|c| c.id == b).unwrap();
        assert_eq!(row.masked, mask("sk-bbb-2222"));
    }

    #[test]
    fn sync_updates_the_unambiguous_connection_and_reports_changed() {
        let db = db();
        add(&db, claude_api("Claude API Key", "sk-old-9999", None)).unwrap();
        let changed = sync_from_local_with(&db, &|platform, kind| {
            matches!((platform, kind), ("claude", "api"))
                .then(|| LocalPair { secret: "sk-new-8888".into(), base_url: None })
        })
        .unwrap();
        assert!(changed);
        assert_eq!(list(&db).unwrap()[0].masked, mask("sk-new-8888"));
    }

    #[test]
    fn sync_skips_when_local_missing_ambiguous_or_paused() {
        let db = db();
        let local = |secret: &str| Some(LocalPair { secret: secret.into(), base_url: None });
        let a = add(&db, claude_api("A", "sk-aaa-1111", None)).unwrap();
        // 本机没读到：不动
        assert!(!sync_from_local_with(&db, &|_, _| None).unwrap());
        // 同类型两条：分不清本机配置属于哪条，不动
        let b = add(&db, claude_api("B", "sk-bbb-2222", None)).unwrap();
        assert!(!sync_from_local_with(&db, &|_, _| local("sk-new-3333")).unwrap());
        // 暂停的连接不参与，也不会因此误更新另一条
        set_paused(&db, &b, true).unwrap();
        assert!(!sync_from_local_with(&db, &|_, _| local("sk-new-4444")).unwrap());
        assert_eq!(creds_of(&db, &a).unwrap().unwrap().secret.as_deref(), Some("sk-aaa-1111"));
        assert_eq!(creds_of(&db, &b).unwrap().unwrap().secret.as_deref(), Some("sk-bbb-2222"));
    }
}

/// 返回给前端的连接。**不含** secret 字段——这是刻意的，不要加。
#[derive(Debug, Serialize)]
pub struct ConnectionDto {
    pub id: String,
    pub platform: String,
    pub kind: String,
    pub name: String,
    pub label: String,
    /// 脱敏标识，如 `sk-ant-****3f9a`；Auth 连接为套餐名或空
    pub masked: String,
    pub status: String,
    /// 相对时间文案，如「刚刚」「12 秒前」；从未同步过为 None
    pub last_sync_text: Option<String>,
    /// 自建网关（sub2api）的部署地址；官方直连为 None。
    /// 地址不是凭证，可以回显，便于用户核对填错没有
    pub base_url: Option<String>,
    /// 手动添加时保存的默认参数：模型 / 思考强度 / 1M 上下文。
    /// 本机读取的连接与历史连接为 None；参数随连接保存，供请求与成本估算使用
    pub model: Option<String>,
    pub effort: Option<String>,
    pub context_1m: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct NewConnection {
    pub platform: String,
    pub kind: String,
    pub name: String,
    /// 仅 kind = "api" 时有值。落库后前端不再回读
    pub secret: Option<String>,
    /// sub2api 等自建网关的部署地址；官方直连留空
    pub base_url: Option<String>,
    /// 手动添加的可选默认参数；缺失时为 None
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub context_1m: Option<bool>,
}

/// 脱敏：保留可识别的前缀与末 4 位，中间固定 4 个星号。
/// 太短的 Key（异常输入）整体打码，避免反而暴露大部分内容。
pub fn mask(secret: &str) -> String {
    let s = secret.trim();
    if s.len() <= 8 {
        return "****".into();
    }
    // 常见前缀 sk-ant- / sk- 保留，便于用户辨认是哪一类 Key
    let prefix_len = if s.starts_with("sk-ant-") {
        7
    } else if s.starts_with("sk-") {
        3
    } else {
        4
    };
    let head: String = s.chars().take(prefix_len).collect();
    let tail: String = s.chars().skip(s.chars().count().saturating_sub(4)).collect();
    format!("{head}****{tail}")
}

/// 毫秒时间戳 → 相对时间文案（§6.3 的「最近同步」列）
fn relative(ms: i64, now: i64) -> String {
    let d = (now - ms).max(0) / 1000;
    match d {
        0..=4 => "刚刚".into(),
        5..=59 => format!("{d} 秒前"),
        60..=3599 => format!("{} 分钟前", d / 60),
        3600..=86399 => format!("{} 小时前", d / 3600),
        _ => format!("{} 天前", d / 86400),
    }
}

fn row_to_dto(r: &rusqlite::Row, now: i64) -> rusqlite::Result<ConnectionDto> {
    let last: Option<i64> = r.get("last_sync_ms")?;
    let platform: String = r.get("platform")?;
    let kind: String = r.get("kind")?;
    let base_url = normalized_base(&platform, &kind, r.get("base_url")?);
    Ok(ConnectionDto {
        id: r.get("id")?,
        platform: r.get("platform")?,
        kind: r.get("kind")?,
        name: r.get("name")?,
        label: r.get("label")?,
        masked: r.get("masked")?,
        status: r.get("status")?,
        last_sync_text: last.map(|m| relative(m, now)),
        base_url,
        model: r.get("model")?,
        effort: r.get("effort")?,
        context_1m: r.get("context_1m")?,
    })
}

pub fn list(conn: &Sql) -> rusqlite::Result<Vec<ConnectionDto>> {
    let now = chrono::Local::now().timestamp_millis();
    let mut stmt = conn.prepare(
        "SELECT id, platform, kind, name, label, masked, status, last_sync_ms, base_url, model, effort, context_1m
         FROM connections ORDER BY created_ms ASC",
    )?;
    let rows = stmt
        .query_map([], |r| row_to_dto(r, now))?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// 连接名称校验：新增与改名共用同一口径（去首尾空白，1–80 个字符）。
fn validate_name(name: &str) -> Result<String, String> {
    let name = name.trim().to_string();
    if name.is_empty() || name.chars().count() > 80 {
        return Err("连接名称应为 1 到 80 个字符".into());
    }
    Ok(name)
}

/// 校验连接输入，显式凭证不触发本机读取。
pub fn prepare_new(mut c: NewConnection) -> Result<NewConnection, String> {
    if !crate::platforms::known(&c.platform) {
        return Err("不支持的平台标识".into());
    }
    if crate::platforms::native(&c.platform) && !(c.kind == "api" && crate::provider_key::supported(&c.platform)) { return Err("该平台请通过本机来源获取，不保存猜测的登录凭证".into()); }
    // 仅获取型平台（画布 18 后续）：凭证只能来自本机读取
    if crate::creds::is_fetch_only_platform(&c.platform) && c.kind != "auth" && !crate::provider_key::supported(&c.platform) {
        return Err("该平台只支持从本机获取凭证，不支持手动填写".into());
    }
    if !matches!(c.kind.as_str(), "auth" | "api") {
        return Err("接入方式必须是 auth 或 api".into());
    }
    c.name = validate_name(&c.name)?;
    let using_local = c.secret.as_deref().map_or(true, |s| s.trim().is_empty());
    if using_local && c.base_url.as_deref().map_or(true, |s| s.trim().is_empty()) {
        c.base_url = crate::creds::local_base_url(&c.platform, &c.kind);
    }
    c.secret = c.secret
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| crate::creds::read_secret(&c.platform, &c.kind));
    if c.secret.is_none() {
        return Err("未找到可保存的凭证；请粘贴 API Key，或先在对应 CLI 中登录".into());
    }
    c.base_url = c.base_url
        .take()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty());
    if let Some(base) = c.base_url.as_deref() {
        let url = reqwest::Url::parse(base).map_err(|_| "网关地址格式无效".to_string())?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err("网关地址必须使用 HTTP 或 HTTPS".into());
        }
    }
    if crate::provider_key::supported(&c.platform) {
        crate::provider_key::check_base(&c.platform, c.base_url.as_deref())?;
        c.model = None; c.effort = None; c.context_1m = None;
    }
    c.base_url = normalized_base(&c.platform, &c.kind, c.base_url);
    Ok(c)
}

fn normalized_base(platform: &str, kind: &str, base: Option<String>) -> Option<String> {
    if kind == "auth" { return None; }
    base.filter(|value| {
        let Ok(url) = reqwest::Url::parse(value) else { return true };
        let host = match platform { "claude" => "api.anthropic.com", "codex" => "api.openai.com", _ => return true };
        !(url.scheme() == "https" && url.host_str() == Some(host) && url.port_or_known_default() == Some(443)
            && matches!(url.path().trim_end_matches('/'), "" | "/v1") && url.query().is_none() && url.fragment().is_none()
            && url.username().is_empty() && url.password().is_none())
    })
}

/// 编辑命令用：api 连接的地址归一（官方地址返回 None，不落库）。
pub(crate) fn normalized_base_for(platform: &str, base: Option<String>) -> Option<String> {
    normalized_base(platform, "api", base)
}

/// 仅写入已经完成凭证解析与网络验证的连接。
pub fn add(conn: &Sql, c: NewConnection) -> Result<String, String> {
    if c.secret.as_deref().map_or(true, |secret| secret.is_empty()) {
        return Err("连接尚未验证".into());
    }
    let now = chrono::Local::now().timestamp_millis();
    let suffix: String = conn.query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    let id = format!("{}-{suffix}", c.platform);
    let masked = c.secret.as_deref().map(mask).unwrap_or_default();
    let label = if c.kind == "api" { "API Key" } else { "官方订阅" };
    let base = c.base_url;

    conn.execute(
        "INSERT INTO connections
           (id, platform, kind, name, label, secret, masked, base_url, model, effort, context_1m, status, last_sync_ms, created_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'connected', ?12, ?12)",
        params![id, c.platform, c.kind, c.name, label, c.secret, masked, base, c.model, c.effort, c.context_1m, now],
    ).map_err(|e| e.to_string())?;
    Ok(id)
}

/// 移除连接：只删凭证与配置，**历史统计保留**（§6.3）。
/// requests 表按 platform 存储、不关联 connection_id，因此这里天然不会碰到统计数据。
/// 「获取模型」：从网关拉取可用模型列表。
/// Claude 走 x-api-key + anthropic-version，OpenAI 兼容网关走 Bearer；
/// 两家的 /v1/models 响应同为 {"data": [{"id": ...}]}，取 id 即可。
pub fn fetch_models(platform: &str, base_url: &str, secret: &str) -> Result<Vec<String>, String> {
    let url = models_endpoint(base_url, None)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = apply_auth(client.get(&url), platform, secret)
        .send()
        .map_err(|e| format!("无法连接网关：{e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("获取模型失败：HTTP {status}，请检查 Key 与地址"));
    }
    let body: serde_json::Value = resp.json().map_err(|e| format!("响应解析失败：{e}"))?;
    let ids = body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(str::to_string))
                .collect::<Vec<_>>()
        });
    match ids {
        Some(ids) if !ids.is_empty() => Ok(ids),
        _ => Err("网关返回了空模型列表".into()),
    }
}

/// 「获取强度」已移除：思考强度没有网关端点，由用户按模型直接选择
/// off/low/medium/high（与请求日志 effort 列同口径），无需网络请求。

/// {base} → {base}/v1/models[/{model}]。用户可能填 https://gw.test
/// 或已带 /v1 的地址，两种都归一；仅接受 HTTP(S)。
/// {base} → {base}/v1/models[/{model}]。用户可能填 https://gw.test
/// 或已带 /v1 的地址，两种都归一；仅接受 HTTP(S)。
/// 「获取模型」与「获取强度」（effort_map）共用。
pub(crate) fn models_endpoint(base_url: &str, model: Option<&str>) -> Result<String, String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    let parsed = reqwest::Url::parse(trimmed).map_err(|_| "网关地址格式无效".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("网关地址必须使用 HTTP 或 HTTPS".into());
    }
    let tail = match model {
        Some(m) => format!("/models/{m}"),
        None => "/models".to_string(),
    };
    if trimmed.ends_with("/v1") {
        Ok(format!("{trimmed}{tail}"))
    } else {
        Ok(format!("{trimmed}/v1{tail}"))
    }
}

/// 平台鉴权头：Claude 走 x-api-key + anthropic-version，OpenAI 兼容走 Bearer。「获取强度」共用。
pub(crate) fn apply_auth(req: reqwest::blocking::RequestBuilder, platform: &str, secret: &str) -> reqwest::blocking::RequestBuilder {
    if platform == "claude" {
        req.header("x-api-key", secret).header("anthropic-version", "2023-06-01")
    } else {
        req.header("Authorization", format!("Bearer {secret}"))
    }
}

pub fn register_local(conn: &Sql, input: NewConnection) -> Result<(String, bool), String> {
    let c = prepare_new(input)?;
    let mut stmt = conn.prepare("SELECT id, base_url FROM connections WHERE platform=?1 AND kind=?2 AND secret=?3").map_err(|e| e.to_string())?;
    let matches = stmt.query_map(params![c.platform, c.kind, c.secret], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))
        .map_err(|e| e.to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())?;
    let existing = matches.into_iter().find(|(_, base)| normalized_base(&c.platform, &c.kind, base.clone()) == c.base_url).map(|(id, _)| id);
    if let Some(id) = existing { return Ok((id, false)); }
    let id = add(conn, c)?;
    conn.execute("UPDATE connections SET status='paused', last_sync_ms=NULL WHERE id=?1", [&id]).map_err(|e| e.to_string())?;
    Ok((id, true))
}

pub(crate) fn selectable_creds(conn: &Sql, id: &str) -> Result<Option<Creds>, String> {
    let status: Option<String> = conn.query_row("SELECT status FROM connections WHERE id=?1", [id], |r| r.get(0)).optional().map_err(|e| e.to_string())?;
    if status.as_deref() != Some("connected") { return Err("请先连接成功，再启用到灵动岛".into()); }
    creds_of(conn, id).map_err(|e| e.to_string())
}

pub fn remove(conn: &Sql, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM connections WHERE id = ?1", params![id])?;
    Ok(())
}

/// 暂停只改变连接状态，保留凭证、配置与所有本机历史。
pub fn set_paused(conn: &Sql, id: &str, paused: bool) -> Result<(), String> {
    let changed = conn.execute(
        "UPDATE connections SET status = ?2 WHERE id = ?1",
        params![id, if paused { "paused" } else { "connected" }],
    ).map_err(|e| e.to_string())?;
    if changed == 0 { return Err("连接不存在".into()); }
    Ok(())
}

/// 取出连接的凭证与地址，**仅供后端查额度/余额使用**。
/// 返回值含凭证原值，绝不可序列化给前端或写进日志。
pub(crate) struct Creds {
    pub paused: bool,
    pub name: String,
    pub platform: String,
    pub kind: String,
    pub secret: Option<String>,
    pub base_url: Option<String>,
}

pub(crate) fn creds_of(conn: &Sql, id: &str) -> rusqlite::Result<Option<Creds>> {
    conn.query_row(
        "SELECT name, platform, kind, secret, base_url, status FROM connections WHERE id = ?1",
        params![id],
        |r| {
            let platform: String = r.get(1)?;
            let kind: String = r.get(2)?;
            let base_url = normalized_base(&platform, &kind, r.get(4)?);
            Ok(Creds {
                paused: r.get::<_, String>(5)? == "paused",
                name: r.get(0)?,
                platform: r.get(1)?,
                kind: r.get(2)?,
                secret: r.get(3)?,
                base_url,
            })
        },
    )
    .optional()
}

/// 所有远程查询在命中缓存或发送请求之前，都必须经过此检查。
pub(crate) fn query_creds(conn: &Sql, id: &str) -> Result<Option<Creds>, String> {
    let current = creds_of(conn, id).map_err(|e| e.to_string())?;
    if current.as_ref().is_some_and(|c| c.paused) {
        return Err("连接已断开，请先重新连接".into());
    }
    Ok(current)
}

/// 记录一次成功查询的时间，用于「最近同步」列
pub(crate) fn touch_sync(conn: &Sql, id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE connections SET last_sync_ms = ?2, status = 'connected' WHERE id = ?1 AND status != 'paused'",
        params![id, chrono::Local::now().timestamp_millis()],
    )?;
    Ok(())
}

/// 把连接标记为凭证失效，供额度查询返回 401 时使用
pub(crate) fn mark_expired(conn: &Sql, id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE connections SET status = 'expired' WHERE id = ?1 AND status != 'paused'",
        params![id],
    )?;
    Ok(())
}

pub(crate) fn record_query_status(conn: &Sql, id: &str, success: bool) -> Result<bool, String> {
    let old: Option<String> = conn.query_row("SELECT status FROM connections WHERE id=?1", [id], |r| r.get(0)).optional().map_err(|e| e.to_string())?;
    if old.is_none() || old.as_deref() == Some("paused") { return Ok(false); }
    if success { touch_sync(conn, id) } else { mark_expired(conn, id) }.map_err(|e| e.to_string())?;
    Ok(old.as_deref() != Some(if success { "connected" } else { "expired" }))
}

pub fn replace_api_key(conn: &Sql, id: &str, secret: &str) -> Result<(), String> {
    let changed = conn.execute(
        "UPDATE connections
         SET secret = ?2, masked = ?3, status = 'connected', last_sync_ms = ?4
         WHERE id = ?1 AND kind = 'api' AND status != 'paused'",
        params![id, secret, mask(secret), chrono::Local::now().timestamp_millis()],
    ).map_err(|e| e.to_string())?;
    if changed == 0 { return Err("API Key 连接不存在".into()); }
    Ok(())
}

/// 重命名连接：只改显示名称，凭证、脱敏标识与状态一概不动。
/// 纯元数据操作，断开的连接同样允许改名。
pub fn rename_connection(conn: &Sql, id: &str, name: &str) -> Result<(), String> {
    let name = validate_name(name)?;
    let changed = conn
        .execute("UPDATE connections SET name = ?2 WHERE id = ?1", params![id, name])
        .map_err(|e| e.to_string())?;
    if changed == 0 { return Err("连接不存在".into()); }
    Ok(())
}

/// 统一编辑（画布 18）：更新 API 连接的网关地址与默认参数（模型/思考强度/1M）。
/// 地址传入前须已经过 normalized_base 归一；名称与凭证走各自的函数。
pub fn update_params(
    conn: &Sql,
    id: &str,
    base_url: Option<&str>,
    model: Option<&str>,
    effort: Option<&str>,
    context_1m: Option<bool>,
) -> Result<(), String> {
    let changed = conn
        .execute(
            "UPDATE connections SET base_url = ?2, model = ?3, effort = ?4, context_1m = ?5
             WHERE id = ?1 AND kind = 'api'",
            params![id, base_url, model, effort, context_1m],
        )
        .map_err(|e| e.to_string())?;
    if changed == 0 { return Err("API Key 连接不存在".into()); }
    Ok(())
}

/// 「添加连接」读取本机配置时应用用户填写的连接名称。
/// 名称只填了空白视为未填，保留自动生成名；超长名称整体拒绝。
pub fn with_custom_name(
    inputs: Vec<NewConnection>,
    name: Option<String>,
) -> Result<Vec<NewConnection>, String> {
    let Some(name) = name.as_deref().map(str::trim).filter(|n| !n.is_empty()) else {
        return Ok(inputs);
    };
    let name = validate_name(name)?;
    Ok(inputs
        .into_iter()
        .map(|input| NewConnection { name: name.clone(), ..input })
        .collect())
}

/// 「获取」的结果（§6.3 的四种状态里，后端只负责成功/未找到）
#[derive(Debug, Serialize)]
pub struct FetchResult {
    pub ok: bool,
    /// 成功时为新的脱敏标识
    pub masked: Option<String>,
    /// 失败原因，直接作为界面提示文案
    pub message: String,
    /// 扫描过的位置，界面需要说明读取范围，不静默读取
    pub scanned: Vec<String>,
    /// 本次是否真的写入了变更；「已更新」与「已是最新」靠它区分
    pub changed: bool,
}

/// 本机当前应用的凭证与地址，是「更新」与自动同步共用的最小变更集。
#[derive(Debug, Clone)]
pub(crate) struct LocalPair {
    pub secret: String,
    pub base_url: Option<String>,
}

/// 把本机当前配置原位应用到一个已有连接上。
///
/// local 为 None 表示本机没有读到凭证：只读不写，原状态与凭证保持不变。
/// 更新范围是 secret、masked 与 base_url——cc-switch 切换中转时两者一起变，
/// 只刷 Key 会把新 Key 发往旧网关。同平台同类型有多条连接且脱敏标识对不上
/// 时拒绝覆盖，让用户显式选择，绝不静默改错行。
pub(crate) fn apply_local(
    conn: &Sql,
    id: &str,
    local: Option<LocalPair>,
    scanned: Vec<String>,
) -> rusqlite::Result<FetchResult> {
    let row: Option<(String, String, String, Option<String>)> = conn
        .query_row(
            "SELECT platform, kind, masked, base_url FROM connections WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((platform, kind, current_masked, current_base)) = row else {
        return Ok(FetchResult { ok: false, masked: None, message: "连接不存在".into(), scanned, changed: false });
    };
    // 失败不得清空或破坏现有连接：这里只读不写
    let Some(local) = local else {
        return Ok(FetchResult { ok: false, masked: None, message: "未找到本机凭证".into(), scanned, changed: false });
    };
    let masked = mask(&local.secret);
    // 与 prepare_new 同一清洗口径：去首尾空白、去结尾斜杠，官方地址归一成 None
    let cleaned_base = local
        .base_url
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty());
    let base_url = normalized_base(&platform, &kind, cleaned_base);
    let same_kind_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM connections WHERE platform = ?1 AND kind = ?2",
        params![platform, kind],
        |row| row.get(0),
    )?;
    if same_kind_count > 1 && !current_masked.is_empty() && current_masked != masked {
        return Ok(FetchResult {
            ok: false,
            masked: Some(masked),
            message: "本机当前登录账号与此命名连接不同，已拒绝覆盖；请新增连接或切回对应 CLI 账号".into(),
            scanned,
            changed: false,
        });
    }
    let changed = current_masked != masked
        || normalized_base(&platform, &kind, current_base) != base_url;
    if changed {
        let now = chrono::Local::now().timestamp_millis();
        conn.execute(
            "UPDATE connections
                SET secret = ?2, masked = ?3, base_url = ?4, status = 'connected', last_sync_ms = ?5
              WHERE id = ?1",
            params![id, local.secret, masked, base_url, now],
        )?;
    }
    Ok(FetchResult {
        ok: true,
        masked: Some(masked),
        message: if changed { "已更新凭证".into() } else { "本机凭证未变化".into() },
        scanned,
        changed,
    })
}

/// 「更新」按钮：重读本机配置刷新**已存在**连接的 secret 与 base_url。
/// 不新建连接、不改名称与类型（§6.3）。
pub fn fetch_local(conn: &Sql, id: &str) -> rusqlite::Result<FetchResult> {
    if creds_of(conn, id)?.is_some_and(|c| c.paused) {
        return Ok(FetchResult { ok: false, masked: None, message: "连接已断开，请先重新连接".into(), scanned: vec![], changed: false });
    }
    let row: Option<(String, String)> = conn
        .query_row("SELECT platform, kind FROM connections WHERE id = ?1", params![id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?;
    let Some((platform, kind)) = row else {
        return Ok(FetchResult { ok: false, masked: None, message: "连接不存在".into(), scanned: vec![], changed: false });
    };
    let scanned = crate::creds::scanned_locations(&platform, &kind);
    let local = crate::creds::read_local_pair(&platform, &kind)
        .map(|(secret, base_url)| LocalPair { secret, base_url });
    apply_local(conn, id, local, scanned)
}

/// 自动同步（默认常开）：把本机当前应用的配置原位写回已有连接。
///
/// 只处理无歧义的连接：同平台同类型恰好一条时才尝试原位更新；本机没读到、
/// 多条同类型（分不清本机配置属于哪条）时一律不动。paused 的连接会被
/// apply_local 拒绝，不会被打断或改写。返回是否有实际变更，供调用方
/// 决定是否推送事件。
pub(crate) fn sync_from_local_with(
    conn: &Sql,
    read: &dyn Fn(&str, &str) -> Option<LocalPair>,
) -> Result<bool, String> {
    let rows: Vec<(String, String, String)> = conn
        .prepare("SELECT id, platform, kind FROM connections")
        .and_then(|mut stmt| {
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(|e| e.to_string())?;
    let mut changed = false;
    for (id, platform, kind) in rows {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM connections WHERE platform = ?1 AND kind = ?2",
                params![platform, kind],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if count != 1 { continue; }
        let Some(local) = read(&platform, &kind) else { continue };
        let result = apply_local(conn, &id, Some(local), vec![]).map_err(|e| e.to_string())?;
        changed |= result.changed;
    }
    Ok(changed)
}

/// 自动同步入口：读本机真实配置。应用启动时与配置文件变化时各调一次。
pub fn sync_from_local(conn: &Sql) -> Result<bool, String> {
    sync_from_local_with(conn, &|platform, kind| {
        crate::creds::read_local_pair(platform, kind)
            .map(|(secret, base_url)| LocalPair { secret, base_url })
    })
}
