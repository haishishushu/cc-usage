# 编程套餐额度查询（cc-switch 全量对齐）实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 cc-usage 中落地与 cc-switch 对齐的全部套餐额度查询：智谱 GLM（中国版/国际版/团队版）、Kimi For Coding、MiniMax（中/国际）、ZenMux、OpenCode Go、火山方舟（Agent/Coding Plan）、Grok（SuperGrok credits），在灵动岛与主面板展示 5 小时/周/月额度。

**Architecture:** 新增 Rust 模块 `backend/src/coding_plan.rs`（9 家套餐，按 base_url 域名识别路由）与 `backend/src/grok_quota.rs`（Grok，读本机 `~/.grok/auth.json` 凭证 + gRPC-Web）。两者都输出既有的 `quota::QuotaState`/`QuotaWindow`，前端 UI 层零适配自动渲染。路由挂点在 `lib.rs::connection_quota` 的 `api + base_url` 分支，`detect_provider` 命中套餐供应商则走新查询，否则维持 sub2api。智谱团队版与火山的辅助凭证（组织/项目 ID、AK/SK）存全局设置 `settings.rs`，不动连接模型。

**Tech Stack:** Tauri 2 + Rust（reqwest blocking + rustls、serde_json、rusqlite、chrono；新增 hmac 0.12 + sha2 0.10）；前端 React 19 + TS（本计划前端改动极小）。

**Spec:** cc-switch 源码（参考副本将随 Task 0 复制到 `docs/reference/cc-switch/`，原始仓库 https://github.com/farion1231/cc-switch ，关键文件 `src-tauri/src/services/coding_plan.rs` 与 `subscription_grok.rs`）。智谱个人版端点已于 2026-09-17 用本机 key 实测验证（HTTP 200，响应样例见 Task 2）。

## Global Constraints

- **Git 纪律：未经鼠鼠明确授权不得执行任何 `git add`/`commit`/`push`**。每个 Task 以「运行验证 + 报告结果」结束，提交统一等鼠鼠验收后另行授权。
- 全部查询**只读**：不发提示内容、不写第三方。火山 OpenAPI 与 Grok 计费是 POST，但均为查询语义（空 body / 空 gRPC 帧）。
- 查询失败**绝不渲染为 0% 或满格**：沿用 `quota::QuotaState` 五态（Ok/Unsupported/Forbidden/Unauthorized/RateLimited/Failed），401→Unauthorized、403→Forbidden、429→RateLimited，其余确定性失败→Failed。
- HTTP 客户端统一 15 秒超时 + 禁止重定向（复用 `quota.rs::client()`）。
- `used_percent` 一律 `.filter(|v| v.is_finite() && (0.0..=100.0).contains(v))`（岛内 §7.4 纪律；cc-switch 对越界值透传，**此处刻意不从**）。
- 鉴权头差异：智谱用 `Authorization: <裸key>`（**无 Bearer**）；Kimi/MiniMax/ZenMux/OpenCode/Grok 用 `Bearer`；火山用 AK/SK HMAC-SHA256 签名。
- 代码风格与注释密度对齐 `quota.rs`：中文注释、模块头写清端点与纪律；UTF-8。
- 解析函数必须是纯函数（无网络 IO），配 fixture 单元测试；真实网络查询配 `#[ignore]` 冒烟测试。
- 窗口命名映射（`QuotaWindow.key/window_name`）：5 小时→`("5h","5 小时额度")`、周→`("7d","周额度")`、月→`("30d","月额度")`、Grok credits→`("credits","Grok 积分额度")`。

---

### Task 0: 参考资料与依赖准备

**Files:**
- Create: `docs/reference/cc-switch/coding_plan.rs`（从克隆副本复制）
- Create: `docs/reference/cc-switch/subscription_grok.rs`（同上）
- Modify: `backend/Cargo.toml`（dependencies 段）

**Interfaces:**
- Produces: 后续任务的移植底稿位于 `docs/reference/cc-switch/`；`hmac`/`sha2` crate 可用。

- [ ] **Step 1: 复制 cc-switch 参考源码**

cc-switch 已克隆在 `%TEMP%\cc-switch-src`（若已被清理，先 `git clone --depth 1 https://github.com/farion1231/cc-switch` 到临时目录）：

```bash
mkdir -p docs/reference/cc-switch
cp "$TMPDIR/cc-switch-src/src-tauri/src/services/coding_plan.rs" docs/reference/cc-switch/
cp "$TMPDIR/cc-switch-src/src-tauri/src/services/subscription_grok.rs" docs/reference/cc-switch/
```

（Git Bash 下 `$TMPDIR` 即 `/tmp`；实际路径为 `C:\Users\<user>\AppData\Local\Temp\cc-switch-src`。）

- [ ] **Step 2: 添加依赖**

`backend/Cargo.toml` 的 `[dependencies]` 末尾（reqwest 条目之后）追加：

```toml
# 火山方舟控制面 OpenAPI 的 AK/SK 签名（火山变体 SigV4，见 coding_plan.rs）
hmac = "0.12"
sha2 = "0.10"
```

- [ ] **Step 3: 验证编译**

Run: `cargo build`（在 `backend/` 下，或仓库根 `pnpm tauri dev` 前的增量检查）
Expected: 编译通过，无新 warning。

---

### Task 1: `coding_plan.rs` 模块骨架 + 供应商识别

**Files:**
- Create: `backend/src/coding_plan.rs`
- Modify: `backend/src/lib.rs:21`（mod 声明区，紧挨 `mod quota;` 加 `mod coding_plan;`）
- Modify: `backend/src/quota.rs:357` 附近（`client()`、`HttpOutcome`、`read()` 改 `pub(crate)`）

**Interfaces:**
- Consumes: `quota::QuotaState`、`quota::QuotaWindow`（既有）；`quota::client()`/`quota::read()`（本任务改为 crate 可见）。
- Produces（后续所有任务依赖，签名逐字一致）:

```rust
pub(crate) enum PlanProvider { Kimi, ZhipuCn, ZhipuEn, MiniMaxCn, MiniMaxEn, ZenMux, OpencodeGo, Volcengine }
pub(crate) fn detect_provider(base_url: &str) -> Option<PlanProvider>
pub(crate) struct PlanExtras<'a> {
    pub team_organization_id: Option<&'a str>,
    pub team_project_id: Option<&'a str>,
    pub volc_access_key_id: Option<&'a str>,
    pub volc_secret_access_key: Option<&'a str>,
}
pub(crate) fn coding_plan_quota(base_url: &str, api_key: &str, extras: &PlanExtras) -> quota::QuotaState
// 窗口命名常量
const W5H: (&str, &str);   // ("5h", "5 小时额度")
const W7D: (&str, &str);   // ("7d", "周额度")
const W30D: (&str, &str);  // ("30d", "月额度")
const WCREDITS: (&str, &str); // ("credits", "Grok 积分额度")
```

- [ ] **Step 1: 写失败测试**（`backend/src/coding_plan.rs` 底部 `#[cfg(test)] mod tests`）

```rust
#[cfg(test)]
mod tests {
    use super::*;

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
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test coding_plan`
Expected: 编译失败（模块不存在）。

- [ ] **Step 3: 最小实现**

模块头注释照抄 `quota.rs` 风格（端点表 + 纪律），实现：

```rust
//! 编程套餐（Coding Plan）额度的只读查询（cc-switch 对齐，2026-09 源）
//!
//! | 供应商 | 端点 | 凭证 | 窗口 |
//! |---|---|---|---|
//! | Kimi For Coding | GET api.kimi.com/coding/v1/usages | Bearer | 5h + 周 |
//! | 智谱 GLM 中国版 | GET open.bigmodel.cn/api/monitor/usage/quota/limit | 裸 key | 5h(unit=3) + 周(unit=6) |
//! | 智谱 GLM 国际版 | GET api.z.ai/api/monitor/usage/quota/limit | 裸 key | 同上 |
//! | 智谱团队版 | 同中国版 + ?type=2 + 组织/项目头 | 裸 key + 组织ID + 项目ID | 同上 |
//! | MiniMax 中/国际 | GET {api.minimaxi.com|api.minimax.io}/v1/api/openplatform/coding_plan/remains | Bearer | 5h + 周(激活时) |
//! | ZenMux | GET {base_url}（base 即用量端点） | Bearer | 5h + 7d（带美元） |
//! | OpenCode Go | GET opencode.ai/zen/go/v1/usage | Bearer | rolling(5h) + 周 + 月 |
//! | 火山方舟 | POST open.volcengineapi.com OpenAPI | AK/SK 签名 | session(5h) + 周 + 月 |
//!
//! 纪律与 [`crate::quota`] 一致：只读、15 秒超时、禁止重定向、五态区分。

use crate::quota::{self, QuotaState, QuotaWindow};

const W5H: (&str, &str) = ("5h", "5 小时额度");
const W7D: (&str, &str) = ("7d", "周额度");
const W30D: (&str, &str) = ("30d", "月额度");
const WCREDITS: (&str, &str) = ("credits", "Grok 积分额度");

pub(crate) enum PlanProvider { Kimi, ZhipuCn, ZhipuEn, MiniMaxCn, MiniMaxEn, ZenMux, OpencodeGo, Volcengine }

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
}

/// 统一入口：detect_provider 命中后由 lib.rs / validate_connection 调用。
pub(crate) fn coding_plan_quota(base_url: &str, api_key: &str, extras: &PlanExtras) -> QuotaState {
    let _ = (base_url, api_key, extras); // 后续任务逐家替换
    QuotaState::Unsupported { reason: "套餐查询尚未接入".into() }
}
```

同时把 `quota.rs` 中 `fn client()`、`enum HttpOutcome`、`fn read()` 加 `pub(crate)`（仅改可见性，不改逻辑），并在 `lib.rs:21` 旁加 `mod coding_plan;`。

- [ ] **Step 4: 运行确认通过**

Run: `cargo test coding_plan`
Expected: 2 个测试 PASS；`cargo build` 无警告。

---

### Task 2: 智谱 GLM 个人版（中国/国际）

**Files:**
- Modify: `backend/src/coding_plan.rs`

**Interfaces:**
- Consumes: Task 1 的全部签名；`quota::client()/read()/HttpOutcome`。
- Produces:

```rust
fn zhipu_quota_base(base_url: &str) -> &'static str        // bigmodel.cn→open.bigmodel.cn，否则 api.z.ai
fn parse_zhipu_windows(data: &serde_json::Value) -> (Vec<QuotaWindow>, Option<String>) // (窗口, level)
fn query_zhipu(base: &str, api_key: &str, team: Option<(&str, &str)>) -> QuotaState // Task 3 复用
```

- [ ] **Step 1: 写失败测试**（加入 `mod tests`；第一个 fixture 来自 2026-09-17 实测响应）

```rust
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

    // 允许直接测内部入口；zhipu_state_from_body = query 的解析半区（见实现）
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test zhipu`
Expected: 编译失败（`parse_zhipu_windows` 不存在）。

- [ ] **Step 3: 实现**

```rust
// ── 智谱 GLM ────────────────────────────────────────────────

/// 控制台同源监控端点（非公开文档 API）。中国/国际站同路径同 JSON 形态。
const ZHIPU_QUOTA_PATH: &str = "/api/monitor/usage/quota/limit";

fn zhipu_quota_base(base_url: &str) -> &'static str {
    if base_url.to_lowercase().contains("bigmodel.cn") { "https://open.bigmodel.cn" } else { "https://api.z.ai" }
}

fn millis_to_iso(ms: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(ms / 1000, ((ms % 1000) * 1_000_000) as u32).map(|t| t.to_rfc3339())
}

/// 智谱条目按 `unit` 显式分类：3=5 小时滚动窗，6=周窗（number 有 5/7/1 多种实测，
/// 只锚定 unit）。缺失或不识别时走兜底：无 reset 优先归 5h，其余按 reset 升序补位。
fn parse_zhipu_windows(data: &serde_json::Value) -> (Vec<QuotaWindow>, Option<String>) {
    enum Window { FiveHour, Weekly }
    let classify = |item: &serde_json::Value| match item.get("unit").and_then(|v| v.as_i64()) {
        Some(3) => Some(Window::FiveHour),
        Some(6) => Some(Window::Weekly),
        _ => None,
    };
    type Entry = (Option<i64>, Option<f64>, Option<String>); // (reset_ms, percent, reset_iso)
    let mut five_hour: Option<Entry> = None;
    let mut weekly: Option<Entry> = None;
    let mut unclassified: Vec<Entry> = Vec::new();

    if let Some(limits) = data.get("limits").and_then(|v| v.as_array()) {
        for item in limits {
            let kind = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if !(kind.eq_ignore_ascii_case("TOKENS_LIMIT") || kind.eq_ignore_ascii_case("CREDIT_LIMIT")) {
                continue;
            }
            let reset_ms = item.get("nextResetTime").and_then(|v| v.as_i64());
            let percent = item.get("percentage").and_then(|v| v.as_f64)
                .filter(|v| v.is_finite() && (0.0..=100.0).contains(v));
            // credits 用量（智谱新接口）：currentValue 已用 / usage 总量
            let amount = match (
                item.get("currentValue").and_then(|v| v.as_f64),
                item.get("usage").and_then(|v| v.as_f64),
            ) {
                (Some(used), Some(total)) if total > 0.0 && used.is_finite() => {
                    Some(format!("{used:.0} / {total:.0} credits"))
                }
                _ => None,
            };
            let entry = (reset_ms, percent, reset_ms.and_then(millis_to_iso));
            let entry_with_amount = (entry.0, entry.1, entry.2, amount);
            match classify(item) {
                Some(Window::FiveHour) if five_hour.is_none() => five_hour = Some(entry_with_amount),
                Some(Window::Weekly) if weekly.is_none() => weekly = Some(entry_with_amount),
                _ => unclassified.push(entry_with_amount),
            }
        }
    }
    unclassified.sort_by_key(|(reset, ..)| (reset.is_some(), reset.unwrap_or(i64::MIN)));
    for entry in unclassified {
        if five_hour.is_none() { five_hour = Some(entry); }
        else if weekly.is_none() { weekly = Some(entry); }
    }

    let mut windows = Vec::new();
    for (key, name, slot) in [(W5H.0, W5H.1, five_hour), (W7D.0, W7D.1, weekly)] {
        if let Some((_, percent, reset, amount)) = slot {
            windows.push(QuotaWindow {
                key: key.into(), window_name: name.into(),
                used_percent: percent, amount_text: amount, resets_at: reset,
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

/// team 为 Some((org, project)) 时走团队版（?type=2 + 组织/项目请求头）。
fn query_zhipu(base: &str, api_key: &str, team: Option<(&str, &str)>) -> QuotaState {
    let c = match quota::client() { Ok(c) => c, Err(e) => return QuotaState::Failed { reason: e } };
    let mut url = format!("https://open.bigmodel.cn{ZHIPU_QUOTA_PATH}");
    if base.to_lowercase().contains("z.ai") { url = format!("https://api.z.ai{ZHIPU_QUOTA_PATH}"); }
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
```

注意：URL 构造以 `zhipu_quota_base` 为准（上面展开写法替换为调用它）。真实查询加冒烟测试：

```rust
    #[test]
    #[ignore = "需要本机智谱 Key 与网络；仅显式执行"]
    fn local_zhipu_read_only_smoke() {
        let key = crate::creds::read_secret("claude", "api").expect("缺少本机 Key");
        let state = query_zhipu("https://open.bigmodel.cn/api/anthropic", &key, None);
        assert!(matches!(state, QuotaState::Ok { .. }), "{state:?}");
    }
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test zhipu`
Expected: 6 个单测 PASS。

---

### Task 3: 智谱团队版路由

**Files:**
- Modify: `backend/src/coding_plan.rs`（`coding_plan_quota` 分发）

**Interfaces:**
- Consumes: `query_zhipu(base, key, team)`（Task 2）、`PlanExtras::team_ids()`（Task 1）。
- Produces: `coding_plan_quota` 对 `ZhipuCn/ZhipuEn` 的完整行为。

- [ ] **Step 1: 写失败测试**

```rust
    #[test]
    fn zhipu_team_requires_org_and_project_ids() {
        // base 命中智谱但缺组织/项目 ID → Failed 引导，而不是发个人版请求把团队额度显示成个人
        let extras = PlanExtras::default();
        let state = coding_plan_quota("https://open.bigmodel.cn/api/anthropic", "k", &extras);
        assert!(matches!(state, QuotaState::Failed { .. }));
    }
```

- [ ] **Step 2: 运行确认失败**（当前桩返回 Unsupported，不匹配 Failed）

Run: `cargo test zhipu_team_requires`

- [ ] **Step 3: 实现 `coding_plan_quota` 的智谱分支**

```rust
pub(crate) fn coding_plan_quota(base_url: &str, api_key: &str, extras: &PlanExtras) -> QuotaState {
    match detect_provider(base_url) {
        Some(PlanProvider::ZhipuCn) | Some(PlanProvider::ZhipuEn) => match extras.team_ids() {
            // 团队版仅国内站；国际站带组织 ID 视为配置错误
            Some((org, project)) if base_url.to_lowercase().contains("z.ai") => QuotaState::Unsupported {
                reason: "智谱团队版仅存在于国内站（open.bigmodel.cn）".into(),
            },
            Some(ids) => query_zhipu(base_url, api_key, Some(ids)),
            None => query_zhipu(base_url, api_key, None),
        },
        _ => QuotaState::Unsupported { reason: "套餐查询尚未接入".into() }, // 后续任务补齐
    }
}
```

- [ ] **Step 4: 运行确认通过**（`cargo test coding_plan` 全绿）

---

### Task 4: Kimi For Coding

**Files:**
- Modify: `backend/src/coding_plan.rs`

**Interfaces:**
- Produces:

```rust
fn parse_kimi_windows(body: &serde_json::Value) -> Vec<QuotaWindow>
fn query_kimi(api_key: &str) -> QuotaState
```

- [ ] **Step 1: 写失败测试**

```rust
    #[test]
    fn kimi_limits_detail_and_weekly_usage() {
        let body = serde_json::json!({
            "limits": [{"detail": {"limit": 300, "remaining": 100, "resetTime": "2026-09-17T12:00:00Z"}}],
            "usage": {"limit": 1000, "remaining": 250, "resetTime": 1_800_000_000}
        });
        let windows = parse_kimi_windows(&body);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].key, "5h");
        assert_eq!(windows[0].used_percent, Some(66.66666666666667));
        assert!(windows[0].resets_at.as_deref().unwrap().starts_with("2026-09-17"));
        assert_eq!(windows[1].key, "7d");
        assert_eq!(windows[1].used_percent, Some(75.0));
        assert!(windows[1].resets_at.is_some()); // 秒级时间戳自动升为毫秒
    }

    #[test]
    fn kimi_zero_limit_and_missing_usage() {
        let windows = parse_kimi_windows(&serde_json::json!({
            "limits": [{"detail": {"limit": 0, "remaining": 0}}]
        }));
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].used_percent, Some(0.0)); // limit=0 时按 0 处理（cc-switch 同款）
        let empty = parse_kimi_windows(&serde_json::json!({}));
        assert!(empty.is_empty());
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test kimi`

- [ ] **Step 3: 实现**

```rust
// ── Kimi For Coding ─────────────────────────────────────────

/// `limits[].detail` 为 5h 桶（limit/remaining 绝对值），顶层 `usage` 为周桶；
/// resetTime 兼容 ISO 字符串与秒/毫秒时间戳。
fn parse_kimi_windows(body: &serde_json::Value) -> Vec<QuotaWindow> {
    let tier = |limit: f64, remaining: f64| {
        let used = (limit - remaining).max(0.0);
        if limit > 0.0 { Some(used / limit * 100.0) } else { Some(0.0) }
            .filter(|v| v.is_finite() && (0.0..=100.0).contains(v))
    };
    let mut windows = Vec::new();
    if let Some(limits) = body.get("limits").and_then(|v| v.as_array()) {
        for item in limits {
            let Some(detail) = item.get("detail") else { continue };
            windows.push(QuotaWindow {
                key: W5H.0.into(), window_name: W5H.1.into(),
                used_percent: tier(
                    detail.get("limit").and_then(|v| v.as_f64).unwrap_or(1.0),
                    detail.get("remaining").and_then(|v| v.as_f64).unwrap_or(0.0),
                ),
                amount_text: None,
                resets_at: detail.get("resetTime").and_then(extract_reset_time),
            });
        }
    }
    if let Some(usage) = body.get("usage") {
        windows.push(QuotaWindow {
            key: W7D.0.into(), window_name: W7D.1.into(),
            used_percent: tier(
                usage.get("limit").and_then(|v| v.as_f64).unwrap_or(1.0),
                usage.get("remaining").and_then(|v| v.as_f64).unwrap_or(0.0),
            ),
            amount_text: None,
            resets_at: usage.get("resetTime").and_then(extract_reset_time),
        });
    }
    windows
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
```

并在 `coding_plan_quota` 加分支：`Some(PlanProvider::Kimi) => query_kimi(api_key),`

- [ ] **Step 4: 运行确认通过**（`cargo test kimi` 全绿）

---

### Task 5: MiniMax（中国/国际）

**Files:**
- Modify: `backend/src/coding_plan.rs`

**Interfaces:**
- Produces:

```rust
fn parse_minimax_windows(body: &serde_json::Value) -> Vec<QuotaWindow>
fn query_minimax(api_key: &str, cn: bool) -> QuotaState
```

- [ ] **Step 1: 写失败测试**

```rust
    #[test]
    fn minimax_general_only_and_remaining_inverted() {
        // 只取 model_name=general（跳过 video）；剩余百分比反转为已用；周桶仅 status=1 展示
        let body = serde_json::json!({
            "base_resp": {"status_code": 0},
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
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test minimax`

- [ ] **Step 3: 实现**

```rust
// ── MiniMax ─────────────────────────────────────────────────

/// 编程套餐剩余百分比在 `model_remains` 的 `general` 条目；`current_*_remaining_percent`
/// 是「剩余」，反转为已用。周桶仅 `current_weekly_status==1` 时存在（3=无周限额）。
fn parse_minimax_windows(body: &serde_json::Value) -> Vec<QuotaWindow> {
    let mut windows = Vec::new();
    let Some(items) = body.get("model_remains").and_then(|v| v.as_array()) else { return windows };
    let Some(item) = items.iter().find(|i| i.get("model_name").and_then(|v| v.as_str()) == Some("general")) else { return windows };
    let used_of = |remain: f64| (100.0 - remain).clamp(0.0, 100.0);
    if let Some(remain) = item.get("current_interval_remaining_percent").and_then(|v| v.as_f64) {
        windows.push(QuotaWindow {
            key: W5H.0.into(), window_name: W5H.1.into(),
            used_percent: Some(used_of(remain)),
            amount_text: None,
            resets_at: item.get("end_time").and_then(|v| v.as_i64).and_then(millis_to_iso),
        });
    }
    if item.get("current_weekly_status").and_then(|v| v.as_i64) == Some(1) {
        if let Some(remain) = item.get("current_weekly_remaining_percent").and_then(|v| v.as_f64) {
            windows.push(QuotaWindow {
                key: W7D.0.into(), window_name: W7D.1.into(),
                used_percent: Some(used_of(remain)),
                amount_text: None,
                resets_at: item.get("weekly_end_time").and_then(|v| v.as_i64).and_then(millis_to_iso),
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
                        let code = base_resp.get("status_code").and_then(|v| v.as_i64).unwrap_or(-1);
                        if code != 0 {
                            let msg = base_resp.get("status_msg").and_then(|v| v.as_str).unwrap_or("Unknown error");
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
```

并在 `coding_plan_quota` 加：`Some(PlanProvider::MiniMaxCn) => query_minimax(api_key, true), Some(PlanProvider::MiniMaxEn) => query_minimax(api_key, false),`

- [ ] **Step 4: 运行确认通过**（`cargo test minimax` 全绿）

---

### Task 6: ZenMux

**Files:**
- Modify: `backend/src/coding_plan.rs`

**Interfaces:**
- Produces:

```rust
fn parse_zenmux_windows(data: &serde_json::Value) -> (Vec<QuotaWindow>, Option<String>)
fn query_zenmux(base_url: &str, api_key: &str) -> QuotaState
```

- [ ] **Step 1: 写失败测试**

```rust
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
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test zenmux`

- [ ] **Step 3: 实现**

```rust
// ── ZenMux ──────────────────────────────────────────────────

/// base_url 本身就是用量端点；usage_percentage 为 0–1 小数；5h 桶带美元已用/上限。
fn parse_zenmux_windows(data: &serde_json::Value) -> (Vec<QuotaWindow>, Option<String>) {
    let mut windows = Vec::new();
    let tier = |key: (&str, &str), node: Option<&serde_json::Value>| {
        let Some(q) = node else { return None };
        Some(QuotaWindow {
            key: key.0.into(), window_name: key.1.into(),
            used_percent: q.get("usage_percentage").and_then(|v| v.as_f64)
                .map(|p| p * 100.0)
                .filter(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            amount_text: match (
                q.get("used_value_usd").and_then(|v| v.as_f64),
                q.get("max_value_usd").and_then(|v| v.as_f64),
            ) {
                (Some(used), Some(max)) if max > 0.0 => Some(format!("${used:.2} / ${max:.2}")),
                _ => None,
            },
            resets_at: q.get("resets_at").and_then(|v| v.as_str).map(str::to_string),
        })
    };
    windows.extend(tier(W5H, data.get("quota_5_hour")));
    windows.extend(tier(W7D, data.get("quota_7_day")));
    let plan = data.get("plan").and_then(|p| p.get("tier")).and_then(|v| v.as_str).and_then(|tier| {
        let status = data.get("account_status").and_then(|v| v.as_str).unwrap_or("");
        (!tier.is_empty()).then(|| format!("{tier} ({status})"))
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
                        let msg = v.get("message").and_then(|v| v.as_str).unwrap_or("Unknown error");
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
```

并在 `coding_plan_quota` 加：`Some(PlanProvider::ZenMux) => query_zenmux(base_url, api_key),`

- [ ] **Step 4: 运行确认通过**（`cargo test zenmux` 全绿）

---

### Task 7: OpenCode Go

**Files:**
- Modify: `backend/src/coding_plan.rs`

**Interfaces:**
- Produces:

```rust
fn parse_opencode_windows(body: &serde_json::Value) -> Vec<QuotaWindow>
fn query_opencode_go(api_key: &str) -> QuotaState
```

- [ ] **Step 1: 写失败测试**

```rust
    #[test]
    fn opencode_go_three_windows_and_zero_percent_drops_reset() {
        // percent=0 时上游 resetsAt 是「now+窗口时长」占位值，丢弃不展示倒计时
        let body = serde_json::json!({"usage":{
            "rolling": {"status":"ok","percent":37,"resetsAt":"2026-09-17T12:00:00Z"},
            "weekly":  {"status":"ok","percent":10,"resetsAt":"2026-09-22T00:00:00Z"},
            "monthly": {"status":"rate-limited","percent":100,"resetsAt":"2026-10-01T00:00:00Z"}
        }});
        let windows = parse_opencode_windows(&body);
        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0].key, "5h");
        assert_eq!(windows[0].used_percent, Some(37.0));
        assert_eq!(windows[2].key, "30d");
        assert_eq!(windows[2].used_percent, Some(100.0)); // rate-limited 时 percent 已钉 100
    }

    #[test]
    fn opencode_go_legacy_flat_shape_is_unrecognized() {
        // 2026-08-11 上线当天即作废的旧扁平形态：整卡不识别
        let windows = parse_opencode_windows(&serde_json::json!({
            "rollingUsage": {"usagePercent": 37, "resetInSec": 3600}
        }));
        assert!(windows.is_empty());
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test opencode`

- [ ] **Step 3: 实现**

```rust
// ── OpenCode Go ─────────────────────────────────────────────

/// 第一方但未文档化的路由（上线次日就改过一次形态），逐窗口防御解析：
/// 缺失或 percent 不可解析的窗口跳过；全空由调用方按「形态不认识」报错。
fn parse_opencode_windows(body: &serde_json::Value) -> Vec<QuotaWindow> {
    const WINDOWS: [(&str, &str, &str); 3] = [
        ("rolling", W5H.0, W5H.1), ("weekly", W7D.0, W7D.1), ("monthly", W30D.0, W30D.1),
    ];
    let Some(usage) = body.get("usage") else { return Vec::new() };
    let mut windows = Vec::new();
    for (node_key, key, name) in WINDOWS {
        let Some(w) = usage.get(node_key) else { continue };
        let Some(percent) = w.get("percent").and_then(|v| v.as_f64) else { continue };
        windows.push(QuotaWindow {
            key: key.into(), window_name: name.into(),
            used_percent: Some(percent).filter(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            amount_text: None,
            // percent=0 时 resetsAt 是占位值（窗口早已过期），不展示倒计时
            resets_at: if percent > 0.0 { w.get("resetsAt").and_then(extract_reset_time) } else { None },
        });
    }
    windows
}

/// 用量端点只认 `Authorization: Bearer`（与推理侧 x-api-key 正好相反，不能互换）。
fn query_opencode_go(api_key: &str) -> QuotaState {
    let c = match quota::client() { Ok(c) => c, Err(e) => return QuotaState::Failed { reason: e } };
    match c.get("https://opencode.ai/zen/go/v1/usage")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Accept", "application/json")
        .send()
    {
        Ok(r) => {
            let status = r.status();
            if status.as_u16() == 403 {
                return QuotaState::Forbidden {
                    reason: "Key 有效但该工作区未订阅 OpenCode Go（HTTP 403）".into(),
                };
            }
            match quota::read(r) {
                quota::HttpOutcome::Body(body) => match serde_json::from_str::<serde_json::Value>(&body) {
                    Ok(v) => {
                        let windows = parse_opencode_windows(&v);
                        if windows.is_empty() { QuotaState::Failed { reason: "OpenCode Go 用量响应形态不认识（上游曾变更过结构）".into() } }
                        else { QuotaState::Ok { windows, plan: None } }
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
```

并在 `coding_plan_quota` 加：`Some(PlanProvider::OpencodeGo) => query_opencode_go(api_key),`

- [ ] **Step 4: 运行确认通过**（`cargo test opencode` 全绿）

---

### Task 8: 火山方舟签名原语（AK/SK HMAC-SHA256）

**Files:**
- Modify: `backend/src/coding_plan.rs`

**Interfaces:**
- Produces（Task 9 依赖）:

```rust
fn volcengine_region(base_url: &str) -> String
fn volc_uri_encode(input: &str) -> String
fn volcengine_canonical_query(action: &str, region: &str) -> String
fn volcengine_sign(ak: &str, sk: &str, region: &str, canonical_query: &str, body: &[u8], now: chrono::DateTime<chrono::Utc>) -> (String, String, String) // (Authorization, X-Date, X-Content-Sha256)
```

- [ ] **Step 1: 写失败测试**（确定性签名测试，注入固定时间）

```rust
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
        // 同输入同输出（确定性），供回归对比
        let again = volcengine_sign("AKTEST", "SKTEST", "cn-beijing", &query, b"", now);
        assert_eq!(auth, again.0);
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test volcengine`

- [ ] **Step 3: 实现**（移植参考 `docs/reference/cc-switch/coding_plan.rs:844-1024`，算法注释照搬——两处与标准 SigV4 的差异是实测踩坑结论）

```rust
// ── 火山方舟 Agent Plan / Coding Plan ───────────────────────
//
// 控制面 OpenAPI（open.volcengineapi.com，非数据面 ark.cn-beijing.volces.com），
// 强制火山引擎签名 V4（AK/SK）——复用推理 Bearer Key 会被网关 400 InvalidAuthorization。
// 签名是 AWS SigV4 的火山变体，两处致命差异：
//   1. canonical headers 与 SignedHeaders 用固定顺序 host;x-date;x-content-sha256;content-type（不按字母序）
//   2. algorithm 串 HMAC-SHA256（无 AWS4 前缀）、scope 终止 request（非 aws4_request）、kDate=HMAC(SK, date)（SK 不加前缀）

const VOLCENGINE_OPENAPI_HOST: &str = "open.volcengineapi.com";
const VOLCENGINE_API_VERSION: &str = "2024-01-01";
const VOLCENGINE_DEFAULT_REGION: &str = "cn-beijing";
const VOLCENGINE_SERVICE: &str = "ark";
const VOLCENGINE_CONTENT_TYPE: &str = "application/json; charset=utf-8";
const VOLCENGINE_SIGNED_HEADERS: &str = "host;x-date;x-content-sha256;content-type";

fn volcengine_region(base_url: &str) -> String {
    let host = base_url.split_once("://").map(|(_, rest)| rest).unwrap_or(base_url)
        .split('/').next().unwrap_or("");
    host.split('.').find(|p| p.starts_with("cn-") || p.starts_with("ap-"))
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
            _ => { use std::fmt::Write; let _ = write!(out, "%{byte:02X}"); }
        }
    }
    out
}

/// 按 key 字母序排序、逐段 URL 编码；同一份字符串既用于签名也用于实际 URL。
fn volcengine_canonical_query(action: &str, region: &str) -> String {
    let mut pairs = [("Action", action), ("Region", region), ("Version", VOLCENGINE_API_VERSION)];
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    pairs.iter().map(|(k, v)| format!("{}={}", volc_uri_encode(k), volc_uri_encode(v))).collect::<Vec<_>>().join("&")
}

/// 生成 (Authorization, X-Date, X-Content-Sha256)，三者都必须随请求发送。
fn volcengine_sign(ak: &str, sk: &str, region: &str, canonical_query: &str, body: &[u8], now: chrono::DateTime<chrono::Utc>) -> (String, String, String) {
    let x_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let short_date = now.format("%Y%m%d").to_string();
    let x_content_sha256 = volc_sha256_hex(body);
    let canonical_headers = format!(
        "host:{VOLCENGINE_OPENAPI_HOST}\nx-date:{x_date}\nx-content-sha256:{x_content_sha256}\ncontent-type:{VOLCENGINE_CONTENT_TYPE}\n"
    );
    let canonical_request = format!("POST\n/\n{canonical_query}\n{canonical_headers}\n{VOLCENGINE_SIGNED_HEADERS}\n{x_content_sha256}");
    let credential_scope = format!("{short_date}/{region}/{VOLCENGINE_SERVICE}/request");
    let string_to_sign = format!("HMAC-SHA256\n{x_date}\n{credential_scope}\n{}", volc_sha256_hex(canonical_request.as_bytes()));
    let k_date = volc_hmac_sha256(sk.as_bytes(), short_date.as_bytes());
    let k_region = volc_hmac_sha256(&k_date, region.as_bytes());
    let k_service = volc_hmac_sha256(&k_region, VOLCENGINE_SERVICE.as_bytes());
    let k_signing = volc_hmac_sha256(&k_service, b"request");
    let signature: String = volc_hmac_sha256(&k_signing, string_to_sign.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    (format!("HMAC-SHA256 Credential={ak}/{credential_scope}, SignedHeaders={VOLCENGINE_SIGNED_HEADERS}, Signature={signature}"), x_date, x_content_sha256)
}
```

- [ ] **Step 4: 运行确认通过**（`cargo test volcengine` 全绿）

---

### Task 9: 火山方舟 OpenAPI 调用与双 Plan 探测

**Files:**
- Modify: `backend/src/coding_plan.rs`

**Interfaces:**
- Consumes: Task 8 签名原语。
- Produces:

```rust
fn parse_afp_windows(result: &serde_json::Value) -> Vec<QuotaWindow>
fn parse_coding_plan_windows(result: &serde_json::Value) -> Vec<QuotaWindow>
fn query_volcengine(base_url: &str, ak: &str, sk: &str) -> QuotaState
// coding_plan_quota 补齐 Volcengine 分支
```

- [ ] **Step 1: 写失败测试**

```rust
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
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test volcengine`

- [ ] **Step 3: 实现**

```rust
/// OpenAPI 错误信封（ResponseMetadata.Error 或顶层 Error）。
fn volcengine_response_error(body: &serde_json::Value) -> Option<(String, String)> {
    let err = body.get("ResponseMetadata").and_then(|m| m.get("Error")).or_else(|| body.get("Error"))?;
    let code = err.get("Code").and_then(|v| v.as_str).unwrap_or("").to_string();
    let msg = err.get("Message").and_then(|v| v.as_str).unwrap_or("").to_string();
    ((code.is_empty() && msg.is_empty()) == false).then_some((code, msg))
}

fn volcengine_is_auth_error_code(code: &str) -> bool {
    let c = code.to_lowercase();
    ["auth", "signature", "denied", "unauthorized", "forbidden", "credential", "token"].iter().any(|k| c.contains(k))
}

const VOLCENGINE_AKSK_HINT: &str = "请检查 AccessKey ID / Secret 是否正确，且账号具备方舟用量查询（OpenAPI）权限";

enum VolcCall {
    Body(serde_json::Value),
    Auth(String),
    Soft(String),
}

/// 单次控制面调用：POST https://open.volcengineapi.com/?Action=...&Version=2024-01-01&Region=...
/// 空 body；火山对签名类错误常回 4xx + Error 信封（而非 401/403），两条路径都要解析。
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
    let raw = match resp.text() {
        Ok(t) => t,
        Err(e) => return VolcCall::Soft(format!("读取响应失败: {e}")),
    };
    let parsed: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return VolcCall::Soft(format!("API error (HTTP {status}): {}", &raw[..raw.len().min(200)])),
    };
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

/// Agent Plan（GetAFPUsage）：Quota/Used 绝对值；AFPDaily 官方已隐藏，跳过。
fn parse_afp_windows(result: &serde_json::Value) -> Vec<QuotaWindow> {
    let mut windows = Vec::new();
    for (key, name) in [("AFPFiveHour", W5H), ("AFPWeekly", W7D), ("AFPMonthly", W30D)] {
        let Some(win) = result.get(key) else { continue };
        let quota_v = win.get("Quota").and_then(|v| v.as_f64).unwrap_or(0.0);
        if quota_v <= 0.0 { continue; }
        let used = win.get("Used").and_then(|v| v.as_f64).unwrap_or(0.0);
        windows.push(QuotaWindow {
            key: key.map_or_else(|| name.0.into(), |_| name.0.into()),
            window_name: name.1.into(),
            used_percent: Some(used / quota_v * 100.0).filter(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            amount_text: None,
            resets_at: win.get("ResetTime").and_then(extract_reset_time),
        });
    }
    windows
}

/// Coding Plan（GetCodingPlanUsage）：Level 标签 → 窗口，防御式兼容多字段名。
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
    let arr = result.get("QuotaUsage").and_then(|v| v.as_array)
        .or_else(|| result.get("Usages").and_then(|v| v.as_array))
        .or_else(|| result.get("Details").and_then(|v| v.as_array));
    let Some(arr) = arr else { return windows };
    for item in arr {
        let label = ["Level", "Type", "Period", "Label", "Window"].iter()
            .find_map(|k| item.get(k).and_then(|v| v.as_str)).unwrap_or("");
        let Some((key, name)) = window_label(label) else { continue };
        let percent = ["Percent", "UsedPercent", "UsagePercent"].iter()
            .find_map(|k| item.get(k).and_then(|v| v.as_f64)).unwrap_or(0.0);
        windows.push(QuotaWindow {
            key: key.into(), window_name: name.into(),
            used_percent: Some(percent).filter(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            amount_text: None,
            resets_at: item.get("ResetTime").or_else(|| item.get("ResetTimestamp")).and_then(extract_reset_time),
        });
    }
    windows
}

fn query_volcengine(base_url: &str, ak: &str, sk: &str) -> QuotaState {
    let region = volcengine_region(base_url);
    let mut soft_errors: Vec<String> = Vec::new();
    // 1) Agent Plan：GetAFPUsage
    match volcengine_openapi_call(&region, ak, sk, "GetAFPUsage") {
        VolcCall::Auth(reason) => return QuotaState::Unauthorized { reason },
        VolcCall::Soft(reason) => soft_errors.push(format!("GetAFPUsage: {reason}")),
        VolcCall::Body(body) => {
            let result = body.get("Result").unwrap_or(&body);
            let windows = parse_afp_windows(result);
            if !windows.is_empty() {
                let plan = result.get("PlanType").and_then(|v| v.as_str).map(|s| format!("Agent Plan {}", s.trim()));
                return QuotaState::Ok { windows, plan };
            }
        }
    }
    // 2) Coding Plan：GetCodingPlanUsage
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
```

`coding_plan_quota` 补齐：

```rust
        Some(PlanProvider::Volcengine) => match extras.volc_aksk() {
            Some((ak, sk)) => query_volcengine(base_url, ak, sk),
            None => QuotaState::Unsupported {
                reason: "火山套餐查询需要账号 AccessKey ID + Secret（控制面 OpenAPI，与推理 Key 是两套凭证）；请在设置 → 套餐查询中填写".into(),
            },
        },
```

- [ ] **Step 4: 运行确认通过**（`cargo test` 全量绿）

---

### Task 10: Grok 本机凭证读取

**Files:**
- Modify: `backend/src/creds.rs`

**Interfaces:**
- Produces:

```rust
pub fn local_grok_auth() -> Result<String, String> // Ok(token)；Err 为可直接展示的原因
fn parse_grok_auth_json(content: &str, now_secs: i64) -> Result<String, String> // 纯函数，供测试
```

- [ ] **Step 1: 写失败测试**（`creds.rs` 的 `#[cfg(test)]` 区）

```rust
    #[test]
    fn grok_auth_prefers_oidc_entry_and_reports_expiry() {
        let content = r#"{
            "https://accounts.x.ai/sign-in": {"key": "legacy-token"},
            "https://auth.x.ai::client-abc": {"key": "oidc-token", "expires_at": "2099-01-01T00:00:00Z"}
        }"#;
        assert_eq!(parse_grok_auth_json(content, 0).unwrap(), "oidc-token");

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
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test grok_auth`

- [ ] **Step 3: 实现**（加在 `creds.rs` 末尾；`~/.grok/auth.json`，顶层 scope→条目 map，OIDC `https://auth.x.ai::` 前缀优先、legacy `https://accounts.x.ai/sign-in` 兜底）

```rust
/// Grok CLI OAuth 凭证（~/.grok/auth.json）。
pub fn local_grok_auth() -> Result<String, String> {
    let path = std::env::var("USERPROFILE").ok()
        .map(|home| std::path::PathBuf::from(home).join(".grok").join("auth.json"))
        .filter(|p| p.exists())
        .ok_or_else(|| "未找到本机 Grok 凭证（~/.grok/auth.json），请先 grok login".to_string())?;
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取 Grok 凭证失败: {e}"))?;
    let now = chrono::Utc::now().timestamp();
    parse_grok_auth_json(&content, now)
}

/// 顶层是 scope → 条目的 map；选 key 非空的首选条目（OIDC 优先），过期视为失效。
fn parse_grok_auth_json(content: &str, now_secs: i64) -> Result<String, String> {
    let root: std::collections::BTreeMap<String, serde_json::Value> = serde_json::from_str(content)
        .map_err(|e| format!("Grok auth.json 不是有效 JSON: {e}"))?;
    let mut oidc = None;
    let mut legacy = None;
    for (scope, value) in &root {
        let key = value.get("key").and_then(|v| v.as_str).unwrap_or("");
        if key.is_empty() { continue; }
        if scope.starts_with("https://auth.x.ai::") { oidc = Some((scope, key, value)); }
        else if scope == "https://accounts.x.ai/sign-in" || scope.contains("/sign-in") { legacy = Some((scope, key, value)); }
    }
    let (_, key, entry) = oidc.or(legacy)
        .ok_or_else(|| "Grok auth.json 中没有可用的 access token".to_string())?;
    if let Some(expires_at) = entry.get("expires_at").and_then(|v| v.as_str) {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(expires_at) {
            if dt.timestamp() < now_secs {
                return Err("Grok OAuth token 已过期，请重新 grok login".into());
            }
        }
    }
    Ok(key.to_string())
}
```

- [ ] **Step 4: 运行确认通过**（`cargo test grok_auth` 全绿）

---

### Task 11: Grok 计费查询（gRPC-Web + protobuf 启发式解析）

**Files:**
- Create: `backend/src/grok_quota.rs`
- Modify: `backend/src/lib.rs`（`mod grok_quota;` + `local_grok_quota` 命令，注册进 `invoke_handler`，紧挨 `local_codex_quota` 的写法 lib.rs:403）
- Modify: `backend/src/quota_cache.rs` 无需改——`local_grok_quota` 复用 lib.rs 中 `cached_query(&QUOTA_CACHE, "local-grok", ...)` 与 `quota_cache_outcome`，并镜像 `local_codex_quota` 的托盘更新逻辑（仅当灵动岛绑定的是 Grok 时才调 `update_tray_quota_icon`；由于托盘图标逻辑假设 claude/codex，v1 先不接托盘，注释说明）。

**Interfaces:**
- Consumes: `creds::local_grok_auth()`（Task 10）、`quota::client()/QuotaState`。
- Produces（Task 14 前端调用）: Tauri 命令 `local_grok_quota(force: bool) -> QuotaState`。

- [ ] **Step 1: 写失败测试**（`grok_quota.rs` 内 `mod tests`，protobuf 构造辅助照抄参考 `subscription_grok.rs` tests 区）

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // ── protobuf 构造辅助（照抄 docs/reference/cc-switch/subscription_grok.rs tests）──
    fn varint(mut value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if byte == 0 && value == 0 { out.push(byte); break; }
            // 注意：与参考实现一致的写法见下方实现区；此处以参考文件为准移植
        }
        out
    }

    #[test]
    fn billing_payload_heuristics_extract_percent_and_reset() {
        // 路径 [1,1]=float 已用百分比；[1,5,1]=varint 重置时间（未来 Unix 秒）
        let mut payload = field_float(1, 42.0f32);
        payload.extend(field_varint(5, 1_900_000_000)); // 嵌套占位
        let snap = parse_billing_payload(&grpc_frame(&payload), 1_800_000_000).unwrap();
        assert_eq!(snap.used_percent, 42.0);
        assert!(snap.resets_at.is_some());
    }

    #[test]
    fn zero_usage_with_period_marker_counts_as_zero_percent() {
        // proto3 省略 0 值：无 percent 字段 + 有用量周期标记 [1,6,1]=1 + 有未来重置时间 → 0%
        let mut payload = field_varint(6, 1);
        payload.extend(field_varint(5, 1_900_000_000));
        let snap = parse_billing_payload(&grpc_frame(&payload), 1_800_000_000).unwrap();
        assert_eq!(snap.used_percent, 0.0);
    }

    #[test]
    fn reset_distance_maps_to_window_tier() {
        assert_eq!(tier_window(Some(1_800_000_000 + 7 * 86_400), 1_800_000_000).0, "7d");
        assert_eq!(tier_window(Some(1_800_000_000 + 30 * 86_400), 1_800_000_000).0, "30d");
        assert_eq!(tier_window(Some(1_800_000_000 + 86_400), 1_800_000_000).0, "credits");
        assert_eq!(tier_window(None, 1_800_000_000).0, "credits");
    }
}
```

（注意：`varint` 辅助在移植时以参考文件 `subscription_grok.rs:736` 起的实现为准，上面注释处不要自创写法。）

- [ ] **Step 2: 运行确认失败**

Run: `cargo test billing_payload`

- [ ] **Step 3: 实现模块**（移植参考 `docs/reference/cc-switch/subscription_grok.rs`：`read_varint`/`scan_protobuf`/`grpc_web_data_frames`/`looks_like_protobuf_payload`/`parse_billing_payload`/gRPC 状态判定与 `query_grok_quota`，适配为岛内同步风格 + `QuotaState`）：

```rust
//! Grok（SuperGrok）积分额度查询（cc-switch 对齐）
//!
//! 端点：POST https://grok.com/grok_api_v2.GrokBuildBilling/GetGrokCreditsConfig
//! gRPC-Web + protobuf（无 .proto 定义，字段启发式扫描）；凭证来自本机 grok CLI（~/.grok/auth.json）。
//! 纪律：查询语义的 POST（空 gRPC 帧）；gRPC 状态 16/7 → Unauthorized，4/14 瞬时 → Failed。

use crate::quota::{self, QuotaState, QuotaWindow};
use crate::coding_plan::{WCREDITS, W7D, W30D};

pub fn local_grok_quota() -> QuotaState {
    match crate::creds::local_grok_auth() {
        Ok(token) => query_grok(&token),
        Err(reason) => QuotaState::Unauthorized { reason },
    }
}

fn query_grok(access_token: &str) -> QuotaState { /* 移植 query_grok_quota：5 字节空帧 body、
    Origin/Referer/x-grpc-web/x-user-agent 头、grpc-status 头与 trailer 双路径、
    parse_billing_payload 输出 used_percent + resets_at（Unix 秒）→
    tier_window 按重置距今天数选窗口（4–12 天→7d，20–45 天→30d，否则 credits），
    单窗口 QuotaState::Ok */ }
```

（实现时把 `query_grok` 注释处展开为完整移植代码，语义逐条对照参考文件 552-720 行；gRPC HTTP 408 与 grpc-status 4/14 → `QuotaState::Failed`；HTTP 401/403 → `Unauthorized`。）

- [ ] **Step 4: lib.rs 命令**

```rust
#[tauri::command]
async fn local_grok_quota(force: bool) -> Result<quota::QuotaState, String> {
    tauri::async_runtime::spawn_blocking(move || cached_query(
        &QUOTA_CACHE, "local-grok".to_string(), force, quota_cache_outcome, grok_quota::local_grok_quota,
    )).await.map_err(|e| e.to_string())?
}
```

（对照 lib.rs:403 `local_codex_quota` 的实际签名与缓存调用形态微调；`generate_handler!` 列表 lib.rs:1572 处注册 `local_grok_quota`。）

- [ ] **Step 5: 运行确认通过**

Run: `cargo test grok`
Expected: 新增测试全绿，`cargo build` 无警告。

---

### Task 12: 设置存储：套餐查询辅助凭证

**Files:**
- Modify: `backend/src/settings.rs`（`Settings` 结构体 settings.rs:11）
- Modify: `backend/src/lib.rs`（设置读写命令透出新字段——沿用现有 settings 保存通道）

**Interfaces:**
- Consumes: 无。
- Produces（Task 13 依赖）:

```rust
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct PlanQuerySettings {
    /// 智谱团队版（open.bigmodel.cn）：团队管理后台用量页 URL 中可见
    pub zhipu_team_organization_id: String,
    pub zhipu_team_project_id: String,
    /// 火山方舟控制面 OpenAPI（与推理 Key 是两套凭据）
    pub volc_access_key_id: String,
    pub volc_secret_access_key: String,
}
// Settings 增加：#[serde(default)] pub plan_query: PlanQuerySettings,
```

- [ ] **Step 1: 写失败测试**（settings.rs tests）

```rust
    #[test]
    fn plan_query_settings_default_when_absent_in_file() {
        // 旧设置文件没有 plan_query 字段，反序列化后必须得到全空默认值而不是失败
        let store = Store::load_test_with(r#"{"refresh_minutes":5}"#);
        assert_eq!(store.get().plan_query.zhipu_team_organization_id, "");
        assert_eq!(store.get().plan_query.volc_secret_access_key, "");
    }
```

（`Store::load_test_with` 若无此测试基建，用既有 settings 测试的构造方式等价实现——settings.rs 已有 JSON 反序列化测试可照抄结构。）

- [ ] **Step 2: 运行确认失败**

Run: `cargo test plan_query`

- [ ] **Step 3: 实现**：`Settings` 加 `plan_query` 字段（`#[serde(default)]`），默认值 `PlanQuerySettings::default()`；确认设置导出/备份路径（`data_files.rs` 的备份若按字段枚举，需把 `plan_query` 中两个 secret 视为敏感，**不得出现在导出 JSON**——对照 db.rs:1615 的备份测试纪律处理）。

- [ ] **Step 4: lib.rs 设置命令透传**：找到现有设置读取/保存命令（grep `settings` in `lib.rs` commands 区），把 `plan_query` 加入返回 DTO 与更新入口；secret 字段更新时接受明文、返回时**只回 masked**（如 `***` + 尾 4 位）。

- [ ] **Step 5: 运行确认通过**（`cargo test settings` + `cargo build`）

---

### Task 13: 路由接入：connection_quota 与 validate_connection

**Files:**
- Modify: `backend/src/lib.rs:900-912`（`connection_quota` 路由）
- Modify: `backend/src/lib.rs:1150、1183、1237`（三处 `validate_connection` 调用）
- Modify: `backend/src/quota.rs:712`（`validate_connection` 签名扩展）

**Interfaces:**
- Consumes: `coding_plan::detect_provider/coding_plan_quota/PlanExtras`（Task 1-9）、`Cfg/settings::PlanQuerySettings`（Task 12）。
- Produces: 套餐连接的额度查询与保存时验证全链路。

- [ ] **Step 1: 写失败测试**（lib.rs 或 connections.rs 既有测试风格；路由是纯函数部分抽出来测）

```rust
    // coding_plan.rs tests
    #[test]
    fn dispatcher_covers_all_detected_providers_without_unsupported() {
        // 除 Volcengine（缺 AK/SK 时引导）外，detect 命中的每家都不得落回「尚未接入」
        let extras = PlanExtras::default();
        for base in [
            "https://api.kimi.com/coding/v1",
            "https://open.bigmodel.cn/api/anthropic",
            "https://api.z.ai/api/anthropic",
            "https://api.minimaxi.com/v1",
            "https://api.minimax.io/v1",
            "https://zenmux.ai/api/x",
            "https://opencode.ai/zen/go/v1",
        ] {
            let state = coding_plan_quota(base, "synthetic-key", &extras);
            assert!(!matches!(state, QuotaState::Unsupported { .. }), "{base} → {state:?}");
        }
    }
```

- [ ] **Step 2: 运行确认失败**（ZenMux/OpenCode 已在各自任务接入，此时应已无 Unsupported；确认无遗漏）

Run: `cargo test dispatcher_covers`

- [ ] **Step 3: lib.rs 路由**

```rust
            || match c.base_url.as_deref() {
                Some(base) if c.kind == "api" && !base.is_empty() => {
                    match coding_plan::detect_provider(base) {
                        Some(_) => {
                            let pq = &app.state::<Cfg>().0.lock().unwrap().get().plan_query; // 按 Cfg 实际 API 取值
                            let extras = coding_plan::PlanExtras {
                                team_organization_id: Some(&pq.zhipu_team_organization_id),
                                team_project_id: Some(&pq.zhipu_team_project_id),
                                volc_access_key_id: Some(&pq.volc_access_key_id),
                                volc_secret_access_key: Some(&pq.volc_secret_access_key),
                            };
                            coding_plan::coding_plan_quota(base, &secret, &extras)
                        }
                        None => quota::sub2api_quota(base, &secret),
                    }
                }
                _ => match (c.platform.as_str(), c.kind.as_str()) { /* 原样保留 */ },
            },
```

（`Cfg` 取值写法以 lib.rs 现有 `app.state::<Cfg>().0.get()` 形态为准；闭包内不能借 `app` 时先在闭包外把 `PlanQuerySettings` clone 进 move 闭包。）

- [ ] **Step 4: validate_connection 扩展**：`quota.rs::validate_connection` 增加参数 `plan: Option<&coding_plan::PlanExtras>`（或把整个验证入口迁到 lib.rs 做分发——以改动最小为准）；`api + base_url` 时先 `detect_provider`，命中走 `coding_plan_quota`（Ok→合法，其余状态把 reason 作为错误返回），未命中维持 `fetch_s2`。三处调用点同步传参。

- [ ] **Step 5: 运行确认通过**

Run: `cargo test`
Expected: 全量测试绿（含既有 sub2api / official 路由测试不受影响）。

---

### Task 14: 前端：设置分区 + Grok 平台接入

**Files:**
- Modify: `frontend/src/views/SettingsView.tsx`（新增「套餐查询」分区，四个输入框）
- Modify: `frontend/src/lib/platforms.ts:50`（grok 行）
- Modify: `frontend/src/lib/api.ts:10` 旁（`localGrokQuota`）
- Modify: `frontend/src/lib/useQuota.ts`（本地 Grok 查询 hook）
- Modify: `frontend/src/views/OverviewView.tsx`（Grok 额度卡片）
- Modify: `frontend/src/types.ts`（settings DTO 增补 plan_query 字段）

**Interfaces:**
- Consumes: `local_grok_quota` 命令（Task 11）、settings DTO（Task 12）。
- Produces: 用户可配置辅助凭证；Grok 额度可见。

- [ ] **Step 1: types.ts 与 api.ts**

```ts
// types.ts：与 Rust PlanQuerySettings 对齐
export interface PlanQuerySettings {
  zhipu_team_organization_id: string
  zhipu_team_project_id: string
  volc_access_key_id: string
  volc_secret_access_key: string
}
// Settings DTO 增加可选 plan_query?: PlanQuerySettings

// api.ts（localCodexQuota 旁）
export const localGrokQuota = (force = false) => invoke<QuotaStateDto>("local_grok_quota", { force })
```

- [ ] **Step 2: useQuota.ts 支持本地 Grok**

`useQuota.ts:53` 的双路分支扩展为三路（`localCodex` 参数升级为 `local?: "codex" | "grok"`，或新增 `useLocalGrokQuota`——以对现有调用点改动最小为准；`queryKey` 相应区分）。

- [ ] **Step 3: SettingsView 新增分区**

在「常规」分区附近加「套餐查询」`SectionTitle` + 四个 `SettingRow`（智谱团队组织 ID / 智谱团队项目 ID / 火山 AccessKey ID / 火山 AccessKey Secret——最后一个用密码型输入）。说明文案写明：「仅智谱团队版与火山方舟套餐需要；个人版套餐凭连接自动识别，无需配置」。保存走 Task 12 透出的设置保存命令。

- [ ] **Step 4: platforms.ts 启用 Grok**

```ts
  {
    id: "grok",
    name: "Grok",
    availability: "available",
    capabilities: ["subscription_quota"],
  },
```

（不开放 `auth_connection`/`api_connection`/`local_sessions`——Grok 额度只来自本机 grok CLI 凭证，会话采集尚不支持。）

- [ ] **Step 5: OverviewView Grok 额度卡片**

在额度卡片区（OverviewView.tsx:266 附近网格）为 Grok 增加一张额度卡，数据用 Step 2 的 hook；渲染复用现有 `QuotaStateDto` 五态展示组件（QueryStateNotice / 窗口行），不新写状态逻辑。

- [ ] **Step 6: 前端验证**

Run: `pnpm --dir frontend build`
Expected: TypeScript 编译与 Vite 构建通过。

---

### Task 15: 文档与全量验证

**Files:**
- Modify: `README.md`（「目前的状态」表增加一行：编程套餐额度查询 cc-switch 全量对齐）
- Modify: `todo.md`（记录真实环境待验证项：智谱团队版、火山 AK/SK、Kimi/MiniMax/ZenMux/OpenCode/Grok 的真实账号联调）

- [ ] **Step 1: 全量后端测试**

Run: `cargo test`
Expected: 全绿。

- [ ] **Step 2: 本机冒烟**（鼠鼠机器上有智谱个人版）

Run: `cargo test local_zhipu_read_only_smoke -- --ignored`
Expected: `QuotaState::Ok`，5h/7d 两窗口与控制台一致。

- [ ] **Step 3: 前端构建**

Run: `pnpm --dir frontend build`
Expected: 通过。

- [ ] **Step 4: 桌面端手工验收**（`pnpm tauri dev`）

1. 主面板连接页：智谱连接的额度卡显示 5 小时/周窗口（不再回退本机统计）
2. 设置 → 套餐查询：四项可保存、masked 回显
3. 概览：Grok 卡片显示「未找到本机 Grok 凭证」的未授权态（未登录 grok 时属正常）

- [ ] **Step 5: 报告并等待鼠鼠验收**——**不做任何 git 操作**，等鼠鼠确认后统一提交。

---

## Self-Review 结论

- **覆盖检查**：鼠鼠选定的范围 = cc-switch 全部 9 家套餐 + Grok。Task 2-3 智谱三态、4 Kimi、5 MiniMax、6 ZenMux、7 OpenCode Go、8-9 火山、10-11 Grok、12-13 设置与路由、14 前端。Claude/Codex OAuth 岛上已有，不重复。Gemini 是模型级配额非 5h/周形态，cc-switch 亦归订阅类，未列入本次（鼠鼠范围内无此项）。
- **占位扫描**：Task 11 的 `query_grok` 与 Task 13 的 Cfg 取值以「移植参考 + 精确行号锚点」表达，参考源码随 Task 0 固化进 `docs/reference/`，不属于悬空占位。
- **类型一致性**：`PlanExtras`/`detect_provider`/`coding_plan_quota`/窗口常量在 Task 1 定义后，Task 2-9、13 全部按 Task 1 的 `Interfaces` 签名引用；`local_grok_quota` 命令名在 Task 11（后端）与 Task 14（前端 invoke）一致。
