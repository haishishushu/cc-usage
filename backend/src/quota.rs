//! 额度与余额的只读查询（§2.5 / §7.4）
//!
//! ## 查询来源
//!
//! | 来源 | 端点 | 凭证 | 能读到 |
//! |---|---|---|---|
//! | Claude 官方订阅 | `GET https://api.anthropic.com/api/oauth/usage` | OAuth accessToken | 5h / 7d 等窗口的已用百分比与重置时间 |
//! | Codex 官方订阅 | `GET https://chatgpt.com/backend-api/wham/usage` | Codex Auth accessToken | 5h / 7d 等实际返回的额度窗口 |
//! | Claude API | `GET /v1/organizations/usage_report/messages` 与 `cost_report` | Admin API Key | 组织今日 Token 与费用 |
//! | Codex API | `GET /v1/organization/usage/completions` 与 `costs` | Admin API Key | 组织今日 Token 与费用 |
//! | sub2api 网关 | `GET {base}/v1/usage` | 分组 API Key（Bearer） | 配额、限流窗口、钱包余额、今日/累计用量 |
//!
//! 订阅额度端点只接受 Auth 凭证；API Key 走组织用量端点。普通调用 Key 没有
//! Admin 权限时返回 403，界面回退本机会话统计，不把它误报为订阅额度或钱包余额。
//!
//! ## 纪律
//!
//! - 全部是 GET 只读，不发送任何提示内容或用量数据到第三方
//! - 查询失败**绝不渲染为 0% 或满格**：用 `QuotaState` 的五种状态区分，
//!   界面显示「—」并给出具体原因（§2.5）
//! - 遵守频率限制：429 单独成一种状态，附带退避提示
//! - 请求超时 15 秒，与 CodexBar 的取值一致

use serde::{Deserialize, Serialize};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(15);

/// 官方 OAuth 用量端点当前需要该 beta 头
const ANTHROPIC_BASE: &str = "https://api.anthropic.com";
const ANTHROPIC_USAGE_PATH: &str = "/api/oauth/usage";
const ANTHROPIC_BETA: &str = "oauth-2025-04-20";

#[derive(Deserialize)]
struct CodexWindow {
    used_percent: Option<f64>,
    limit_window_seconds: i64,
    reset_at: Option<i64>,
}

#[derive(Deserialize)]
struct CodexLimits {
    primary_window: Option<CodexWindow>,
    secondary_window: Option<CodexWindow>,
}

#[derive(Deserialize)]
struct CodexUsage {
    plan_type: Option<String>,
    rate_limit: Option<CodexLimits>,
}

fn parse_codex_usage(body: &str) -> QuotaState {
    let usage: CodexUsage = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return QuotaState::Failed { reason: "Codex 额度响应格式无效".into() },
    };
    let windows: Vec<QuotaWindow> = usage.rate_limit.into_iter()
        .flat_map(|r| [r.primary_window, r.secondary_window]).flatten()
        .filter(|w| w.limit_window_seconds > 0)
        .map(|w| {
            // primary 不一定是 5h：当前 Pro 账号的 primary 就是 7d。
            let (key, name) = match w.limit_window_seconds {
                18000 => ("5h".into(), "5 小时额度".into()),
                604800 => ("7d".into(), "周额度".into()),
                seconds => (format!("{}m", seconds / 60), format!("{} 分钟额度", seconds / 60)),
            };
            QuotaWindow {
                key, window_name: name,
                used_percent: w.used_percent.filter(|v| v.is_finite() && (0.0..=100.0).contains(v)),
                amount_text: None,
                resets_at: w.reset_at.and_then(|s| chrono::DateTime::from_timestamp(s, 0))
                    .map(|t| t.to_rfc3339()),
            }
        }).collect();
    if windows.is_empty() { return QuotaState::Failed { reason: "Codex 未返回可识别的额度窗口".into() }; }
    QuotaState::Ok { windows, plan: usage.plan_type }
}

pub fn codex_oauth_quota(access_token: &str, account: Option<&str>) -> QuotaState {
    // 固定官方地址并禁止重定向，凭证不会发送到自定义地址或重定向目标。
    let client = match reqwest::blocking::Client::builder().timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::none()).build() {
        Ok(c) => c,
        Err(_) => return QuotaState::Failed { reason: "无法创建 Codex 额度查询客户端".into() },
    };
    let mut request = client.get("https://chatgpt.com/backend-api/wham/usage")
        .bearer_auth(access_token).header("Accept", "application/json");
    if let Some(account) = account { request = request.header("ChatGPT-Account-Id", account); }
    match request.send() {
        Ok(r) => match read(r) {
            HttpOutcome::Body(body) => parse_codex_usage(&body),
            HttpOutcome::Unauthorized(reason) => QuotaState::Unauthorized { reason },
            HttpOutcome::Forbidden(reason) => QuotaState::Forbidden { reason },
            HttpOutcome::RateLimited(reason) => QuotaState::RateLimited { reason },
            HttpOutcome::Failed(reason) => QuotaState::Failed { reason },
        },
        Err(_) => QuotaState::Failed { reason: "Codex 额度查询失败，请检查网络后刷新".into() },
    }
}

pub fn local_codex_quota() -> QuotaState {
    let Some((token, account)) = crate::creds::local_codex_auth() else {
        return QuotaState::Unauthorized { reason: "未找到本机 Codex Auth，请先在 Codex 中登录".into() };
    };
    codex_oauth_quota(&token, account.as_deref())
}

#[cfg(test)]
mod codex_tests {
    use super::*;
    #[test]
    #[ignore = "需要本机 Codex Auth 和网络；仅显式执行"]
    fn local_auth_read_only_smoke() {
        match local_codex_quota() {
            QuotaState::Ok { windows, .. } => {
                assert!(!windows.is_empty());
                for w in windows { println!("{} used={:?} has_reset={}", w.key, w.used_percent, w.resets_at.is_some()); }
            }
            other => panic!("额度查询未成功: {other:?}"),
        }
    }
    #[test]
    fn weekly_primary_is_not_mislabeled_as_five_hours() {
        let QuotaState::Ok { windows, plan } = parse_codex_usage(r#"{"plan_type":"pro","rate_limit":{"primary_window":{"used_percent":56,"limit_window_seconds":604800,"reset_at":1789805546},"secondary_window":null}}"#) else { panic!("解析失败") };
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].key, "7d");
        assert_eq!(windows[0].used_percent, Some(56.0));
        assert!(windows[0].resets_at.is_some());
        assert_eq!(plan.as_deref(), Some("pro"));
    }

    #[test]
    fn dual_windows_preserve_real_zero_and_unknown_values() {
        let QuotaState::Ok { windows, .. } = parse_codex_usage(r#"{"rate_limit":{"primary_window":{"used_percent":0,"limit_window_seconds":18000,"reset_at":null},"secondary_window":{"used_percent":null,"limit_window_seconds":604800,"reset_at":null}}}"#) else { panic!("解析失败") };
        assert_eq!(windows[0].key, "5h");
        assert_eq!(windows[0].used_percent, Some(0.0));
        assert_eq!(windows[1].key, "7d");
        assert_eq!(windows[1].used_percent, None);
    }
}

/// 一个额度窗口。`used_percent` 为 None 时界面**不画进度条**（§7.4）
#[derive(Debug, Clone, Serialize)]
pub struct QuotaWindow {
    /// 窗口标识，如 5h / 7d / 1d
    pub key: String,
    /// 窗口说明，如「5 小时窗口」
    pub window_name: String,
    pub used_percent: Option<f64>,
    /// 已用 / 总量的文字描述，来源没给分母时为 None
    pub amount_text: Option<String>,
    /// 重置时间（来源原样透传的 ISO 字符串）
    pub resets_at: Option<String>,
}

/// 额度查询状态。**刻意不用 `Option<f64>`** —— 那样无法区分
/// 「查不到」与「真的是 0」，也无法把失败原因带到界面上（§2.5）
#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[derive(Clone)]
pub enum QuotaState {
    /// 查询成功
    Ok {
        windows: Vec<QuotaWindow>,
        /// 套餐名原样透传，不由百分比推断
        plan: Option<String>,
    },
    /// 该接入方式不支持额度查询（如用 API Key 查官方订阅额度）
    Unsupported { reason: String },
    /// 凭证有效但无权限
    Forbidden { reason: String },
    /// 凭证失效或过期，需要重新授权
    Unauthorized { reason: String },
    /// 触发频率限制，需退避后重试
    RateLimited { reason: String },
    /// 网络错误、解析失败等
    Failed { reason: String },
}

/// 余额查询状态。真实余额为零时显示 `0.00`，查不到才显示「—」（§2.5）
#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[derive(Clone)]
pub enum BalanceState {
    Ok {
        balance: f64,
        currency: String,
        /// 已用额度（若来源提供）
        used: Option<f64>,
    },
    Unsupported { reason: String },
    Forbidden { reason: String },
    Unauthorized { reason: String },
    RateLimited { reason: String },
    Failed { reason: String },
}

/// 官方直连在不同凭证类型下可提供的能力。订阅额度与 API 组织用量是两条独立链路，
/// 不能用 API Key 冒充 Auth，也不能把 Auth 当成组织管理 Key。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg(test)]
pub struct OfficialCapabilities {
    pub quota: bool,
    pub admin_usage: bool,
    pub local_sessions: bool,
}

#[cfg(test)]
pub fn official_capabilities(platform: &str, kind: &str) -> OfficialCapabilities {
    match (platform, kind) {
        ("claude" | "codex", "auth") => OfficialCapabilities {
            quota: true,
            admin_usage: false,
            local_sessions: true,
        },
        ("claude" | "codex", "api") => OfficialCapabilities {
            quota: false,
            admin_usage: true,
            local_sessions: true,
        },
        _ => OfficialCapabilities {
            quota: false,
            admin_usage: false,
            local_sessions: false,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedApiUsage {
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    total_tokens: u64,
}

/// API Key 的官方组织用量查询结果。普通调用 Key 通常会得到 403；界面据此回退到
/// 本机同平台会话统计，而不是伪造订阅额度或余额。
#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[derive(Clone)]
pub enum ApiUsageState {
    Ok {
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        cache_write_tokens: u64,
        total_tokens: u64,
        cost_usd: Option<f64>,
        cost_reason: Option<String>,
        source: String,
    },
    Unsupported { reason: String },
    Forbidden { reason: String },
    Unauthorized { reason: String },
    RateLimited { reason: String },
    Failed { reason: String },
}

fn sum_u64(value: &serde_json::Value, key: &str) -> u64 {
    value.get(key).and_then(serde_json::Value::as_u64).unwrap_or(0)
}

fn result_rows(root: &serde_json::Value) -> Result<Vec<&serde_json::Value>, String> {
    if root.get("has_more").and_then(serde_json::Value::as_bool) == Some(true)
        || root.get("next_page").and_then(serde_json::Value::as_str).is_some_and(|s| !s.is_empty()) {
        return Err("响应仍有分页，不能将部分结果显示为今日总量".into());
    }
    let buckets = root.get("data").and_then(serde_json::Value::as_array).ok_or("响应缺少 data 数组")?;
    let mut rows = Vec::new();
    for bucket in buckets {
        rows.extend(bucket.get("results").and_then(serde_json::Value::as_array).ok_or("响应缺少 results 数组")?);
    }
    Ok(rows)
}

fn required_tokens(row: &serde_json::Value, key: &str) -> Result<u64, String> {
    row.get(key).and_then(serde_json::Value::as_u64).ok_or_else(|| format!("缺少或无效的 {key}"))
}

fn cost_amount(value: Option<&serde_json::Value>) -> Result<f64, String> {
    value.and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
        .filter(|v| v.is_finite() && *v >= 0.0).ok_or_else(|| "缺少或无效的费用金额".into())
}

fn parse_anthropic_admin_usage(body: &str) -> Result<ParsedApiUsage, String> {
    let root: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let mut parsed = ParsedApiUsage {
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        total_tokens: 0,
    };
    for row in result_rows(&root)? {
        let uncached = required_tokens(row, "uncached_input_tokens")?;
        let cache_read = sum_u64(row, "cache_read_input_tokens");
        let cache_write = row
            .get("cache_creation")
            .map(|v| {
                sum_u64(v, "ephemeral_1h_input_tokens")
                    + sum_u64(v, "ephemeral_5m_input_tokens")
            })
            .unwrap_or(0);
        let output = required_tokens(row, "output_tokens")?;
        parsed.input_tokens += uncached + cache_read + cache_write;
        parsed.output_tokens += output;
        parsed.cache_read_tokens += cache_read;
        parsed.cache_write_tokens += cache_write;
    }
    parsed.total_tokens = parsed.input_tokens + parsed.output_tokens;
    Ok(parsed)
}

fn parse_openai_admin_usage(body: &str) -> Result<ParsedApiUsage, String> {
    let root: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let mut parsed = ParsedApiUsage {
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        total_tokens: 0,
    };
    for row in result_rows(&root)? {
        // OpenAI 的 input_tokens 已包含 cached input，不能再把 input_cached_tokens 加进总量。
        parsed.input_tokens += required_tokens(row, "input_tokens")?;
        parsed.output_tokens += required_tokens(row, "output_tokens")?;
        parsed.cache_read_tokens += sum_u64(row, "input_cached_tokens");
    }
    parsed.total_tokens = parsed.input_tokens + parsed.output_tokens;
    Ok(parsed)
}

fn parse_anthropic_admin_cost(body: &str) -> Result<f64, String> {
    let root: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let mut cents = 0.0;
    for row in result_rows(&root)? {
        if row.get("currency").and_then(serde_json::Value::as_str).is_some_and(|v| !v.eq_ignore_ascii_case("usd")) { return Err("费用币种不是 USD".into()); }
        cents += cost_amount(row.get("amount"))?;
    }
    Ok(cents / 100.0)
}

fn parse_openai_admin_cost(body: &str) -> Result<f64, String> {
    let root: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let mut total = 0.0;
    for row in result_rows(&root)? {
        let amount = row.get("amount").ok_or("缺少费用金额")?;
        if amount.get("currency").and_then(serde_json::Value::as_str).is_some_and(|v| !v.eq_ignore_ascii_case("usd")) { return Err("费用币种不是 USD".into()); }
        total += cost_amount(amount.get("value"))?;
    }
    Ok(total)
}

pub(crate) fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("无法创建 HTTP 客户端: {e}"))
}

/// HTTP 状态码 → 面向用户的原因文案。
/// 401 / 403 / 429 必须分开，含义与后续动作都不同。
pub(crate) enum HttpOutcome {
    Body(String),
    Unauthorized(String),
    Forbidden(String),
    RateLimited(String),
    Failed(String),
}

pub(crate) fn read(resp: reqwest::blocking::Response) -> HttpOutcome {
    let status = resp.status();
    let code = status.as_u16();
    match code {
        200..=299 => match resp.text() {
            Ok(t) => HttpOutcome::Body(t),
            Err(e) => HttpOutcome::Failed(format!("读取响应失败: {e}")),
        },
        401 => HttpOutcome::Unauthorized("凭证已失效或过期，需要重新授权".into()),
        403 => HttpOutcome::Forbidden("该凭证没有查询此数据的权限".into()),
        429 => {
            // Retry-After 若存在则原样告知，让用户知道等多久
            let after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .map(|s| format!("，请等待 {s} 秒后重试"))
                .unwrap_or_default();
            HttpOutcome::RateLimited(format!("已触发平台频率限制{after}"))
        }
        500..=599 => HttpOutcome::Failed(format!("服务端错误（HTTP {code}）")),
        _ => HttpOutcome::Failed(format!("查询失败（HTTP {code}）")),
    }
}

/* ------------------- Claude 官方订阅：/api/oauth/usage ------------------- */

#[derive(Debug, Deserialize)]
struct OAuthWindow {
    /// 已用比例。来源给的是 0–100 的百分数
    utilization: Option<f64>,
    resets_at: Option<String>,
}

/// 只取已确认存在的窗口字段。来源新增字段时不会解析失败（serde 默认忽略未知键）。
#[derive(Debug, Deserialize)]
struct OAuthUsage {
    five_hour: Option<OAuthWindow>,
    seven_day: Option<OAuthWindow>,
    seven_day_opus: Option<OAuthWindow>,
    seven_day_sonnet: Option<OAuthWindow>,
}

/// 查询 Claude 官方订阅额度。
///
/// **调用方必须先确认连接类型是 `auth`**：该端点只认 OAuth accessToken，
/// 传 API Key 会返回 401，那时应报「不支持」而非「凭证失效」。
pub fn claude_oauth_quota(access_token: &str) -> QuotaState {
    let c = match client() {
        Ok(c) => c,
        Err(e) => return QuotaState::Failed { reason: e },
    };

    let resp = c
        .get(format!("{ANTHROPIC_BASE}{ANTHROPIC_USAGE_PATH}"))
        .header("Authorization", format!("Bearer {access_token}"))
        .header("Accept", "application/json")
        .header("anthropic-beta", ANTHROPIC_BETA)
        .send();

    let body = match resp {
        Ok(r) => match read(r) {
            HttpOutcome::Body(b) => b,
            HttpOutcome::Unauthorized(reason) => return QuotaState::Unauthorized { reason },
            HttpOutcome::Forbidden(reason) => return QuotaState::Forbidden { reason },
            HttpOutcome::RateLimited(reason) => return QuotaState::RateLimited { reason },
            HttpOutcome::Failed(reason) => return QuotaState::Failed { reason },
        },
        Err(e) if e.is_timeout() => {
            return QuotaState::Failed {
                reason: "查询超时（15 秒）".into(),
            }
        }
        Err(e) => {
            return QuotaState::Failed {
                reason: format!("网络错误: {e}"),
            }
        }
    };

    let parsed: OAuthUsage = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            return QuotaState::Failed {
                reason: format!("响应格式无法解析: {e}"),
            }
        }
    };

    // 只放入来源真的给了数据的窗口；缺失的窗口不补 0%（§7.4）
    let mut windows = Vec::new();
    for (key, name, w) in [
        ("5h", "5 小时窗口", parsed.five_hour),
        ("7d", "7 天窗口", parsed.seven_day),
        ("7d-opus", "7 天窗口 · Opus", parsed.seven_day_opus),
        ("7d-sonnet", "7 天窗口 · Sonnet", parsed.seven_day_sonnet),
    ] {
        let Some(w) = w else { continue };
        // utilization 缺失时保留该窗口但不给百分比，界面只显示重置时间、不画条
        windows.push(QuotaWindow {
            key: key.into(),
            window_name: name.into(),
            used_percent: w.utilization.filter(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            amount_text: None,
            resets_at: w.resets_at,
        });
    }

    if windows.is_empty() {
        return QuotaState::Failed {
            reason: "该账号未返回任何额度窗口".into(),
        };
    }
    // 当前官方用量响应不含可信套餐档位，保持未知而不是复用连接类型标签。
    QuotaState::Ok { windows, plan: None }
}

/* ---------------- 官方 API Key：组织用量与费用（Admin 权限） ---------------- */

fn api_usage_error(outcome: HttpOutcome, provider: &str) -> Result<String, ApiUsageState> {
    match outcome {
        HttpOutcome::Body(body) => Ok(body),
        HttpOutcome::Unauthorized(reason) => Err(ApiUsageState::Unauthorized { reason }),
        HttpOutcome::Forbidden(_) => Err(ApiUsageState::Forbidden {
            reason: format!("{provider} 拒绝组织用量查询（需要 Admin 权限）；当前显示本机会话统计"),
        }),
        HttpOutcome::RateLimited(reason) => Err(ApiUsageState::RateLimited { reason }),
        HttpOutcome::Failed(reason) => Err(ApiUsageState::Failed { reason }),
    }
}

fn send_api_usage(
    request: reqwest::blocking::RequestBuilder,
    provider: &str,
) -> Result<String, ApiUsageState> {
    let response = request.send().map_err(|e| ApiUsageState::Failed {
        reason: if e.is_timeout() {
            format!("{provider} 用量查询超时（15 秒）")
        } else {
            format!("{provider} 用量查询网络错误: {e}")
        },
    })?;
    api_usage_error(read(response), provider)
}

fn today_bounds() -> (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) {
    let now = chrono::Local::now();
    let start = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|v| v.and_local_timezone(chrono::Local).single())
        .unwrap_or(now);
    (start.with_timezone(&chrono::Utc), now.with_timezone(&chrono::Utc))
}

fn cost_result(
    response: Result<reqwest::blocking::Response, reqwest::Error>,
    parse: fn(&str) -> Result<f64, String>,
) -> (Option<f64>, Option<String>) {
    let response = match response {
        Ok(response) => response,
        Err(e) => return (None, Some(format!("费用查询失败: {e}"))),
    };
    match read(response) {
        HttpOutcome::Body(body) => match parse(&body) {
            Ok(value) => (Some(value), None),
            Err(e) => (None, Some(format!("费用响应格式无法解析: {e}"))),
        },
        HttpOutcome::Forbidden(reason)
        | HttpOutcome::Unauthorized(reason)
        | HttpOutcome::RateLimited(reason)
        | HttpOutcome::Failed(reason) => (None, Some(reason)),
    }
}

pub fn anthropic_api_usage(api_key: &str) -> ApiUsageState {
    let client = match client() {
        Ok(client) => client,
        Err(reason) => return ApiUsageState::Failed { reason },
    };
    let (start, end) = today_bounds();
    let start = start.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let end = end.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let headers = |request: reqwest::blocking::RequestBuilder| {
        request
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Accept", "application/json")
    };
    let usage_body = match send_api_usage(
        headers(client.get(format!(
            "{ANTHROPIC_BASE}/v1/organizations/usage_report/messages"
        )))
        .query(&[
            ("starting_at", start.as_str()),
            ("ending_at", end.as_str()),
            ("bucket_width", "1d"),
            ("limit", "31"),
        ]),
        "Claude",
    ) {
        Ok(body) => body,
        Err(state) => return state,
    };
    let parsed = match parse_anthropic_admin_usage(&usage_body) {
        Ok(parsed) => parsed,
        Err(e) => return ApiUsageState::Failed { reason: format!("Claude 用量响应格式无法解析: {e}") },
    };
    let (cost_usd, cost_reason) = cost_result(
        headers(client.get(format!("{ANTHROPIC_BASE}/v1/organizations/cost_report")))
            .query(&[
                ("starting_at", start.as_str()),
                ("ending_at", end.as_str()),
                ("bucket_width", "1d"),
                ("limit", "31"),
            ])
            .send(),
        parse_anthropic_admin_cost,
    );
    ApiUsageState::Ok {
        input_tokens: parsed.input_tokens,
        output_tokens: parsed.output_tokens,
        cache_read_tokens: parsed.cache_read_tokens,
        cache_write_tokens: parsed.cache_write_tokens,
        total_tokens: parsed.total_tokens,
        cost_usd,
        cost_reason,
        source: "Claude 组织用量接口（Admin API Key）".into(),
    }
}

pub fn openai_api_usage(api_key: &str) -> ApiUsageState {
    let client = match client() {
        Ok(client) => client,
        Err(reason) => return ApiUsageState::Failed { reason },
    };
    let (start, end) = today_bounds();
    let start = start.timestamp().to_string();
    let end = end.timestamp().to_string();
    let auth = |request: reqwest::blocking::RequestBuilder| {
        request.bearer_auth(api_key).header("Accept", "application/json")
    };
    let usage_body = match send_api_usage(
        auth(client.get("https://api.openai.com/v1/organization/usage/completions"))
            .query(&[
                ("start_time", start.as_str()),
                ("end_time", end.as_str()),
                ("bucket_width", "1d"),
                ("limit", "31"),
            ]),
        "Codex",
    ) {
        Ok(body) => body,
        Err(state) => return state,
    };
    let parsed = match parse_openai_admin_usage(&usage_body) {
        Ok(parsed) => parsed,
        Err(e) => return ApiUsageState::Failed { reason: format!("Codex 用量响应格式无法解析: {e}") },
    };
    let (cost_usd, cost_reason) = cost_result(
        auth(client.get("https://api.openai.com/v1/organization/costs"))
            .query(&[
                ("start_time", start.as_str()),
                ("end_time", end.as_str()),
                ("bucket_width", "1d"),
                ("limit", "31"),
            ])
            .send(),
        parse_openai_admin_cost,
    );
    ApiUsageState::Ok {
        input_tokens: parsed.input_tokens,
        output_tokens: parsed.output_tokens,
        cache_read_tokens: parsed.cache_read_tokens,
        cache_write_tokens: parsed.cache_write_tokens,
        total_tokens: parsed.total_tokens,
        cost_usd,
        cost_reason,
        source: "OpenAI 组织用量接口（Admin API Key）".into(),
    }
}

/// 只有结构正确的成功响应才能确认模型列表查询有效。
fn validate_models_body(body: &str) -> Result<(), String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|_| "模型列表响应不是有效 JSON")?;
    if value.get("data").and_then(serde_json::Value::as_array).is_none() { return Err("模型列表响应格式无效".into()); }
    Ok(())
}

/// 普通 API Key 使用不产生模型调用费用的模型列表接口验证，403 本身不是凭证有效的证据。
fn official_models(platform: &str, secret: &str) -> HttpOutcome {
    let client = match client() { Ok(client) => client, Err(reason) => return HttpOutcome::Failed(reason) };
    let request = if platform == "claude" {
        client.get("https://api.anthropic.com/v1/models").header("x-api-key", secret).header("anthropic-version", "2023-06-01")
    } else {
        client.get("https://api.openai.com/v1/models").bearer_auth(secret)
    };
    request.send().map(read).unwrap_or_else(|_| HttpOutcome::Failed("验证连接失败，请检查网络后重试".into()))
}

fn validate_official_api_key(platform: &str, secret: &str) -> Result<(), String> {
    match official_models(platform, secret) {
        HttpOutcome::Body(body) => validate_models_body(&body),
        HttpOutcome::Forbidden(reason) | HttpOutcome::Unauthorized(reason) => {
            // Admin Key 可能没有模型调用权限，但组织接口成功仍可确认其有效性。
            let usage = if platform == "claude" { anthropic_api_usage(secret) } else { openai_api_usage(secret) };
            if matches!(usage, ApiUsageState::Ok { .. }) { Ok(()) } else { Err(reason) }
        }
        HttpOutcome::RateLimited(reason) | HttpOutcome::Failed(reason) => Err(reason),
    }
}

fn denied_admin_usage(models: HttpOutcome) -> ApiUsageState {
    match models {
        HttpOutcome::Body(body) => match validate_models_body(&body) {
            Ok(()) => ApiUsageState::Forbidden { reason: "当前 Key 可用，但没有组织统计权限；当前显示本机会话统计".into() },
            Err(reason) => ApiUsageState::Failed { reason },
        },
        HttpOutcome::Unauthorized(reason) => ApiUsageState::Unauthorized { reason },
        HttpOutcome::Forbidden(reason) => ApiUsageState::Forbidden { reason },
        HttpOutcome::RateLimited(reason) => ApiUsageState::RateLimited { reason },
        HttpOutcome::Failed(reason) => ApiUsageState::Failed { reason },
    }
}

/// 组织接口拒绝普通 Key 时，用模型列表确认，避免把缺少 Admin 权限当成凭证过期。
pub fn official_api_usage(platform: &str, secret: &str) -> ApiUsageState {
    let result = match platform {
        "claude" => anthropic_api_usage(secret),
        "codex" => openai_api_usage(secret),
        _ => return ApiUsageState::Unsupported { reason: "该平台暂不支持组织统计".into() },
    };
    if matches!(result, ApiUsageState::Unauthorized { .. }) {
        denied_admin_usage(official_models(platform, secret))
    } else { result }
}

pub fn validate_connection(
    platform: &str,
    kind: &str,
    secret: &str,
    base_url: Option<&str>,
    plan: &crate::coding_plan::PlanExtras,
) -> Result<(), String> {
    if kind == "api" && crate::provider_key::supported(platform) {
        return crate::provider_key::validate(platform, secret, base_url);
    }
    if let Some(base) = base_url.filter(|base| kind == "api" && !base.is_empty()) {
        // 套餐供应商（智谱/Kimi/MiniMax/ZenMux/OpenCode Go/火山）优先于 sub2api 网关路由
        if crate::coding_plan::detect_provider(base).is_some() {
            return match crate::coding_plan::coding_plan_quota(base, secret, plan) {
                QuotaState::Ok { .. } => Ok(()),
                QuotaState::Unsupported { reason }
                | QuotaState::Forbidden { reason }
                | QuotaState::Unauthorized { reason }
                | QuotaState::RateLimited { reason }
                | QuotaState::Failed { reason } => Err(reason),
            };
        }
        return match fetch_s2(base, secret) {
            Ok(data) if s2_can_validate(&data) => Ok(()),
            Ok(_) => Err("网关未返回可识别的用量、额度或余额数据".into()),
            Err((_, reason)) => Err(reason),
        };
    }

    match (platform, kind) {
        ("claude", "auth") => match claude_oauth_quota(secret) {
            QuotaState::Ok { .. } => Ok(()),
            QuotaState::Unsupported { reason }
            | QuotaState::Forbidden { reason }
            | QuotaState::Unauthorized { reason }
            | QuotaState::RateLimited { reason }
            | QuotaState::Failed { reason } => Err(reason),
        },
        ("codex", "auth") => {
            let account = crate::creds::local_codex_auth()
                .and_then(|(token, account)| (token == secret).then_some(account).flatten());
            match codex_oauth_quota(secret, account.as_deref()) {
                QuotaState::Ok { .. } => Ok(()),
                QuotaState::Unsupported { reason }
                | QuotaState::Forbidden { reason }
                | QuotaState::Unauthorized { reason }
                | QuotaState::RateLimited { reason }
                | QuotaState::Failed { reason } => Err(reason),
            }
        }
        ("claude" | "codex", "api") => validate_official_api_key(platform, secret),
        _ => Err("当前平台或接入方式尚未支持".into()),
    }
}

/* ---------------------- sub2api 网关：GET /v1/usage ---------------------- */

#[derive(Debug, Deserialize)]
struct S2Quota {
    limit: f64,
    used: f64,
    unit: Option<String>,
}

#[derive(Debug, Deserialize)]
struct S2RateLimit {
    /// 5h / 1d / 7d
    window: String,
    limit: f64,
    used: f64,
    reset_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct S2Subscription {
    daily_usage_usd: Option<f64>,
    weekly_usage_usd: Option<f64>,
    monthly_usage_usd: Option<f64>,
    daily_limit_usd: Option<f64>,
    weekly_limit_usd: Option<f64>,
    monthly_limit_usd: Option<f64>,
    #[serde(rename = "expires_at")]
    _expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct S2Usage {
    #[serde(rename = "isValid")]
    is_valid: Option<bool>,
    #[serde(rename = "planName")]
    plan_name: Option<String>,
    balance: Option<f64>,
    unit: Option<String>,
    quota: Option<S2Quota>,
    subscription: Option<S2Subscription>,
    #[serde(default)]
    rate_limits: Vec<S2RateLimit>,
}

fn s2_can_validate(data: &S2Usage) -> bool {
    data.is_valid != Some(false) && (data.balance.is_some_and(f64::is_finite)
        || data.quota.is_some() || data.subscription.is_some() || !data.rate_limits.is_empty())
}

/// 拼出 `{base}/v1/usage`，容忍用户把 base 填成 `.../v1` 或 `.../v1/usage`
fn s2_endpoint(base: &str) -> String {
    let mut b = base.trim_end_matches('/').to_string();
    if !(b.ends_with("/v1") || b.ends_with("/v1/usage")) {
        b.push_str("/v1");
    }
    if !b.ends_with("/usage") {
        b.push_str("/usage");
    }
    b
}

/// 百分比：分母缺失或为 0 时返回 None —— 除零会得到 inf，把进度条画坏
fn pct(used: f64, limit: f64) -> Option<f64> {
    if !used.is_finite() || !limit.is_finite() || used < 0.0 || limit <= 0.0 {
        None
    } else {
        Some((used / limit * 100.0).clamp(0.0, 100.0))
    }
}

fn amount(used: f64, limit: f64, unit: &str) -> String {
    format!("{used:.2} / {limit:.2} {unit}")
}

/// fetch_s2 对 HTTP 404 的固定文案；分流层（lib.rs）据此识别「网关没有账单端点」
/// 并转入编程套餐自动探测。404 ≠ 查询失败：Claude 式反代、本地中转都没有账单接口
pub const S2_ENDPOINT_MISSING: &str = "该网关未提供 sub2api 账单接口（HTTP 404）";

fn fetch_s2(base: &str, api_key: &str) -> Result<S2Usage, (u16, String)> {
    let c = client().map_err(|e| (0, e))?;
    let resp = c
        .get(s2_endpoint(base))
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Accept", "application/json")
        .send()
        .map_err(|e| {
            if e.is_timeout() {
                (0, "查询超时（15 秒）".to_string())
            } else {
                (0, format!("网络错误: {e}"))
            }
        })?;

    // 404 = 网关没有此端点（Claude 式反代、本地中转都没有账单接口），单独给码，
    // 避免落进笼统的「查询失败」让用户误以为是配置错误
    if resp.status().as_u16() == 404 {
        return Err((404, S2_ENDPOINT_MISSING.into()));
    }

    match read(resp) {
        HttpOutcome::Body(b) => {
            let data: S2Usage = serde_json::from_str(&b).map_err(|e| (200, format!("响应格式无法解析: {e}")))?;
            if data.is_valid == Some(false) { return Err((401, "网关标记当前凭证无效".into())); }
            Ok(data)
        }
        HttpOutcome::Unauthorized(r) => Err((401, r)),
        HttpOutcome::Forbidden(r) => Err((403, r)),
        HttpOutcome::RateLimited(r) => Err((429, r)),
        HttpOutcome::Failed(r) => Err((500, r)),
    }
}

/// 余额侧的错误映射，与 [`s2_quota_err`] 对称。
/// 404 归为「不支持」：没有该端点的网关（Claude 式反代、本地中转）不是配置错误
fn s2_balance_err(code: u16, reason: String) -> BalanceState {
    match code {
        401 => BalanceState::Unauthorized { reason },
        403 => BalanceState::Forbidden { reason },
        429 => BalanceState::RateLimited { reason },
        404 => BalanceState::Unsupported { reason },
        _ => BalanceState::Failed { reason },
    }
}

/// 按 HTTP 语义把错误映射到状态，401/403/429 各自成态
fn s2_quota_err(code: u16, reason: String) -> QuotaState {
    match code {
        401 => QuotaState::Unauthorized { reason },
        403 => QuotaState::Forbidden { reason },
        429 => QuotaState::RateLimited { reason },
        404 => QuotaState::Unsupported { reason },
        _ => QuotaState::Failed { reason },
    }
}

pub fn sub2api_quota(base: &str, api_key: &str) -> QuotaState {
    let d = match fetch_s2(base, api_key) {
        Ok(d) => d,
        Err((code, reason)) => return s2_quota_err(code, reason),
    };

    s2_quota_from_usage(d)
}

fn s2_quota_from_usage(d: S2Usage) -> QuotaState {
    let unit = d.unit.clone().unwrap_or_else(|| "USD".into());
    let mut windows = Vec::new();

    // 订阅型分组：日 / 周 / 月消费对上限
    if let Some(s) = &d.subscription {
        for (key, name, used, limit) in [
            ("1d", "日消费", s.daily_usage_usd, s.daily_limit_usd),
            ("7d", "周消费", s.weekly_usage_usd, s.weekly_limit_usd),
            ("30d", "月消费", s.monthly_usage_usd, s.monthly_limit_usd),
        ] {
            // 缺失用量必须保持未知，不能以 0.00 冒充服务端返回的真实零值。
            let (percent, text) = match (used, limit) {
                (Some(used), Some(limit)) if limit > 0.0 => {
                    (pct(used, limit), Some(amount(used, limit, &unit)))
                }
                (Some(used), _) => (None, Some(format!("{used:.2} {unit}"))),
                (None, Some(limit)) if limit > 0.0 => {
                    (None, Some(format!("— / {limit:.2} {unit}")))
                }
                (None, _) => (None, None),
            };
            windows.push(QuotaWindow {
                key: key.into(),
                window_name: name.into(),
                used_percent: percent,
                amount_text: text,
                // expires_at 是订阅到期时间，不是日/周/月额度的重置时刻。
                resets_at: None,
            });
        }
    }

    // 配额型分组：总配额
    if let Some(q) = &d.quota {
        let u = q.unit.clone().unwrap_or_else(|| unit.clone());
        windows.push(QuotaWindow {
            key: "quota".into(),
            window_name: "总配额".into(),
            used_percent: pct(q.used, q.limit),
            amount_text: Some(amount(q.used, q.limit, &u)),
            resets_at: None,
        });
    }

    // 限流窗口：来源已给出 window 标识，按其原样映射名称
    for r in &d.rate_limits {
        let name = match r.window.to_ascii_lowercase().as_str() {
            "5h" => "5 小时窗口".to_string(),
            "1d" => "日限额".to_string(),
            "7d" => "7 天窗口".to_string(),
            other => format!("{other} 窗口"),
        };
        windows.push(QuotaWindow {
            key: r.window.clone(),
            window_name: name,
            used_percent: pct(r.used, r.limit),
            amount_text: Some(amount(r.used, r.limit, &unit)),
            resets_at: r.reset_at.clone(),
        });
    }

    if windows.is_empty() {
        return QuotaState::Unsupported {
            reason: "该分组未提供额度窗口；钱包余额可在主面板查看".into(),
        };
    }
    QuotaState::Ok {
        windows,
        plan: d.plan_name,
    }
}

pub fn sub2api_balance(base: &str, api_key: &str) -> BalanceState {
    let d = match fetch_s2(base, api_key) {
        Ok(d) => d,
        Err((code, reason)) => return s2_balance_err(code, reason),
    };

    match d.balance {
        // 余额 0 是真实值，必须显示 0.00，不能当成「查不到」
        Some(b) => BalanceState::Ok {
            balance: b,
            currency: d.unit.unwrap_or_else(|| "USD".into()),
            used: d.quota.map(|q| q.used),
        },
        // 该分组不是钱包型，来源本就不提供余额
        None => BalanceState::Unsupported {
            reason: "该分组未提供钱包余额（可能是配额型或订阅型分组）".into(),
        },
    }
}

#[cfg(test)]
mod official_connection_tests {
    use super::*;

    #[test]
    #[ignore = "需要用户授权读取本机 Claude API 配置及其实际网关，仅输出状态"]
    fn local_claude_gateway_read_only_smoke() {
        let key = crate::creds::read_secret("claude", "api").expect("缺少本机 Claude Key");
        let base = crate::creds::local_base_url("claude", "api").expect("当前测试要求配置网关");
        let data = fetch_s2(&base, &key).unwrap_or_else(|(code, _)| panic!("HTTP/parse status {code}"));
        assert!(s2_can_validate(&data), "未返回可识别的数据");
        assert!(data.balance.is_some_and(f64::is_finite), "余额字段未知");
        println!("Claude gateway: validated=true; balance_present=true; currency={}", data.unit.as_deref().unwrap_or("unknown"));
        assert!(matches!(s2_quota_from_usage(data), QuotaState::Ok { .. } | QuotaState::Unsupported { .. }));
    }

    #[test]
    fn wallet_only_gateway_is_valid_without_quota_windows() {
        let data: S2Usage = serde_json::from_str(r#"{"balance":0,"unit":"USD","isValid":true}"#).unwrap();
        assert!(s2_can_validate(&data));
        assert!(matches!(s2_quota_from_usage(data), QuotaState::Unsupported { .. }));
        let invalid: S2Usage = serde_json::from_str(r#"{"balance":20,"isValid":false}"#).unwrap();
        assert!(!s2_can_validate(&invalid));
        assert!(!s2_can_validate(&serde_json::from_str("{}").unwrap()));
    }

    #[test]
    fn sub2api_missing_usage_stays_unknown_and_expiry_is_not_a_reset() {
        let usage: S2Usage = serde_json::from_str(r#"{
          "unit":"USD",
          "subscription":{
            "daily_limit_usd":10,
            "weekly_usage_usd":0,
            "weekly_limit_usd":70,
            "expires_at":"2026-10-01T00:00:00Z"
          }
        }"#).unwrap();
        let QuotaState::Ok { windows, .. } = s2_quota_from_usage(usage) else {
            panic!("应生成订阅窗口")
        };
        assert_eq!(windows[0].used_percent, None);
        assert_eq!(windows[0].amount_text.as_deref(), Some("— / 10.00 USD"));
        assert_eq!(windows[0].resets_at, None);
        assert_eq!(windows[1].used_percent, Some(0.0));
        assert_eq!(windows[1].resets_at, None);
    }

    #[test]
    fn gateway_without_billing_endpoint_reports_unsupported_not_failure() {
        // 404 = 网关没有该端点（Claude 式反代、本地中转都没有账单接口），
        // 属于「不支持」而非「查询失败」，否则用户会误以为是配置错误
        let reason = "该网关未提供 sub2api 账单接口（HTTP 404）".to_string();
        assert!(matches!(s2_balance_err(404, reason.clone()), BalanceState::Unsupported { .. }));
        assert!(matches!(s2_quota_err(404, reason), QuotaState::Unsupported { .. }));
        // 其余错误码语义保持不变
        assert!(matches!(s2_balance_err(500, "服务端错误".into()), BalanceState::Failed { .. }));
        assert!(matches!(s2_quota_err(500, "服务端错误".into()), QuotaState::Failed { .. }));
        assert!(matches!(s2_balance_err(401, "凭证失效".into()), BalanceState::Unauthorized { .. }));
        assert!(matches!(s2_balance_err(429, "限频".into()), BalanceState::RateLimited { .. }));
    }

    #[test]
    fn four_official_connection_modes_have_separate_capabilities() {
        assert_eq!(
            official_capabilities("claude", "auth"),
            OfficialCapabilities { quota: true, admin_usage: false, local_sessions: true }
        );
        assert_eq!(
            official_capabilities("claude", "api"),
            OfficialCapabilities { quota: false, admin_usage: true, local_sessions: true }
        );
        assert_eq!(
            official_capabilities("codex", "auth"),
            OfficialCapabilities { quota: true, admin_usage: false, local_sessions: true }
        );
        assert_eq!(
            official_capabilities("codex", "api"),
            OfficialCapabilities { quota: false, admin_usage: true, local_sessions: true }
        );
    }

    #[test]
    fn anthropic_admin_usage_counts_each_input_class_once() {
        let parsed = parse_anthropic_admin_usage(r#"{
          "data": [{"results": [{
            "uncached_input_tokens": 1500,
            "cache_read_input_tokens": 200,
            "cache_creation": {
              "ephemeral_1h_input_tokens": 30,
              "ephemeral_5m_input_tokens": 70
            },
            "output_tokens": 500
          }]}]
        }"#).expect("valid Anthropic usage response");

        assert_eq!(parsed.input_tokens, 1800);
        assert_eq!(parsed.output_tokens, 500);
        assert_eq!(parsed.cache_read_tokens, 200);
        assert_eq!(parsed.cache_write_tokens, 100);
        assert_eq!(parsed.total_tokens, 2300);
    }

    #[test]
    fn openai_admin_usage_does_not_add_cached_input_twice() {
        let parsed = parse_openai_admin_usage(r#"{
          "data": [{"results": [{
            "input_tokens": 1000,
            "output_tokens": 500,
            "input_cached_tokens": 800
          }]}]
        }"#).expect("valid OpenAI usage response");

        assert_eq!(parsed.input_tokens, 1000);
        assert_eq!(parsed.output_tokens, 500);
        assert_eq!(parsed.cache_read_tokens, 800);
        assert_eq!(parsed.cache_write_tokens, 0);
        assert_eq!(parsed.total_tokens, 1500);
    }

    #[test]
    fn admin_cost_parsers_keep_provider_units_straight() {
        let anthropic = parse_anthropic_admin_cost(r#"{
          "data": [{"results": [{"amount": "123.45", "currency": "USD"}]}]
        }"#).expect("valid Anthropic cost response");
        let openai = parse_openai_admin_cost(r#"{
          "data": [{"results": [{"amount": {"value": 1.2345, "currency": "usd"}}]}]
        }"#).expect("valid OpenAI cost response");

        assert!((anthropic - 1.2345).abs() < f64::EPSILON);
        assert!((openai - 1.2345).abs() < f64::EPSILON);
    }
    #[test]
    fn malformed_admin_responses_are_not_reported_as_zero_usage_or_cost() {
        assert!(matches!(denied_admin_usage(HttpOutcome::Body("{\"data\":[]}".into())), ApiUsageState::Forbidden { .. }));
        assert!(matches!(denied_admin_usage(HttpOutcome::Unauthorized("expired".into())), ApiUsageState::Unauthorized { .. }));
        assert!(matches!(denied_admin_usage(HttpOutcome::Failed("offline".into())), ApiUsageState::Failed { .. }));
        assert!(matches!(denied_admin_usage(HttpOutcome::Body("{}".into())), ApiUsageState::Failed { .. }));
        for body in ["{}", "{\"data\":[{}]}", "{\"data\":[{\"results\":[{}]}]}", "{\"data\":[],\"has_more\":true}"] {
            assert!(parse_anthropic_admin_usage(body).is_err(), "Claude usage accepted {body}");
            assert!(parse_openai_admin_usage(body).is_err(), "OpenAI usage accepted {body}");
            assert!(parse_anthropic_admin_cost(body).is_err(), "Claude cost accepted {body}");
            assert!(parse_openai_admin_cost(body).is_err(), "OpenAI cost accepted {body}");
        }
        assert_eq!(parse_openai_admin_usage("{\"data\":[]}").unwrap().total_tokens, 0);
        assert_eq!(parse_anthropic_admin_cost("{\"data\":[]}").unwrap(), 0.0);
        assert!(matches!(parse_codex_usage("{}"), QuotaState::Failed { .. }));
        assert!(validate_models_body("{}").is_err());
        assert!(validate_models_body("{\"error\":\"invalid key\"}").is_err());
        assert!(validate_models_body("{\"data\":[]}").is_ok());
        assert_eq!(pct(-1.0, 100.0), None);
        assert_eq!(pct(0.0, 100.0), Some(0.0));
        assert!(parse_openai_admin_cost("{\"data\":[{\"results\":[{\"amount\":{\"value\":-1}}]}]}").is_err());
    }

}
