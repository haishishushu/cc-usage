//! 编程套餐（Coding Plan）额度的只读查询（cc-switch 对齐，2026-09 源）
//!
//! | 供应商 | 端点 | 凭证 | 窗口 |
//! |---|---|---|---|
//! | Kimi For Coding | GET api.kimi.com/coding/v1/usages | Bearer | 5h + 周 |
//! | 智谱 GLM 中国版 | GET open.bigmodel.cn/api/monitor/usage/quota/limit | 裸 key | 5h(unit=3) + 周(unit=6) |
//! | 智谱 GLM 国际版 | GET api.z.ai/api/monitor/usage/quota/limit | 裸 key | 同上 |
//! | 智谱团队版 | 同中国版 + ?type=2 + 组织/项目头 | 裸 key + 组织ID + 项目ID | 同上 |
//! | MiniMax 中/国际 | GET {api.minimaxi.com\|api.minimax.io}/v1/api/openplatform/coding_plan/remains | Bearer | 5h + 周(激活时) |
//! | ZenMux | GET {base_url}（base 即用量端点） | Bearer | 5h + 7d（带美元） |
//! | OpenCode Go | GET opencode.ai/zen/go/v1/usage | Bearer | rolling(5h) + 周 + 月 |
//! | 火山方舟 | POST open.volcengineapi.com OpenAPI | AK/SK 签名 | session(5h) + 周 + 月 |
//!
//! 纪律与 [`crate::quota`] 一致：只读、15 秒超时、禁止重定向、五态区分。
//! 鉴权头差异：智谱用 `Authorization: <裸key>`（无 Bearer）；其余 Bearer；火山 AK/SK 签名。

use crate::quota::{self, QuotaState, QuotaWindow};

/// 窗口命名（key, 名称），与前端额度窗口渲染约定一致
const W5H: (&str, &str) = ("5h", "5 小时额度");
pub(crate) const W7D: (&str, &str) = ("7d", "周额度");
pub(crate) const W30D: (&str, &str) = ("30d", "月额度");
pub(crate) const WCREDITS: (&str, &str) = ("credits", "Grok 积分额度");

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanProvider {
    Kimi,
    ZhipuCn,
    ZhipuEn,
    MiniMaxCn,
    MiniMaxEn,
    ZenMux,
    OpencodeGo,
    Volcengine,
}

impl PlanProvider {
    /// 用户可读的套餐名（探测命中后的提示文案用）
    pub(crate) fn display_name(&self) -> &'static str {
        match self {
            PlanProvider::ZhipuCn => "智谱 GLM（国内站）",
            PlanProvider::ZhipuEn => "智谱 GLM（国际站）",
            PlanProvider::Kimi => "Kimi",
            PlanProvider::MiniMaxCn => "MiniMax（国内站）",
            PlanProvider::MiniMaxEn => "MiniMax（国际站）",
            PlanProvider::ZenMux => "ZenMux",
            PlanProvider::OpencodeGo => "OpenCode Go",
            PlanProvider::Volcengine => "火山方舟",
        }
    }
}

/// 按数据面 base_url 域名识别套餐供应商；不命中返回 None（维持 sub2api 路由）。
/// 智谱团队版 base_url 与个人版相同，靠 extras 显式区分，不在此识别。
pub(crate) fn detect_provider(base_url: &str) -> Option<PlanProvider> {
    let url = base_url.to_lowercase();
    if url.contains("api.kimi.com/coding") {
        Some(PlanProvider::Kimi)
    } else if url.contains("bigmodel.cn") {
        Some(PlanProvider::ZhipuCn)
    } else if url.contains("api.z.ai") {
        Some(PlanProvider::ZhipuEn)
    } else if url.contains("api.minimaxi.com") {
        Some(PlanProvider::MiniMaxCn)
    } else if url.contains("api.minimax.io") {
        Some(PlanProvider::MiniMaxEn)
    } else if url.contains("zenmux") {
        Some(PlanProvider::ZenMux)
    } else if url.contains("opencode.ai/zen/go") {
        // 覆盖 /zen/go 与 /zen/go/v1；Zen 按量版（/zen/v1）无用量 API，刻意不命中
        Some(PlanProvider::OpencodeGo)
    } else if url.contains("volces.com/api/plan") || url.contains("volces.com/api/coding") {
        // 仅套餐入口；/api/v3 与 /api/compatible 按量付费不命中
        Some(PlanProvider::Volcengine)
    } else {
        None
    }
}

/// 套餐查询的辅助凭证（团队版组织/项目 ID、火山 AK/SK），来自全局设置。
#[derive(Default, Clone, Copy)]
pub(crate) struct PlanExtras<'a> {
    pub team_organization_id: Option<&'a str>,
    pub team_project_id: Option<&'a str>,
    pub volc_access_key_id: Option<&'a str>,
    pub volc_secret_access_key: Option<&'a str>,
}

impl<'a> PlanExtras<'a> {
    fn trimmed(value: Option<&'a str>) -> Option<&'a str> {
        value.map(str::trim).filter(|s| !s.is_empty())
    }
    pub fn team_ids(&self) -> Option<(&'a str, &'a str)> {
        Some((Self::trimmed(self.team_organization_id)?, Self::trimmed(self.team_project_id)?))
    }
    pub fn volc_aksk(&self) -> Option<(&'a str, &'a str)> {
        Some((Self::trimmed(self.volc_access_key_id)?, Self::trimmed(self.volc_secret_access_key)?))
    }
    /// 从独立凭证存储构造（[`crate::settings::PlanQuerySecrets`]）。
    pub fn from_secrets(secrets: &'a crate::settings::PlanQuerySecrets) -> Self {
        Self {
            team_organization_id: Some(&secrets.zhipu_team_organization_id),
            team_project_id: Some(&secrets.zhipu_team_project_id),
            volc_access_key_id: Some(&secrets.volc_access_key_id),
            volc_secret_access_key: Some(&secrets.volc_secret_access_key),
        }
    }
}

/// 统一入口：detect_provider 命中后由 lib.rs / validate_connection 调用。
pub(crate) fn coding_plan_quota(base_url: &str, api_key: &str, extras: &PlanExtras) -> QuotaState {
    match detect_provider(base_url) {
        Some(PlanProvider::ZhipuCn) | Some(PlanProvider::ZhipuEn) => {
            let has_org = PlanExtras::trimmed(extras.team_organization_id).is_some();
            let has_project = PlanExtras::trimmed(extras.team_project_id).is_some();
            if has_org != has_project {
                return QuotaState::Failed {
                    reason: "智谱团队版需要组织 ID 与项目 ID 两项都填写（设置 → 套餐查询）；留空则按个人版查询".into(),
                };
            }
            match extras.team_ids() {
                // 团队版仅国内站；国际站带全套团队配置视为配置错误
                Some(_) if base_url.to_lowercase().contains("z.ai") => QuotaState::Unsupported {
                    reason: "智谱团队版仅存在于国内站（open.bigmodel.cn）".into(),
                },
                Some(ids) => query_zhipu(base_url, api_key, Some(ids)),
                None => query_zhipu(base_url, api_key, None), // 个人版
            }
        }
        Some(PlanProvider::Kimi) => query_kimi(api_key),
        Some(PlanProvider::MiniMaxCn) => query_minimax(api_key, true),
        Some(PlanProvider::MiniMaxEn) => query_minimax(api_key, false),
        Some(PlanProvider::ZenMux) => query_zenmux(base_url, api_key),
        Some(PlanProvider::OpencodeGo) => query_opencode_go(api_key),
        Some(PlanProvider::Volcengine) => match extras.volc_aksk() {
            Some((ak, sk)) => query_volcengine(base_url, ak, sk),
            None => QuotaState::Unsupported {
                reason: "火山套餐查询需要账号 AccessKey ID + Secret（控制面 OpenAPI，与推理 Key 是两套凭证）；请在设置 → 套餐查询中填写".into(),
            },
        },
        // detect 未命中时不应进入本函数；防御性返回而不是 panic
        None => QuotaState::Unsupported { reason: "该 base_url 未命中任何套餐供应商".into() },
    }
}

/* ─────────────── 套餐自动探测（中转/反代 base 的兜底识别） ─────────────── */

/// 参与自动探测的套餐与探测优先级。火山需要独立 AK/SK、ZenMux 的用量地址就是
/// 用户填写的 base_url 本身，都无法盲探，不参与。
const PROBE_ORDER: [PlanProvider; 6] = [
    PlanProvider::ZhipuCn,
    PlanProvider::ZhipuEn,
    PlanProvider::Kimi,
    PlanProvider::MiniMaxCn,
    PlanProvider::MiniMaxEn,
    PlanProvider::OpencodeGo,
];

/// 按探测优先级挑第一个查询成功的套餐；全不中返回 None。
/// 探测请求并行发出，完成顺序不定，必须按 PROBE_ORDER 取优先级最高的命中。
fn pick_probe_hit(results: &[(PlanProvider, QuotaState)]) -> Option<PlanProvider> {
    PROBE_ORDER.iter().find_map(|p| {
        results
            .iter()
            .any(|(hit, s)| hit == p && matches!(s, QuotaState::Ok { .. }))
            .then_some(*p)
    })
}

/// 按已识别的套餐直接查询（跳过探测）。查询端点全部是固定官方域名，与用户的
/// base_url 无关——中转/反代 base 不影响套餐查询。
pub(crate) fn query_plan_direct(provider: PlanProvider, api_key: &str) -> QuotaState {
    match provider {
        PlanProvider::ZhipuCn => query_zhipu("https://open.bigmodel.cn", api_key, None),
        PlanProvider::ZhipuEn => query_zhipu("https://api.z.ai", api_key, None),
        PlanProvider::Kimi => query_kimi(api_key),
        PlanProvider::MiniMaxCn => query_minimax(api_key, true),
        PlanProvider::MiniMaxEn => query_minimax(api_key, false),
        PlanProvider::OpencodeGo => query_opencode_go(api_key),
        // 不参与探测的两种：调用方不会传入，防御性返回
        PlanProvider::ZenMux | PlanProvider::Volcengine => QuotaState::Unsupported {
            reason: "该套餐无法自动识别，请将 base_url 填写为套餐官方地址".into(),
        },
    }
}

fn probe_cache() -> &'static std::sync::Mutex<std::collections::HashMap<String, PlanProvider>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, PlanProvider>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// 逐家用官方端点并行试查，识别 key 所属的编程套餐，命中结果进程内缓存。
/// 仅在 sub2api 账单端点 404（中转/反代 base 按域名认不出套餐）时调用：
/// 这类网关的 key 往往就是某家套餐的官方 key（实测智谱套餐经本地中转即如此）。
/// 探测只发各家的正常用量查询（只读 GET），401/404 视为「不是这家」。
pub(crate) fn probe_plan_cached(api_key: &str) -> Option<PlanProvider> {
    if let Ok(cache) = probe_cache().lock() {
        if let Some(hit) = cache.get(api_key) {
            return Some(*hit);
        }
    }
    let mut handles = Vec::new();
    for provider in PROBE_ORDER {
        let key = api_key.to_string();
        handles.push(std::thread::spawn(move || {
            let state = query_plan_direct(provider, &key);
            (provider, state)
        }));
    }
    let mut results = Vec::new();
    for handle in handles {
        if let Ok(pair) = handle.join() {
            results.push(pair);
        }
    }
    let hit = pick_probe_hit(&results)?;
    if let Ok(mut cache) = probe_cache().lock() {
        cache.insert(api_key.to_string(), hit);
    }
    Some(hit)
}

/* ─────────────────────── 智谱 GLM ─────────────────────── */

/// 控制台同源监控端点（非公开文档 API）。中国/国际站同路径同 JSON 形态。
const ZHIPU_QUOTA_PATH: &str = "/api/monitor/usage/quota/limit";

fn zhipu_quota_base(base_url: &str) -> &'static str {
    if base_url.to_lowercase().contains("bigmodel.cn") { "https://open.bigmodel.cn" } else { "https://api.z.ai" }
}

fn millis_to_iso(ms: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(ms / 1000, ((ms % 1000) * 1_000_000) as u32).map(|t| t.to_rfc3339())
}

/// JSON 值 → ISO 重置时间：字符串原样；数字区分秒（<1e12）与毫秒；≤0 视为无。
fn extract_reset_time(value: &serde_json::Value) -> Option<String> {
    if let Some(s) = value.as_str() { return Some(s.to_string()); }
    if let Some(n) = value.as_i64() {
        if n <= 0 { return None; }
        let ms = if n < 1_000_000_000_000 { n * 1000 } else { n };
        return millis_to_iso(ms);
    }
    None
}

/// 智谱条目解析中间态
struct ZhipuEntry {
    reset_ms: Option<i64>,
    percent: Option<f64>,
    amount: Option<String>,
}

/// 智谱条目按 `unit` 显式分类：3=5 小时滚动窗，6=周窗（number 有 5/7/1 多种实测，
/// 只锚定 unit）。缺失或不识别时走兜底：无 reset 优先归 5h，其余按 reset 升序补位。
fn parse_zhipu_windows(data: &serde_json::Value) -> (Vec<QuotaWindow>, Option<String>) {
    let classify = |item: &serde_json::Value| match item.get("unit").and_then(|v| v.as_i64()) {
        Some(3) => Some(0usize), // 0 = 5h 槽位
        Some(6) => Some(1usize), // 1 = 周槽位
        _ => None,
    };
    let mut slots: [Option<ZhipuEntry>; 2] = [None, None];
    let mut unclassified: Vec<ZhipuEntry> = Vec::new();

    if let Some(limits) = data.get("limits").and_then(|v| v.as_array()) {
        for item in limits {
            let kind = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if !(kind.eq_ignore_ascii_case("TOKENS_LIMIT") || kind.eq_ignore_ascii_case("CREDIT_LIMIT")) {
                continue;
            }
            // credits 用量（智谱新接口）：currentValue 已用 / usage 总量
            let amount = match (
                item.get("currentValue").and_then(|v| v.as_f64()),
                item.get("usage").and_then(|v| v.as_f64()),
            ) {
                (Some(used), Some(total)) if total > 0.0 && used.is_finite() && used >= 0.0 => {
                    Some(format!("{used:.0} / {total:.0} credits"))
                }
                _ => None,
            };
            let entry = ZhipuEntry {
                reset_ms: item.get("nextResetTime").and_then(|v| v.as_i64()),
                percent: item.get("percentage").and_then(|v| v.as_f64())
                    .filter(|v| v.is_finite() && (0.0..=100.0).contains(v)),
                amount,
            };
            match classify(item) {
                Some(slot) if slots[slot].is_none() => slots[slot] = Some(entry),
                _ => unclassified.push(entry),
            }
        }
    }
    // 兜底：无 reset 的优先归 5h（5h 桶 0% 等状态可能没有 nextResetTime），其余按 reset 升序
    unclassified.sort_by_key(|e| (e.reset_ms.is_some(), e.reset_ms.unwrap_or(i64::MIN)));
    for entry in unclassified {
        let slot = if slots[0].is_none() { 0 } else if slots[1].is_none() { 1 } else { break };
        slots[slot] = Some(entry);
    }

    let mut windows = Vec::new();
    for ((key, name), slot) in [W5H, W7D].into_iter().zip(slots) {
        if let Some(e) = slot {
            windows.push(QuotaWindow {
                key: key.into(),
                window_name: name.into(),
                used_percent: e.percent,
                amount_text: e.amount,
                resets_at: e.reset_ms.and_then(millis_to_iso),
            });
        }
    }
    let level = data.get("level").and_then(|v| v.as_str()).map(str::to_string);
    (windows, level)
}

/// query 的解析半区（业务错误 → Failed），供查询与单测共用。
fn zhipu_state_from_body(body: &serde_json::Value) -> QuotaState {
    if body.get("success").and_then(|v| v.as_bool()) == Some(false) {
        let msg = body.get("msg").and_then(|v| v.as_str()).unwrap_or("Unknown error");
        return QuotaState::Failed { reason: format!("智谱套餐接口错误: {msg}") };
    }
    let Some(data) = body.get("data") else {
        return QuotaState::Failed { reason: "智谱套餐响应缺少 data 字段".into() };
    };
    let (windows, level) = parse_zhipu_windows(data);
    if windows.is_empty() {
        return QuotaState::Failed { reason: "智谱未返回可识别的套餐额度窗口".into() };
    }
    QuotaState::Ok { windows, plan: level }
}

/// 查询智谱套餐额度。team 为 Some((org, project)) 时走团队版（?type=2 + 组织/项目请求头）。
fn query_zhipu(base_url: &str, api_key: &str, team: Option<(&str, &str)>) -> QuotaState {
    let c = match quota::client() { Ok(c) => c, Err(e) => return QuotaState::Failed { reason: e } };
    let mut url = format!("{}{ZHIPU_QUOTA_PATH}", zhipu_quota_base(base_url));
    if team.is_some() { url.push_str("?type=2"); }
    let mut request = c.get(&url)
        .header("Authorization", api_key) // 智谱不加 Bearer 前缀
        .header("Content-Type", "application/json")
        .header("Accept-Language", "en-US,en");
    if let Some((org, project)) = team {
        request = request.header("bigmodel-organization", org).header("bigmodel-project", project);
    }
    match request.send() {
        Ok(r) => match quota::read(r) {
            quota::HttpOutcome::Body(body) => match serde_json::from_str::<serde_json::Value>(&body) {
                Ok(v) => zhipu_state_from_body(&v),
                Err(e) => QuotaState::Failed { reason: format!("智谱套餐响应格式无法解析: {e}") },
            },
            quota::HttpOutcome::Unauthorized(reason) => QuotaState::Unauthorized { reason },
            quota::HttpOutcome::Forbidden(reason) => QuotaState::Forbidden { reason },
            quota::HttpOutcome::RateLimited(reason) => QuotaState::RateLimited { reason },
            quota::HttpOutcome::Failed(reason) => QuotaState::Failed { reason: format!("智谱套餐查询失败: {reason}") },
        },
        Err(e) if e.is_timeout() => QuotaState::Failed { reason: "智谱套餐查询超时（15 秒）".into() },
        Err(_) => QuotaState::Failed { reason: "智谱套餐查询失败，请检查网络后刷新".into() },
    }
}

/* ─────────────────────── Kimi For Coding ─────────────────────── */

/// `limits[].detail` 为 5h 桶（limit/remaining 绝对值），顶层 `usage` 为周桶；
/// resetTime 兼容 ISO 字符串与秒/毫秒时间戳。usage 缺失 → 无周桶。
fn parse_kimi_windows(body: &serde_json::Value) -> Vec<QuotaWindow> {
    // limit/remaining 是绝对值；limit=0 视为空窗按 0% 处理（cc-switch 同款语义）
    let tier = |limit: f64, remaining: f64| {
        let used = (limit - remaining).max(0.0);
        let percent = if limit > 0.0 { used / limit * 100.0 } else { 0.0 };
        Some(percent).filter(|v| v.is_finite() && (0.0..=100.0).contains(v))
    };
    let mut windows = Vec::new();
    if let Some(limits) = body.get("limits").and_then(|v| v.as_array()) {
        for item in limits {
            let Some(detail) = item.get("detail") else { continue };
            windows.push(QuotaWindow {
                key: W5H.0.into(),
                window_name: W5H.1.into(),
                used_percent: tier(
                    detail.get("limit").and_then(|v| v.as_f64()).unwrap_or(1.0),
                    detail.get("remaining").and_then(|v| v.as_f64()).unwrap_or(0.0),
                ),
                amount_text: None,
                resets_at: detail.get("resetTime").and_then(extract_reset_time),
            });
        }
    }
    if let Some(usage) = body.get("usage") {
        windows.push(QuotaWindow {
            key: W7D.0.into(),
            window_name: W7D.1.into(),
            used_percent: tier(
                usage.get("limit").and_then(|v| v.as_f64()).unwrap_or(1.0),
                usage.get("remaining").and_then(|v| v.as_f64()).unwrap_or(0.0),
            ),
            amount_text: None,
            resets_at: usage.get("resetTime").and_then(extract_reset_time),
        });
    }
    windows
}

fn query_kimi(api_key: &str) -> QuotaState {
    let c = match quota::client() { Ok(c) => c, Err(e) => return QuotaState::Failed { reason: e } };
    match c.get("https://api.kimi.com/coding/v1/usages")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Accept", "application/json")
        .send()
    {
        Ok(r) => match quota::read(r) {
            quota::HttpOutcome::Body(body) => match serde_json::from_str::<serde_json::Value>(&body) {
                Ok(v) => {
                    let windows = parse_kimi_windows(&v);
                    if windows.is_empty() { QuotaState::Failed { reason: "Kimi 未返回可识别的额度窗口".into() } }
                    else { QuotaState::Ok { windows, plan: None } }
                }
                Err(e) => QuotaState::Failed { reason: format!("Kimi 用量响应格式无法解析: {e}") },
            },
            quota::HttpOutcome::Unauthorized(reason) => QuotaState::Unauthorized { reason },
            quota::HttpOutcome::Forbidden(reason) => QuotaState::Forbidden { reason },
            quota::HttpOutcome::RateLimited(reason) => QuotaState::RateLimited { reason },
            quota::HttpOutcome::Failed(reason) => QuotaState::Failed { reason: format!("Kimi 用量查询失败: {reason}") },
        },
        Err(e) if e.is_timeout() => QuotaState::Failed { reason: "Kimi 用量查询超时（15 秒）".into() },
        Err(_) => QuotaState::Failed { reason: "Kimi 用量查询失败，请检查网络后刷新".into() },
    }
}

/* ─────────────────────── MiniMax ─────────────────────── */

/// 编程套餐剩余百分比在 `model_remains` 的 `general` 条目；`current_*_remaining_percent`
/// 是「剩余」，反转为已用。周桶仅 `current_weekly_status==1` 时存在（3=无周限额）。
fn parse_minimax_windows(body: &serde_json::Value) -> Vec<QuotaWindow> {
    let mut windows = Vec::new();
    let Some(items) = body.get("model_remains").and_then(|v| v.as_array()) else { return windows };
    let Some(item) = items.iter().find(|i| i.get("model_name").and_then(|v| v.as_str()) == Some("general")) else { return windows };
    let used_of = |remain: f64| (100.0 - remain).clamp(0.0, 100.0);
    if let Some(remain) = item.get("current_interval_remaining_percent").and_then(|v| v.as_f64()) {
        windows.push(QuotaWindow {
            key: W5H.0.into(),
            window_name: W5H.1.into(),
            used_percent: Some(used_of(remain)),
            amount_text: None,
            resets_at: item.get("end_time").and_then(|v| v.as_i64()).and_then(millis_to_iso),
        });
    }
    if item.get("current_weekly_status").and_then(|v| v.as_i64()) == Some(1) {
        if let Some(remain) = item.get("current_weekly_remaining_percent").and_then(|v| v.as_f64()) {
            windows.push(QuotaWindow {
                key: W7D.0.into(),
                window_name: W7D.1.into(),
                used_percent: Some(used_of(remain)),
                amount_text: None,
                resets_at: item.get("weekly_end_time").and_then(|v| v.as_i64()).and_then(millis_to_iso),
            });
        }
    }
    windows
}

fn query_minimax(api_key: &str, cn: bool) -> QuotaState {
    let host = if cn { "api.minimaxi.com" } else { "api.minimax.io" };
    let c = match quota::client() { Ok(c) => c, Err(e) => return QuotaState::Failed { reason: e } };
    match c.get(&format!("https://{host}/v1/api/openplatform/coding_plan/remains"))
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .send()
    {
        Ok(r) => match quota::read(r) {
            quota::HttpOutcome::Body(body) => match serde_json::from_str::<serde_json::Value>(&body) {
                Ok(v) => {
                    if let Some(base_resp) = v.get("base_resp") {
                        let code = base_resp.get("status_code").and_then(|v| v.as_i64()).unwrap_or(-1);
                        if code != 0 {
                            let msg = base_resp.get("status_msg").and_then(|v| v.as_str()).unwrap_or("Unknown error");
                            return QuotaState::Failed { reason: format!("MiniMax 套餐接口错误（code {code}）: {msg}") };
                        }
                    }
                    let windows = parse_minimax_windows(&v);
                    if windows.is_empty() { QuotaState::Failed { reason: "MiniMax 未返回可识别的额度窗口".into() } }
                    else { QuotaState::Ok { windows, plan: None } }
                }
                Err(e) => QuotaState::Failed { reason: format!("MiniMax 用量响应格式无法解析: {e}") },
            },
            quota::HttpOutcome::Unauthorized(reason) => QuotaState::Unauthorized { reason },
            quota::HttpOutcome::Forbidden(reason) => QuotaState::Forbidden { reason },
            quota::HttpOutcome::RateLimited(reason) => QuotaState::RateLimited { reason },
            quota::HttpOutcome::Failed(reason) => QuotaState::Failed { reason: format!("MiniMax 用量查询失败: {reason}") },
        },
        Err(e) if e.is_timeout() => QuotaState::Failed { reason: "MiniMax 用量查询超时（15 秒）".into() },
        Err(_) => QuotaState::Failed { reason: "MiniMax 用量查询失败，请检查网络后刷新".into() },
    }
}

/* ─────────────────────── ZenMux ─────────────────────── */

/// base_url 本身就是用量端点；usage_percentage 为 0–1 小数；5h 桶带美元已用/上限。
fn parse_zenmux_windows(data: &serde_json::Value) -> (Vec<QuotaWindow>, Option<String>) {
    let mut windows = Vec::new();
    let tier = |key: (&str, &str), node: Option<&serde_json::Value>| {
        let Some(q) = node else { return None };
        Some(QuotaWindow {
            key: key.0.into(),
            window_name: key.1.into(),
            used_percent: q.get("usage_percentage").and_then(|v| v.as_f64())
                .map(|p| p * 100.0)
                .filter(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            amount_text: match (
                q.get("used_value_usd").and_then(|v| v.as_f64()),
                q.get("max_value_usd").and_then(|v| v.as_f64()),
            ) {
                (Some(used), Some(max)) if max > 0.0 && used.is_finite() => Some(format!("${used:.2} / ${max:.2}")),
                _ => None,
            },
            resets_at: q.get("resets_at").and_then(|v| v.as_str()).map(str::to_string),
        })
    };
    windows.extend(tier(W5H, data.get("quota_5_hour")));
    windows.extend(tier(W7D, data.get("quota_7_day")));
    let plan = data.get("plan").and_then(|p| p.get("tier")).and_then(|v| v.as_str()).map(|tier| {
        let status = data.get("account_status").and_then(|v| v.as_str()).unwrap_or("");
        if status.is_empty() { tier.to_string() } else { format!("{tier} ({status})") }
    });
    (windows, plan)
}

fn query_zenmux(base_url: &str, api_key: &str) -> QuotaState {
    let c = match quota::client() { Ok(c) => c, Err(e) => return QuotaState::Failed { reason: e } };
    match c.get(base_url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Accept", "application/json")
        .send()
    {
        Ok(r) => match quota::read(r) {
            quota::HttpOutcome::Body(body) => match serde_json::from_str::<serde_json::Value>(&body) {
                Ok(v) => {
                    if v.get("success").and_then(|v| v.as_bool()) != Some(true) {
                        let msg = v.get("message").and_then(|v| v.as_str()).unwrap_or("Unknown error");
                        return QuotaState::Failed { reason: format!("ZenMux 套餐接口错误: {msg}") };
                    }
                    match v.get("data") {
                        Some(data) => {
                            let (windows, plan) = parse_zenmux_windows(data);
                            if windows.is_empty() { QuotaState::Failed { reason: "ZenMux 未返回可识别的额度窗口".into() } }
                            else { QuotaState::Ok { windows, plan } }
                        }
                        None => QuotaState::Failed { reason: "ZenMux 响应缺少 data 字段".into() },
                    }
                }
                Err(e) => QuotaState::Failed { reason: format!("ZenMux 用量响应格式无法解析: {e}") },
            },
            quota::HttpOutcome::Unauthorized(reason) => QuotaState::Unauthorized { reason },
            quota::HttpOutcome::Forbidden(reason) => QuotaState::Forbidden { reason },
            quota::HttpOutcome::RateLimited(reason) => QuotaState::RateLimited { reason },
            quota::HttpOutcome::Failed(reason) => QuotaState::Failed { reason: format!("ZenMux 用量查询失败: {reason}") },
        },
        Err(e) if e.is_timeout() => QuotaState::Failed { reason: "ZenMux 用量查询超时（15 秒）".into() },
        Err(_) => QuotaState::Failed { reason: "ZenMux 用量查询失败，请检查网络后刷新".into() },
    }
}

/* ─────────────────────── OpenCode Go ─────────────────────── */

/// 第一方但未文档化的路由（上线次日就改过一次形态），逐窗口防御解析：
/// 缺失或 percent 不可解析的窗口跳过；全空由调用方按「形态不认识」报错。
fn parse_opencode_windows(body: &serde_json::Value) -> Vec<QuotaWindow> {
    const WINDOWS: [(&str, &str, &str); 3] = [
        ("rolling", W5H.0, W5H.1),
        ("weekly", W7D.0, W7D.1),
        ("monthly", W30D.0, W30D.1),
    ];
    let Some(usage) = body.get("usage") else { return Vec::new() };
    let mut windows = Vec::new();
    for (node_key, key, name) in WINDOWS {
        let Some(w) = usage.get(node_key) else { continue };
        let Some(percent) = w.get("percent").and_then(|v| v.as_f64()) else { continue };
        windows.push(QuotaWindow {
            key: key.into(),
            window_name: name.into(),
            used_percent: Some(percent).filter(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            amount_text: None,
            // percent=0 时 resetsAt 是占位值（滚动窗按最后记账时间整窗清零，窗口早已过期）
            resets_at: if percent > 0.0 { w.get("resetsAt").and_then(extract_reset_time) } else { None },
        });
    }
    windows
}

/// 用量端点只认 `Authorization: Bearer`（与推理侧 x-api-key 正好相反，不能互换）。
fn query_opencode_go(api_key: &str) -> QuotaState {
    let c = match quota::client() { Ok(c) => c, Err(e) => return QuotaState::Failed { reason: e } };
    let send = c.get("https://opencode.ai/zen/go/v1/usage")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Accept", "application/json")
        .send();
    match send {
        Ok(r) => {
            // 403 EntitlementError：key 本身有效（Zen 与 Go 共用同一把 workspace key），
            // 但该工作区没有 Go 订阅——与 401 认证失败分开提示。
            if r.status().as_u16() == 403 {
                return QuotaState::Forbidden {
                    reason: "Key 有效但该工作区未订阅 OpenCode Go（HTTP 403）".into(),
                };
            }
            match quota::read(r) {
                quota::HttpOutcome::Body(body) => match serde_json::from_str::<serde_json::Value>(&body) {
                    Ok(v) => {
                        let windows = parse_opencode_windows(&v);
                        if windows.is_empty() {
                            QuotaState::Failed { reason: "OpenCode Go 用量响应形态不认识（上游曾变更过结构）".into() }
                        } else {
                            QuotaState::Ok { windows, plan: None }
                        }
                    }
                    Err(e) => QuotaState::Failed { reason: format!("OpenCode Go 用量响应格式无法解析: {e}") },
                },
                quota::HttpOutcome::Unauthorized(reason) => QuotaState::Unauthorized { reason },
                quota::HttpOutcome::Forbidden(reason) => QuotaState::Forbidden { reason },
                quota::HttpOutcome::RateLimited(reason) => QuotaState::RateLimited { reason },
                quota::HttpOutcome::Failed(reason) => QuotaState::Failed { reason: format!("OpenCode Go 用量查询失败: {reason}") },
            }
        }
        Err(e) if e.is_timeout() => QuotaState::Failed { reason: "OpenCode Go 用量查询超时（15 秒）".into() },
        Err(_) => QuotaState::Failed { reason: "OpenCode Go 用量查询失败，请检查网络后刷新".into() },
    }
}

/* ─────────────────────── 火山方舟 Agent Plan / Coding Plan ─────────────────────── */
//
// 控制面 OpenAPI（open.volcengineapi.com，非数据面 ark.cn-beijing.volces.com），
// 强制火山引擎签名 V4（AK/SK）——实测复用推理 Bearer Key 会被网关以
// 400 InvalidAuthorization 拒绝。签名是 AWS SigV4 的火山变体，两处致命差异：
//   1. canonical headers 与 SignedHeaders 用固定顺序
//      host;x-date;x-content-sha256;content-type（不按字母序）
//   2. algorithm 串 HMAC-SHA256（无 AWS4 前缀）、credential scope 终止 request
//      （非 aws4_request）、签名密钥 kDate=HMAC(SK, date)（SK 不加 AWS4 前缀）

const VOLCENGINE_OPENAPI_HOST: &str = "open.volcengineapi.com";
const VOLCENGINE_API_VERSION: &str = "2024-01-01";
const VOLCENGINE_DEFAULT_REGION: &str = "cn-beijing";
const VOLCENGINE_SERVICE: &str = "ark";
const VOLCENGINE_CONTENT_TYPE: &str = "application/json; charset=utf-8";
const VOLCENGINE_SIGNED_HEADERS: &str = "host;x-date;x-content-sha256;content-type";

/// 从数据面 base_url 提取控制面所需的 Region（如 ark.cn-beijing.volces.com → cn-beijing）；
/// 无法识别时回落 cn-beijing。控制面 Host 固定，不随 base_url 变化。
fn volcengine_region(base_url: &str) -> String {
    let host = base_url
        .split_once("://").map(|(_, rest)| rest).unwrap_or(base_url)
        .split('/').next().unwrap_or("");
    host.split('.')
        .find(|p| p.starts_with("cn-") || p.starts_with("ap-"))
        .map(str::to_string)
        .unwrap_or_else(|| VOLCENGINE_DEFAULT_REGION.to_string())
}

fn volc_hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    type HmacSha256 = Hmac<sha2::Sha256>;
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn volc_sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(data))
}

/// RFC3986 unreserved 之外全部按 %XX 编码（canonical query 用）。
fn volc_uri_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => {
                use std::fmt::Write;
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

/// 按 key 字母序排序、逐段 URL 编码；同一份字符串既用于签名也用于实际 URL，
/// 保证两者完全一致（否则签名不匹配）。
fn volcengine_canonical_query(action: &str, region: &str) -> String {
    let mut pairs = [("Action", action), ("Region", region), ("Version", VOLCENGINE_API_VERSION)];
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    pairs.iter().map(|(k, v)| format!("{}={}", volc_uri_encode(k), volc_uri_encode(v))).collect::<Vec<_>>().join("&")
}

/// 生成火山引擎签名 V4 的 (Authorization, X-Date, X-Content-Sha256)，三者都必须随请求发送。
/// `now` 作参数传入保证签名可做确定性单测。
fn volcengine_sign(
    ak: &str,
    sk: &str,
    region: &str,
    canonical_query: &str,
    body: &[u8],
    now: chrono::DateTime<chrono::Utc>,
) -> (String, String, String) {
    let x_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let short_date = now.format("%Y%m%d").to_string();
    let x_content_sha256 = volc_sha256_hex(body);
    // 固定顺序 canonical headers（火山特有，不排序）
    let canonical_headers = format!(
        "host:{VOLCENGINE_OPENAPI_HOST}\nx-date:{x_date}\nx-content-sha256:{x_content_sha256}\ncontent-type:{VOLCENGINE_CONTENT_TYPE}\n"
    );
    let canonical_request = format!(
        "POST\n/\n{canonical_query}\n{canonical_headers}\n{VOLCENGINE_SIGNED_HEADERS}\n{x_content_sha256}"
    );
    let credential_scope = format!("{short_date}/{region}/{VOLCENGINE_SERVICE}/request");
    let string_to_sign = format!(
        "HMAC-SHA256\n{x_date}\n{credential_scope}\n{}",
        volc_sha256_hex(canonical_request.as_bytes())
    );
    // 签名密钥派生：kDate=HMAC(SK, date)（SK 不加 AWS4 前缀），终止串 request
    let k_date = volc_hmac_sha256(sk.as_bytes(), short_date.as_bytes());
    let k_region = volc_hmac_sha256(&k_date, region.as_bytes());
    let k_service = volc_hmac_sha256(&k_region, VOLCENGINE_SERVICE.as_bytes());
    let k_signing = volc_hmac_sha256(&k_service, b"request");
    let signature: String = volc_hmac_sha256(&k_signing, string_to_sign.as_bytes())
        .iter().map(|b| format!("{b:02x}")).collect();
    (
        format!("HMAC-SHA256 Credential={ak}/{credential_scope}, SignedHeaders={VOLCENGINE_SIGNED_HEADERS}, Signature={signature}"),
        x_date,
        x_content_sha256,
    )
}

const VOLCENGINE_AKSK_HINT: &str =
    "请检查 AccessKey ID / Secret 是否正确，且账号具备方舟用量查询（OpenAPI）权限";

/// OpenAPI 错误信封（ResponseMetadata.Error 或顶层 Error）。
fn volcengine_response_error(body: &serde_json::Value) -> Option<(String, String)> {
    let err = body.get("ResponseMetadata").and_then(|m| m.get("Error")).or_else(|| body.get("Error"))?;
    let code = err.get("Code").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let msg = err.get("Message").and_then(|v| v.as_str()).unwrap_or("").to_string();
    (!code.is_empty() || !msg.is_empty()).then_some((code, msg))
}

/// 签名/凭据类错误码 → 硬停提示换 AK/SK（两个 plan 共用凭据，命中即停）。
fn volcengine_is_auth_error_code(code: &str) -> bool {
    let c = code.to_lowercase();
    ["auth", "signature", "denied", "unauthorized", "forbidden", "credential", "token"]
        .iter().any(|k| c.contains(k))
}

/// 单次控制面调用的归类结果。
enum VolcCall {
    /// 2xx 且 JSON 可解析、无 OpenAPI 级错误（业务 Result 仍可能为空 = 未订阅）
    Body(serde_json::Value),
    /// 硬鉴权失败——两个 plan 共用凭据，命中即停
    Auth(String),
    /// 非鉴权 HTTP 错误 / 响应体非法 JSON——记录后可继续尝试另一个 plan
    Soft(String),
}

/// 单次控制面调用：POST https://open.volcengineapi.com/?Action=...&Version=2024-01-01&Region=...
/// 空 body。火山对签名类错误常回 4xx + Error 信封（而非 401/403），两条路径都要解析信封。
fn volcengine_openapi_call(region: &str, ak: &str, sk: &str, action: &str) -> VolcCall {
    let c = match quota::client() { Ok(c) => c, Err(e) => return VolcCall::Soft(e) };
    let canonical_query = volcengine_canonical_query(action, region);
    let url = format!("https://{VOLCENGINE_OPENAPI_HOST}/?{canonical_query}");
    let body: &[u8] = b"";
    let (authorization, x_date, x_content_sha256) =
        volcengine_sign(ak, sk, region, &canonical_query, body, chrono::Utc::now());
    let resp = match c.post(&url)
        .header("X-Date", x_date)
        .header("X-Content-Sha256", x_content_sha256)
        .header("Content-Type", VOLCENGINE_CONTENT_TYPE)
        .header("Authorization", authorization)
        .body(body.to_vec())
        .timeout(std::time::Duration::from_secs(15))
        .send()
    {
        Ok(r) => r,
        Err(e) if e.is_timeout() => return VolcCall::Soft("查询超时（15 秒）".into()),
        Err(e) => return VolcCall::Soft(format!("网络错误: {e}")),
    };
    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return VolcCall::Auth(format!("火山鉴权失败（HTTP {status}）。{VOLCENGINE_AKSK_HINT}"));
    }
    let raw = match resp.text() {
        Ok(t) => t,
        Err(e) => return VolcCall::Soft(format!("读取响应失败: {e}")),
    };
    let parsed: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => {
            let head: String = raw.chars().take(200).collect();
            return VolcCall::Soft(format!("API error (HTTP {status}): {head}"));
        }
    };
    // 业务错误常以 200 + ResponseMetadata.Error 返回
    if let Some((code, msg)) = volcengine_response_error(&parsed) {
        if volcengine_is_auth_error_code(&code) {
            return VolcCall::Auth(format!("火山鉴权失败（{code}: {msg}）。{VOLCENGINE_AKSK_HINT}"));
        }
        return VolcCall::Soft(format!("API error ({code}): {msg}"));
    }
    if !status.is_success() {
        return VolcCall::Soft(format!("API error (HTTP {})", status.as_u16()));
    }
    VolcCall::Body(parsed)
}

/// Agent Plan（GetAFPUsage）：Quota/Used 绝对值；AFPDaily 官方控制台也隐藏（历史
/// 默认值非强制限额），跳过；Quota<=0 视为未订阅/未启用——也用于识别「已鉴权但
/// 无 Agent Plan」从而回落 Coding Plan 探测。
fn parse_afp_windows(result: &serde_json::Value) -> Vec<QuotaWindow> {
    let mut windows = Vec::new();
    for (node_key, key, name) in [("AFPFiveHour", W5H.0, W5H.1), ("AFPWeekly", W7D.0, W7D.1), ("AFPMonthly", W30D.0, W30D.1)] {
        let Some(win) = result.get(node_key) else { continue };
        let quota_v = win.get("Quota").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if quota_v <= 0.0 { continue; }
        let used = win.get("Used").and_then(|v| v.as_f64()).unwrap_or(0.0);
        windows.push(QuotaWindow {
            key: key.into(),
            window_name: name.into(),
            used_percent: Some(used / quota_v * 100.0).filter(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            amount_text: None,
            resets_at: win.get("ResetTime").and_then(extract_reset_time),
        });
    }
    windows
}

/// Coding Plan（GetCodingPlanUsage）：Level 标签 → 窗口，只给已用百分比，
/// 防御式兼容多字段名（实测字段为 Level，其余作 fallback）。
fn parse_coding_plan_windows(result: &serde_json::Value) -> Vec<QuotaWindow> {
    fn window_label(label: &str) -> Option<(&'static str, &'static str)> {
        match label.to_lowercase().as_str() {
            "session" | "5h" | "fivehour" | "five_hour" | "rolling_5h" => Some(W5H),
            "weekly" | "week" | "7d" => Some(W7D),
            "monthly" | "month" => Some(W30D),
            _ => None,
        }
    }
    let mut windows = Vec::new();
    let arr = result.get("QuotaUsage").and_then(|v| v.as_array())
        .or_else(|| result.get("Usages").and_then(|v| v.as_array()))
        .or_else(|| result.get("Details").and_then(|v| v.as_array()));
    let Some(arr) = arr else { return windows };
    for item in arr {
        let label = ["Level", "Type", "Period", "Label", "Window"].iter()
            .find_map(|k| item.get(*k).and_then(|v| v.as_str())).unwrap_or("");
        let Some((key, name)) = window_label(label) else { continue };
        let percent = ["Percent", "UsedPercent", "UsagePercent"].iter()
            .find_map(|k| item.get(*k).and_then(|v| v.as_f64())).unwrap_or(0.0);
        windows.push(QuotaWindow {
            key: key.into(),
            window_name: name.into(),
            used_percent: Some(percent).filter(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            amount_text: None,
            resets_at: item.get("ResetTime").or_else(|| item.get("ResetTimestamp")).and_then(extract_reset_time),
        });
    }
    windows
}

/// 双 plan 自动探测：先 GetAFPUsage（Agent Plan），无额度再 GetCodingPlanUsage
/// （Coding Plan）。鉴权类错误直接停（共用同一份 AK/SK）。
fn query_volcengine(base_url: &str, ak: &str, sk: &str) -> QuotaState {
    let region = volcengine_region(base_url);
    let mut soft_errors: Vec<String> = Vec::new();
    match volcengine_openapi_call(&region, ak, sk, "GetAFPUsage") {
        VolcCall::Auth(reason) => return QuotaState::Unauthorized { reason },
        VolcCall::Soft(reason) => soft_errors.push(format!("GetAFPUsage: {reason}")),
        VolcCall::Body(body) => {
            let result = body.get("Result").unwrap_or(&body);
            let windows = parse_afp_windows(result);
            if !windows.is_empty() {
                let plan = result.get("PlanType").and_then(|v| v.as_str())
                    .map(str::trim).filter(|s| !s.is_empty())
                    .map(|s| format!("Agent Plan {s}"));
                return QuotaState::Ok { windows, plan };
            }
        }
    }
    match volcengine_openapi_call(&region, ak, sk, "GetCodingPlanUsage") {
        VolcCall::Auth(reason) => return QuotaState::Unauthorized { reason },
        VolcCall::Soft(reason) => soft_errors.push(format!("GetCodingPlanUsage: {reason}")),
        VolcCall::Body(body) => {
            let result = body.get("Result").unwrap_or(&body);
            let windows = parse_coding_plan_windows(result);
            if !windows.is_empty() {
                return QuotaState::Ok { windows, plan: Some("Coding Plan".into()) };
            }
        }
    }
    if soft_errors.is_empty() {
        QuotaState::Failed { reason: "该凭证下未发现有效的 Agent Plan 或 Coding Plan 订阅".into() }
    } else {
        QuotaState::Failed { reason: soft_errors.join("; ") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::QuotaWindow;

    #[test]
    fn pick_probe_hit_prefers_priority_order_over_completion_order() {
        let ok = |key: &str| QuotaState::Ok { windows: vec![QuotaWindow {
            key: key.into(), window_name: "w".into(), used_percent: Some(1.0), amount_text: None, resets_at: None,
        }], plan: None };
        let failed = |reason: &str| QuotaState::Failed { reason: reason.into() };
        // 完成顺序打乱：低优先级先完成也必须按 PROBE_ORDER 优先级取智谱
        let results = vec![
            (PlanProvider::OpencodeGo, ok("go")),
            (PlanProvider::ZhipuEn, ok("zai")),
            (PlanProvider::ZhipuCn, ok("bigmodel")),
        ];
        assert!(matches!(pick_probe_hit(&results), Some(PlanProvider::ZhipuCn)));
        // 首选全部失败时取优先级最高的成功项
        let results = vec![
            (PlanProvider::ZhipuCn, failed("401")),
            (PlanProvider::MiniMaxCn, ok("minimax")),
        ];
        assert!(matches!(pick_probe_hit(&results), Some(PlanProvider::MiniMaxCn)));
        // 全不中
        let results = vec![(PlanProvider::Kimi, failed("401"))];
        assert!(pick_probe_hit(&results).is_none());
        assert!(pick_probe_hit(&[]).is_none());
    }

    #[test]
    fn detect_provider_routes_by_base_url_domain() {
        assert!(matches!(detect_provider("https://api.kimi.com/coding/v1"), Some(PlanProvider::Kimi)));
        assert!(matches!(detect_provider("https://open.bigmodel.cn/api/anthropic"), Some(PlanProvider::ZhipuCn)));
        assert!(matches!(detect_provider("https://bigmodel.cn/api/anthropic"), Some(PlanProvider::ZhipuCn)));
        assert!(matches!(detect_provider("https://api.z.ai/api/anthropic"), Some(PlanProvider::ZhipuEn)));
        assert!(matches!(detect_provider("https://api.minimaxi.com/v1"), Some(PlanProvider::MiniMaxCn)));
        assert!(matches!(detect_provider("https://api.minimax.io/v1"), Some(PlanProvider::MiniMaxEn)));
        assert!(matches!(detect_provider("https://zenmux.ai/api/example"), Some(PlanProvider::ZenMux)));
        assert!(matches!(detect_provider("https://opencode.ai/zen/go"), Some(PlanProvider::OpencodeGo)));
        assert!(matches!(detect_provider("https://opencode.ai/zen/go/v1"), Some(PlanProvider::OpencodeGo)));
        assert!(matches!(detect_provider("https://ark.cn-beijing.volces.com/api/plan/v3"), Some(PlanProvider::Volcengine)));
        assert!(matches!(detect_provider("https://ark.cn-beijing.volces.com/api/coding/v3"), Some(PlanProvider::Volcengine)));
        // 大小写不敏感
        assert!(matches!(detect_provider("https://API.KIMI.COM/coding/v1"), Some(PlanProvider::Kimi)));
    }

    #[test]
    fn detect_provider_rejects_non_plan_gateways_and_official() {
        // 官方端点、sub2api 网关、Zen 按量版都不命中
        assert!(detect_provider("https://api.anthropic.com").is_none());
        assert!(detect_provider("https://gateway.example.com").is_none());
        assert!(detect_provider("https://opencode.ai/zen/v1").is_none());
        // DouBaoSeed 按量付费路径不命中
        assert!(detect_provider("https://ark.cn-beijing.volces.com/api/v3").is_none());
        assert!(detect_provider("").is_none());
    }

    #[test]
    fn zhipu_real_shape_credit_limit_two_windows() {
        // 2026-09-17 实测：type=CREDIT_LIMIT，unit=3/6 定窗口，percentage 为已用百分比
        let body: serde_json::Value = serde_json::from_str(r#"{
            "code":200,"msg":"Operation successful","success":true,
            "data":{"level":"pro","limits":[
                {"type":"CREDIT_LIMIT","unit":3,"number":5,"usage":12000,"currentValue":5710,"remaining":6289,"percentage":47,"nextResetTime":1789648624765},
                {"type":"CREDIT_LIMIT","unit":6,"number":1,"usage":60000,"currentValue":5710,"remaining":54289,"percentage":9,"nextResetTime":1790235145997}
            ]}}"#).unwrap();
        let (windows, level) = parse_zhipu_windows(&body.get("data").unwrap());
        assert_eq!(level.as_deref(), Some("pro"));
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].key, "5h");
        assert_eq!(windows[0].used_percent, Some(47.0));
        assert_eq!(windows[0].amount_text.as_deref(), Some("5710 / 12000 credits"));
        assert!(windows[0].resets_at.is_some());
        assert_eq!(windows[1].key, "7d");
        assert_eq!(windows[1].used_percent, Some(9.0));
    }

    #[test]
    fn zhipu_unit_field_anchors_window_not_reset_order() {
        // issue #3036：周期末尾周桶比 5h 桶更早重置，按时间排序必然标反；unit 优先
        let data = serde_json::json!({"limits":[
            {"type":"TOKENS_LIMIT","unit":6,"number":7,"percentage":42.0,"nextResetTime":1_000_003_600_000i64},
            {"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":1.0,"nextResetTime":1_000_018_000_000i64}
        ]});
        let (windows, _) = parse_zhipu_windows(&data);
        assert_eq!(windows[0].key, "5h");
        assert_eq!(windows[0].used_percent, Some(1.0));
        assert_eq!(windows[1].key, "7d");
        assert_eq!(windows[1].used_percent, Some(42.0));
    }

    #[test]
    fn zhipu_missing_unit_falls_back_to_heuristics() {
        // unit 缺失：无 reset 的优先归 5h（5h 桶 0% 时可能没有 nextResetTime），其余按 reset 升序
        let data = serde_json::json!({"limits":[
            {"type":"TOKENS_LIMIT","percentage":25.0,"nextResetTime":2_000_000_000_000i64},
            {"type":"TOKENS_LIMIT","percentage":0.0}
        ]});
        let (windows, _) = parse_zhipu_windows(&data);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].key, "5h");
        assert_eq!(windows[0].used_percent, Some(0.0));
        assert!(windows[0].resets_at.is_none());
        assert_eq!(windows[1].key, "7d");
    }

    #[test]
    fn zhipu_old_plan_single_entry_and_type_case_insensitive() {
        // 老套餐只回 1 条；type 大小写不敏感；TIME_LIMIT 等其它类型跳过
        let data = serde_json::json!({"limits":[
            {"type":"tokens_limit","percentage":2.0,"nextResetTime":1_774_967_594_803i64},
            {"type":"TIME_LIMIT","percentage":7.0}
        ]});
        let (windows, _) = parse_zhipu_windows(&data);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].key, "5h");
        assert_eq!(windows[0].used_percent, Some(2.0));
    }

    #[test]
    fn zhipu_out_of_range_percentage_is_unknown_not_clamped() {
        // 岛内纪律：越界/非数 → None（不画条），区别于 cc-switch 的透传
        let data = serde_json::json!({"limits":[
            {"type":"TOKENS_LIMIT","unit":3,"percentage":150.0,"nextResetTime":1_000_000_000_000i64}
        ]});
        let (windows, _) = parse_zhipu_windows(&data);
        assert_eq!(windows[0].used_percent, None);
    }

    #[test]
    fn zhipu_business_error_maps_to_failed() {
        let state = zhipu_state_from_body(&serde_json::json!({"success":false,"msg":"invalid key"}));
        assert!(matches!(state, QuotaState::Failed { .. }));
    }

    #[test]
    fn zhipu_team_partial_config_fails_instead_of_personal_query() {
        // 团队版与个人版同 base_url：两项都缺 → 个人版查询；只配一项 → Failed 引导补全，
        // 不能拿半份团队配置静默查个人版，也不能缺配置就报错挡住个人版。
        let partial = PlanExtras { team_organization_id: Some("org-1"), ..Default::default() };
        let state = coding_plan_quota("https://open.bigmodel.cn/api/anthropic", "k", &partial);
        assert!(matches!(state, QuotaState::Failed { .. }), "{state:?}");

        let partial = PlanExtras { team_project_id: Some("p-1"), ..Default::default() };
        let state = coding_plan_quota("https://open.bigmodel.cn/api/anthropic", "k", &partial);
        assert!(matches!(state, QuotaState::Failed { .. }), "{state:?}");
    }

    #[test]
    fn zhipu_team_on_international_station_is_unsupported() {
        // 团队版仅国内站；国际站带全套团队配置视为配置错误
        let extras = PlanExtras {
            team_organization_id: Some("org-1"),
            team_project_id: Some("p-1"),
            ..Default::default()
        };
        let state = coding_plan_quota("https://api.z.ai/api/anthropic", "k", &extras);
        assert!(matches!(state, QuotaState::Unsupported { .. }), "{state:?}");
    }

    #[test]
    fn kimi_limits_detail_and_weekly_usage() {
        let body = serde_json::json!({
            "limits": [{"detail": {"limit": 300, "remaining": 100, "resetTime": "2026-09-17T12:00:00Z"}}],
            "usage": {"limit": 1000, "remaining": 250, "resetTime": 1_800_000_000}
        });
        let windows = parse_kimi_windows(&body);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].key, "5h");
        assert_eq!(windows[0].used_percent, Some(200.0 / 300.0 * 100.0));
        assert!(windows[0].resets_at.as_deref().unwrap().starts_with("2026-09-17"));
        assert_eq!(windows[1].key, "7d");
        assert_eq!(windows[1].used_percent, Some(75.0));
        assert!(windows[1].resets_at.is_some()); // 秒级时间戳自动升为毫秒
    }

    #[test]
    fn kimi_zero_limit_counts_zero_and_missing_usage_skipped() {
        let windows = parse_kimi_windows(&serde_json::json!({
            "limits": [{"detail": {"limit": 0, "remaining": 0}}]
        }));
        assert_eq!(windows.len(), 1); // 顶层 usage 缺失 → 无周桶
        assert_eq!(windows[0].used_percent, Some(0.0)); // limit=0 时按 0 处理
        let empty = parse_kimi_windows(&serde_json::json!({}));
        assert!(empty.is_empty());
    }

    #[test]
    fn minimax_general_only_and_remaining_inverted() {
        // 只取 model_name=general（跳过 video）；剩余百分比反转为已用；周桶仅 status=1 展示
        let body = serde_json::json!({
            "model_remains": [
                {"model_name": "video", "current_interval_remaining_percent": 10.0},
                {"model_name": "general",
                 "current_interval_remaining_percent": 80.0, "end_time": 1_800_000_000_000i64,
                 "current_weekly_status": 1, "current_weekly_remaining_percent": 90.0,
                 "weekly_end_time": 1_805_000_000_000i64}
            ]
        });
        let windows = parse_minimax_windows(&body);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].key, "5h");
        assert_eq!(windows[0].used_percent, Some(20.0));
        assert!(windows[0].resets_at.is_some());
        assert_eq!(windows[1].key, "7d");
        assert_eq!(windows[1].used_percent, Some(10.0));
    }

    #[test]
    fn minimax_weekly_status_three_means_no_weekly_limit() {
        // status=3：该套餐无周限额（remaining 恒 100），不展示，避免常绿假象
        let body = serde_json::json!({"model_remains":[{"model_name":"general",
            "current_interval_remaining_percent":50.0,
            "current_weekly_status":3, "current_weekly_remaining_percent":100.0}]});
        let windows = parse_minimax_windows(&body);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].key, "5h");
    }

    #[test]
    fn zenmux_windows_with_usd_amounts_and_plan() {
        let data = serde_json::json!({
            "quota_5_hour": {"usage_percentage": 0.42, "resets_at": "2026-09-17T12:00:00Z", "used_value_usd": 4.2, "max_value_usd": 10.0},
            "quota_7_day": {"usage_percentage": 0.08, "resets_at": "2026-09-22T00:00:00Z"},
            "plan": {"tier": "pro"}, "account_status": "active"
        });
        let (windows, plan) = parse_zenmux_windows(&data);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].key, "5h");
        assert_eq!(windows[0].used_percent, Some(42.0)); // 0-1 小数 ×100
        assert_eq!(windows[0].amount_text.as_deref(), Some("$4.20 / $10.00"));
        assert_eq!(windows[1].amount_text, None);
        assert_eq!(plan.as_deref(), Some("pro (active)"));
    }

    #[test]
    fn zenmux_missing_windows_yield_empty() {
        let (windows, plan) = parse_zenmux_windows(&serde_json::json!({"plan":{"tier":"pro"}}));
        assert!(windows.is_empty());
        assert_eq!(plan.as_deref(), Some("pro")); // account_status 缺失时只显示档位，不渲染空括号
    }

    #[test]
    fn opencode_go_three_windows_and_rate_limited_pinned_at_100() {
        // percent=0 时上游 resetsAt 是「now+窗口时长」占位值，丢弃不展示倒计时；
        // status=rate-limited 时上游已把 percent 钉在 100，无需特判
        let body = serde_json::json!({"usage":{
            "rolling": {"status":"ok","percent":37,"resetsAt":"2026-09-17T12:00:00Z"},
            "weekly":  {"status":"ok","percent":10,"resetsAt":"2026-09-22T00:00:00Z"},
            "monthly": {"status":"rate-limited","percent":100,"resetsAt":"2026-10-01T00:00:00Z"}
        }});
        let windows = parse_opencode_windows(&body);
        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0].key, "5h");
        assert_eq!(windows[0].used_percent, Some(37.0));
        assert!(windows[0].resets_at.is_some());
        assert_eq!(windows[2].key, "30d");
        assert_eq!(windows[2].used_percent, Some(100.0));
    }

    #[test]
    fn opencode_go_zero_percent_drops_reset_placeholder() {
        let body = serde_json::json!({"usage":{
            "rolling": {"status":"ok","percent":0,"resetsAt":"2026-09-17T12:00:00Z"}
        }});
        let windows = parse_opencode_windows(&body);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].used_percent, Some(0.0));
        assert!(windows[0].resets_at.is_none()); // 占位重置时间不展示
    }

    #[test]
    fn opencode_go_legacy_flat_shape_is_unrecognized() {
        // 2026-08-11 上线当天即作废的旧扁平形态：整卡不识别
        let windows = parse_opencode_windows(&serde_json::json!({
            "rollingUsage": {"usagePercent": 37, "resetInSec": 3600}
        }));
        assert!(windows.is_empty());
    }

    #[test]
    fn volcengine_region_extracted_from_data_plane_host() {
        assert_eq!(volcengine_region("https://ark.cn-beijing.volces.com/api/plan/v3"), "cn-beijing");
        assert_eq!(volcengine_region("https://ark.ap-southeast.bytepluses.com/api/coding"), "ap-southeast");
        assert_eq!(volcengine_region("https://example.com"), "cn-beijing"); // 回落默认
    }

    #[test]
    fn volcengine_canonical_query_is_sorted_and_encoded() {
        assert_eq!(volcengine_canonical_query("GetAFPUsage", "cn-beijing"),
            "Action=GetAFPUsage&Region=cn-beijing&Version=2024-01-01");
    }

    #[test]
    fn volcengine_uri_encode_follows_rfc3986_unreserved() {
        assert_eq!(volc_uri_encode("aB9-_.~"), "aB9-_.~");
        assert_eq!(volc_uri_encode("a b"), "a%20b");
        assert_eq!(volc_uri_encode("中"), "%E4%B8%AD");
    }

    #[test]
    fn volcengine_sign_is_deterministic_and_volc_shaped() {
        // 火山变体与标准 SigV4 的两处致命差异必须锁死：
        // 1) algorithm 无 AWS4 前缀；2) SignedHeaders 固定顺序 host;x-date;x-content-sha256;content-type
        let now = chrono::DateTime::parse_from_rfc3339("2026-09-17T00:00:00Z").unwrap().with_timezone(&chrono::Utc);
        let query = volcengine_canonical_query("GetAFPUsage", "cn-beijing");
        let (auth, x_date, sha) = volcengine_sign("AKTEST", "SKTEST", "cn-beijing", &query, b"", now);
        assert_eq!(x_date, "20260917T000000Z");
        assert_eq!(sha, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"); // 空 body SHA-256
        assert!(auth.starts_with("HMAC-SHA256 Credential=AKTEST/20260917/cn-beijing/ark/request,"));
        assert!(auth.contains("SignedHeaders=host;x-date;x-content-sha256;content-type,"));
        // 同输入同输出（确定性）
        let again = volcengine_sign("AKTEST", "SKTEST", "cn-beijing", &query, b"", now);
        assert_eq!(auth, again.0);
    }

    #[test]
    fn volcengine_afp_windows_skip_empty_quota_and_daily() {
        // AFPDaily 官方控制台也隐藏（历史默认值非强制限额），跳过；Quota<=0 视为未订阅
        let result = serde_json::json!({
            "PlanType": "Pro",
            "AFPFiveHour": {"Quota": 100.0, "Used": 25.0, "ResetTime": 1_800_000_000},
            "AFPWeekly":  {"Quota": 700.0, "Used": 70.0},
            "AFPMonthly": {"Quota": 0.0,   "Used": 0.0},
            "AFPDaily":   {"Quota": 999.0, "Used": 1.0}
        });
        let windows = parse_afp_windows(&result);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].key, "5h");
        assert_eq!(windows[0].used_percent, Some(25.0));
        assert!(windows[0].resets_at.is_some()); // 秒级自动升毫秒
        assert_eq!(windows[1].key, "7d");
        assert_eq!(windows[1].used_percent, Some(10.0));
    }

    #[test]
    fn volcengine_coding_plan_level_labels_map_to_windows() {
        // 实测 2026-06-21 字段为 Level: session/weekly/monthly，只给已用百分比
        let result = serde_json::json!({"QuotaUsage":[
            {"Level":"session","Percent":12.0,"ResetTime":1_800_000_000},
            {"Level":"weekly", "Percent":34.0,"ResetTime":1_805_000_000},
            {"Level":"monthly","Percent":56.0,"ResetTime":1_810_000_000},
            {"Level":"unknown","Percent":99.0}
        ]});
        let windows = parse_coding_plan_windows(&result);
        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0].key, "5h");
        assert_eq!(windows[1].key, "7d");
        assert_eq!(windows[2].key, "30d");
    }

    #[test]
    fn volcengine_auth_error_codes_are_recognized() {
        assert!(volcengine_is_auth_error_code("InvalidAuthorization"));
        assert!(volcengine_is_auth_error_code("SignatureDoesNotMatch"));
        assert!(volcengine_is_auth_error_code("AccessDenied"));
        assert!(!volcengine_is_auth_error_code("InternalError"));
        assert!(!volcengine_is_auth_error_code("QuotaExceeded"));
    }

    #[test]
    #[ignore = "需要本机智谱 Key 与网络；仅显式执行"]
    fn local_zhipu_read_only_smoke() {
        let key = crate::creds::read_secret("claude", "api").expect("缺少本机 Key");
        let state = query_zhipu("https://open.bigmodel.cn/api/anthropic", &key, None);
        assert!(matches!(state, QuotaState::Ok { .. }), "{state:?}");
        if let QuotaState::Ok { windows, plan } = state {
            for w in windows {
                println!("{} used={:?} amount={:?} has_reset={}", w.key, w.used_percent, w.amount_text, w.resets_at.is_some());
            }
            println!("plan={plan:?}");
        }
    }
}
