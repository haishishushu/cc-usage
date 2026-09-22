# 八个平台完整接入实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** 完成 Claude、Codex、Gemini、Grok、Zcode、Trae、Qoder、Workbuddy 的平台能力、连接操作、真实数据采集及统计／灵动岛联动；逐项报告实际可用与无法验证的能力。

**Architecture:** 保留当前 React 页面和 Tauri 命令边界。后端用平台能力表和独立来源适配器消除前后端分支失配；Token、积分、上下文占用分别建模，统一持久化和查询。凭证读取、远程验证、外部应用账号切换分别处理。

**Tech Stack:** 现有 Rust／Tauri 2、rusqlite、serde、reqwest、React／TypeScript、Node test；先复用现有依赖，不为通用解析增加 SDK。

**Spec:** [已确认设计](../specs/2026-09-22-eight-platform-integration-design.md)

## Global Constraints

- 全程中文，用户称鼠鼠，自称小的；所有新增文本 UTF-8。
- 不执行 git add、commit、push、checkout、reset；保留开始时所有工作区修改。
- 不自动更改实际账号、模型或渠道；外部配置写入测试限定临时目录。
- 不输出密钥、用户身份、完整会话内容，不将第三方凭证发送到猜测的接口。
- `null`、来源报告的 `0`、来源明确声明不可用的占位 `0` 必须区分。
- 未验证来源不得声明实测完成；不得以“支持空状态”替代某平台适配已实现。
- 原生应用数据库只读查询；不使用会创建不存在文件的 SQLite 打开方式。
- 单平台错误不得阻断其他平台。桌面端不得回退浏览器演示数据。

## Review Focus

1. 本机根目录存在但没有有效登录／会话：必须返回准确缺项，不产生“已连接”假象（任务 1、3）。
2. 同消息多次写入、旧 JSON 与新 JSONL 共存、SQLite 累计快照回退：不能重复计数或吞掉更正（任务 2、4、5）。
3. 国内／国际版具有相同会话或消息 ID：独立来源标识避免串账号、串数据（任务 2、5）。
4. OAuth 只替换 access token、写第二个配置文件失败：不可保留混合身份或半成功状态（任务 3）。
5. 切换平台后旧网络响应返回、文件锁定、来源字段格式变化：仅当前来源更新，旧值标过期，错误可恢复（任务 6、7）。

## 任务 1：平台能力与状态契约

**Files:** 新建 `backend/src/platforms.rs`、`frontend/src/lib/platformCapabilities.ts` 及 `.test.mjs`；修改 `backend/src/lib.rs`、`backend/src/connections.rs`、`backend/src/creds.rs`、`frontend/src/types.ts`、`frontend/src/lib/api.ts`、`frontend/src/lib/platforms.ts`、`frontend/src/lib/connectionPlatformTab.ts`。

**接口：**

```rust
#[derive(Clone, Copy, serde::Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState { Supported, Unverified, Unsupported }

#[derive(serde::Serialize)]
pub struct PlatformCapabilityDto {
    pub id: String,
    pub state: CapabilityState,
    pub reason: Option<String>,
}

#[derive(serde::Serialize)]
pub struct PlatformDto {
    pub id: String,
    pub name: String,
    pub capabilities: Vec<PlatformCapabilityDto>,
}

pub fn is_known_platform(id: &str) -> bool;
pub fn platform_catalog() -> Vec<PlatformDto>;
```

能力 ID 为 `local_connection`、`api_connection`、`remote_validation`、`external_switch`、`local_sessions`、`token_usage`、`credits`、`context_usage`、`subscription_quota`、`gateway_balance`、`cost_estimate`。配置未找到属于来源状态，不把已实现的能力改成未实现。

- [ ] 写失败测试：八个正确 ID 接受，`tare` 和任意未知 ID 拒绝；读取 Trae 本机连接不再被 Claude／Codex 白名单提前拒绝。

```rust
#[test]
fn catalog_accepts_all_eight_ids_and_rejects_typo() {
    for id in ["claude", "codex", "gemini", "grok", "zcode", "trae", "qoder", "workbuddy"] {
        assert!(is_known_platform(id));
    }
    assert!(!is_known_platform("tare"));
    assert!(!is_known_platform("unknown"));
}
```

- [ ] `cargo test --manifest-path backend/Cargo.toml catalog_accepts_all_eight_ids_and_rejects_typo`，记录首次失败。
- [ ] 实现后端能力表与 `platform_catalog` Tauri 命令；`prepare_new`、`local_connections`、导入校验使用同一 ID 校验。未知平台不回落 Claude。
- [ ] 前端保留图标与名称静态配置；运行时按钮能力由后端目录决定。读取失败时显示失败，不擅自启用所有能力。
- [ ] 运行该后端测试及前端能力测试：未验证能力不宣称支持，已支持但无配置显示“未配置”。

## 任务 2：统一来源、计量单位与幂等写入

**Files:** 新建 `backend/src/source_records.rs`、`backend/src/source_store.rs` 及单元测试；修改 `backend/src/db.rs`、`backend/src/collector.rs`、`backend/src/lib.rs`、`frontend/src/lib/api.ts`。

**接口：**新平台适配器输出下列记录，旧 Claude／Codex 路径暂不重写。

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputSemantics { ExcludesCache, IncludesCache, Unknown }

#[derive(Clone, Debug)]
pub struct SourceRecord {
    pub source_id: String,
    pub request: crate::db::RequestRecord,
    pub input_semantics: InputSemantics,
    pub reasoning_tokens: Option<i64>,
    pub tool_tokens: Option<i64>,
    pub credits: Option<f64>,
    pub original_credits: Option<f64>,
    pub billable: Option<bool>,
    pub context_used: Option<i64>,
    pub context_limit: Option<i64>,
    pub context_ratio: Option<f64>,
}

pub fn ingest_source_records(
    conn: &mut rusqlite::Connection,
    records: &[SourceRecord],
) -> Result<usize, String>;
```

为 `requests` 增加 `input_semantics` 列；历史 Claude 标 `excludes_cache`、Codex 标 `includes_cache`，其他历史记录标 `unknown`。附加量存在 `request_metrics(request_id PRIMARY KEY, reasoning_tokens, tool_tokens, credits, original_credits, billable, context_used, context_limit, context_ratio)`。来源状态独立表 `source_state(source_id PRIMARY KEY, platform, state, observed_at_ms, last_success_ms, reason)`。迁移保留历史去重键。积分展示限定精度，但内部保留来源数值，测试汇总使用误差容限而非整数四舍五入。

- [ ] 写失败测试：同来源同 ID 导入两次，请求与积分都只保留一份；两个来源相同 ID 保留两份；`credits=0.65` 不增加 Token；未知 Token 不被默认 0 覆盖。
- [ ] 分别验证输入口径：Claude `2+380+198100` 的总输入为 `198482`；含缓存输入 `1000`、命中 `800` 的新增输入为 `200`；口径未知保持未知。
- [ ] 修改新来源写入：完整快照按来源标识更新；请求级最终值与修正事件在同一事务中写入。新适配器不得直接套用旧 `max(old,next)` 合并累计快照。
- [ ] 区分账单与留存会话：Gemini 回退消息后以官方回放结果重建该会话快照，并明确统计是“本地留存会话用量”；不得称为完整已付费用。流式采集和最终快照重建事务不得互相覆盖。
- [ ] 统计新增输入改为按记录 `input_semantics` 计算；积分单独汇总。仅上下文占用的来源不造请求或 Token 总量。
- [ ] 导出格式升级为版本 2，保留版本 1 导入；版本 2 含来源和附加量。清理历史时同步清理附加量，不让旧源记录下一轮重导入。
- [ ] 运行 `cargo test --manifest-path backend/Cargo.toml source_store` 和现有 `db_breakdown_tests`，验证旧 Claude／Codex 结果不变。

## 任务 3：完整连接生命周期

**Files:** 新建 `backend/src/platform_credentials.rs`、`backend/src/connection_probe.rs`；修改 `backend/src/creds.rs`、`backend/src/connections.rs`、`backend/src/cli_apply.rs`、`backend/src/lib.rs`、`frontend/src/lib/useConnections.ts`、`frontend/src/components/settings/ConnectionDialogs.tsx`、`ConnectionTable.tsx`。

**接口：**

```rust
#[derive(serde::Serialize)]
pub struct ConnectionProbe {
    pub state: String, // readable / verified / expired / unavailable / failed
    pub scope: String, // local / remote
    pub checked_at_ms: i64,
    pub latency_ms: Option<u64>,
    pub reason: Option<String>,
}

pub fn discover_local_connections(
    platform: &str,
    home: &std::path::Path,
    app_data: &std::path::Path,
) -> Result<Vec<crate::connections::NewConnection>, String>;
```

凭证结构与路径在任务 4／5 的资料中限定，不再遍历通用字段找“第一个像 token 的字符串”。原生来源允许注册不含 token 的本机监控连接，`auth` 仅作为现有兼容类型；界面明确标“本机来源”，不宣称官方订阅已验证。

- [ ] 写失败测试：本机来源无 token 可注册为待读取／已发现；没有远程验证时状态不是 `verified`；空目录不是可用来源；相同来源重复获取不新增连接。
- [ ] 移除 `test_connection` 对四个平台无条件 `ok:true` 的路径，返回分范围结果。检测不先要求启用，避免“必须启用才能知道能否启用”的闭环。
- [ ] `enable_connection` 根据 `external_switch` 能力分支：本机监控仅恢复本应用采集；已实现外部切换的平台才调用写配置代码。按钮或结果明确操作范围。
- [ ] 修复 Claude API 切换后的凭证优先级、默认模型与地址一致性；Codex 保留自定义 provider、配置目录和无关 TOML。Auth 不移植单个 access token；只选择当前本机完整身份，不能切换的历史身份返回明确原因。
- [ ] 两文件配置更新在隔离目录中验证：先备份，失败恢复，只有全成功后改变连接状态。未知平台无任何外部写入。
- [ ] 编辑／移除／暂停统一刷新缓存和前端。暂停不会删除源数据；移除不会退出第三方应用。
- [ ] 运行 `cargo test --manifest-path backend/Cargo.toml connections`、`cargo test --manifest-path backend/Cargo.toml cli_apply`，以及连接前端测试。

## 任务 4：Gemini、Grok、Trae 适配

**Files:** 新建 `backend/src/adapters/mod.rs`、`gemini.rs`、`grok.rs`、`trae.rs`；修改 `backend/src/grok_quota.rs`、`backend/src/quota.rs`、`backend/src/platform_credentials.rs`。

所有适配器统一入口：

```rust
pub struct AdapterResult {
    pub records: Vec<crate::source_records::SourceRecord>,
    pub sessions: Vec<SourceSession>,
    pub sources: Vec<SourceStatus>,
}
pub struct SourceSession {
    pub source_id: String,
    pub platform: String,
    pub session_id: String,
    pub title: Option<String>,
    pub state: String,
    pub updated_at_ms: i64,
}
pub struct SourceStatus {
    pub source_id: String,
    pub platform: String,
    pub state: String, // ready / missing / unsupported / failed
    pub reason: Option<String>,
}
pub fn scan(home: &std::path::Path, app_data: &std::path::Path) -> AdapterResult;
```

Gemini 官方记录字段：`tokens.input`、`output`、`cached`、`thoughts`、`tool`、`total`。总量采用 `total`，不把 thoughts 再加到总量；输出分项与思考分项分开保存。input 已包含缓存，创建量无字段时未知。来源仅扫描 `.gemini/tmp/*/chats/session-*`，不将 Antigravity 目录当 CLI。

- [ ] 写 Gemini 失败测试：JSON 会话、JSONL 消息、同 ID 替换、`$set` checkpoint、`$rewindTo`、JSON/JSONL 迁移共存、部分尾行；合成输入 `input=1000,output=20,cached=800,thoughts=10,total=1030` 保留总量 1030。
- [ ] 实现 Gemini 记录重建及格式版本识别；无法判定生命周期时会话状态为未知，不用 mtime 推定正在生成。
- [ ] Gemini API Key 使用官方接口只读验证；OAuth 不复用到猜测接口。没有已验证额度响应时返回限定范围的 `unsupported`，本地 Token 统计仍工作。
- [ ] Grok 接入现有本机 OAuth 解析和额度模块到正式连接 ID；过期、gRPC 非零状态、空响应与字段不明测试不返回假额度。xAI API Key 与 Grok OAuth 不混用。xAI API Key 使用 `GET https://api.x.ai/v1/api-key` 只读检测，200 后还需检查 `api_key_blocked/api_key_disabled/team_blocked`，任一为 true 不能验证成功。
- [ ] xAI 账户余额仅在用户提供独立 Management key 和 team ID 时查询 `https://management-api.x.ai/v1/billing/teams/{team_id}/prepaid/balance`；普通 API Key 不尝试管理端点。该能力与消费版 Grok 积分独立展示，金额符号和美分单位按已验证响应保留。
- [ ] Trae 必须使用 Trae IDE 的官方／实际格式；不得使用 `bytedance/trae-agent` 的 CLI 日志代替。未取得可验证格式前，本机来源检测与能力限制可交付，Trae 会话／额度适配在验收表明确列为未完成，不宣称八平台全部完成。
- [ ] 运行各适配器单测，记录无本机样本的项目；样本缺口只集中请求一次，不猜字段填实现。

## 任务 5：Zcode、Qoder、Workbuddy 原生数据适配

**Files:** 新建 `backend/src/adapters/zcode.rs`、`qoder.rs`、`workbuddy.rs` 与隔离测试夹具；修改 `backend/src/platform_credentials.rs`。

适配器复用任务 4 的 `AdapterResult`。详细来源依据补在本计划“本机核实记录”。

- [ ] Zcode 使用 `.zcode/cli/db/db.sqlite` 的 `model_usage`，以 `id` 为记录键，关联 `session.id`；`logical_request_id/attempt_index` 只保留关联信息，不把不同已消耗重试折叠成一次。`turn_usage` 已是聚合，不再额外累计。时间用 `completed_at`，未完成时以 `started_at` 保留状态；总量优先 `provider_total_tokens`，回退 `computed_total_tokens`。
- [ ] Zcode 输入口径以 provider 类型及 raw_usage 白名单验证。已核实的本机样本 `input=14210,output=235,cache_read=11712,total=14445` 为含缓存输入，不能用 Claude 口径重复加缓存。失败行的缺失总量不能仅凭数据库默认零当作实际零用量。
- [ ] Zcode 合成 SQLite 测试：相同轮次更新、reasoning 与 total 同时存在、缓存字段缺失、损坏数据库、数据库不存在不被创建、WAL 更新；其中含真实数值 `cache_creation_input_tokens` 时必须出现在最终分项。
- [ ] Qoder 扫描 `.qoder/projects` 与 `.qoder-cn/projects`，请求去重优先 `request_id` 再 `message.id`，同消息多个 assistant 分片只计一次。国际／国内来源前缀分开。
- [ ] Qoder 测试：两个分片共用 message.id，最终积分 0.64880178 只记一次；`isApiErrorMessage` 无 usage 不产生零用量；`tokenCountsAvailable=false` 下零 Token 保持未知；有明确已知标记的真实 0 保留。保留 `billable=false` 与 `original_credits`，积分不能标成已经扣除的余额。
- [ ] Token 可用性标记必须关联到同一来源／会话及可验证时间范围；不能把当前 snapshot 的 false 追溯覆盖全部历史有值记录。标记无法关联的零值保留来源报告并附完整性未知，不自行补出正数。
- [ ] Workbuddy 分别读取 `.workbuddy/projects/**/*.jsonl` 和 `.workbuddy-ai/projects/**/*.jsonl` 的 `providerData`；包含 `function_call` 等所有携带 usage 的记录，不能只筛 assistant message。稳定键优先 `record.id`，回退 `record.messageId`；同 `conversationRequestId` 的多个调用都保留。
- [ ] Workbuddy 映射 `usage.inputTokens/outputTokens/totalTokens` 及 `rawUsage.prompt_tokens/completion_tokens/total_tokens`；OpenAI 形状 input 已含缓存。缓存命中优先 `prompt_tokens_details.cached_tokens`、再 `prompt_cache_hit_tokens`，不能被 `cache_read_input_tokens=0` 抢先覆盖。缓存创建仅采用语义已核实的 `cache_creation_input_tokens/prompt_cache_write_tokens`；rawUsage 形状变化时报来源格式错误，不默认套协议。
- [ ] Workbuddy 原生 SQLite 仅补充会话和上下文快照，`session_usage.used/size` 不进入消耗总量。`rawUsage.credit` 为请求积分，以 JSONL 为明细主来源；`credit_json` 是累计校验来源，不能再加一次。
- [ ] Workbuddy 测试：上下文快照下降不变成负消耗；多次相同快照不增加总量；无 Token 分项时保留未知；数据库锁定仅影响该来源并保留最后成功时间。
- [ ] 凭证状态读取限明确格式；加密容器不能当 token，无法可靠读取时返回“不支持读取此登录格式”，不会记录虚假连接成功。
- [ ] Qoder 登录快照读取 `.qoder[-cn]/.qoder-app-status.json` 的 `logged_in/snapshot_at`，只标带时间的登录状态。桌面 `auth.v1.dat` 是 Electron safeStorage 加密容器，不自行当 JSON 读取或输出；未接通授权凭证读取前不宣称在线额度可用。
- [ ] 运行三个适配器单测，再执行不打印身份或正文的本机只读核对；报告请求数、来源数及可用字段，不导出私密原始行。

## 任务 6：采集调度、额度刷新和来源状态

**Files:** 修改 `backend/src/collector.rs`、`backend/src/watcher.rs`、`backend/src/session_presence.rs`、`backend/src/lib.rs`、`backend/src/quota.rs`；新增 `backend/src/source_runtime.rs`。

- [ ] 失败测试：任一来源打不开，其他适配器仍入库；第一次目录缺失后新建目录，心跳可发现；SQLite WAL 更新触发重新读取；当前来源无新增但状态从 running 到 done 仍发事件。
- [ ] 原 `scan`／`scan_paths` 汇总旧采集器与新适配器；来源检查、读取和写入结果分开。`ScanResult` 增加 `sources: Vec<SourceStatus>`，保留已有字段兼容现有调用。
- [ ] `set_session_activity` 接受并保留 `unknown` 和 `waiting` 状态，不再把新适配器未知状态默认改成 done；旧 running/done/failed 行为通过回归测试保留。
- [ ] watcher 目标来自适配器路径列表。JSONL 看目录，SQLite 看数据库与 WAL 所在目录；去抖仍用已有 120ms，保留 10 秒补漏心跳。不要每 120ms 全盘递归。
- [ ] `list_sources` 返回真实来源，支持国内／国际实例；无来源时返回缺项说明而不是空白成功。
- [ ] 额度查询按平台／凭证身份／来源建立缓存键，修改凭证或地址后失效；不将读取会话成功当作额度认证成功。
- [ ] 为缺少已验证额度接口的平台返回具体 `unsupported` 原因；网络断开、401、403、429、解析失败分别保留现有结构，过期数据标时间。
- [ ] 运行采集器、watcher、缓存及生命周期测试；验证切换灵动岛平台后旧 cursor 不造成新增动画或漏更新。

## 任务 7：前端页面、统计、会话与灵动岛

**Files:** 修改 `frontend/src/types.ts`、`lib/api.ts`、`lib/useUsage.ts`、`lib/useQuota.ts`、`lib/useCollectionStatus.ts`、`lib/requestInput.ts`、`views/OverviewView.tsx`、`views/IslandWindow.tsx`、`components/settings/ConnectionDialogs.tsx`、`ConnectionTable.tsx`、`components/panel/UsageHero.tsx`、`backend/src/tray_summary.rs`；新建 `frontend/src/lib/sourceMetrics.ts` 与测试。

新增展示映射函数（纯函数，不依赖浏览器）：

```ts
export type SourceMetric = {
  tokens: number | null
  credits: number | null
  contextRatio: number | null
}
export function primaryMetric(metric: SourceMetric): {
  value: number | null
  unit: "Token" | "积分" | null
}
```

选择规则：实际 Token 已知优先 Token，否则实际积分已知显示积分，否则未知；上下文占用永远独立展示，不参与这个选择。

- [ ] 写失败测试：未知 Token、有积分、零积分、仅上下文比率、来源读取失败、平台切换后迟到响应；不存在将积分标为 Token 的分支。
- [ ] 连接标签均可查看各平台状态；添加、检测、外部切换按能力开放并说明范围。不再显示统一的“CC Switch 管理全部平台”说明。
- [ ] 统计卡、趋势、请求日志按来源与单位展示；没有 Token 数据不绘制虚假零趋势。混合未知的完整合计保持未知，已知部分如果显示必须明确范围。
- [ ] 使用后端记录口径计算日志总输入，移除 `platform !== claude` 即认定含缓存的通用假设。
- [ ] 灵动岛、停靠条、托盘使用同一快照字段与单位；有会话但无真实运行事件时显示状态未知，不编造持续运行时长。
- [ ] 测试 `primaryMetric({tokens:null,credits:0.65,contextRatio:0.5})` 返回积分 0.65；`{tokens:null,credits:null,contextRatio:0.5}` 返回未知。
- [ ] 运行 `node --test frontend/src/lib/*.test.mjs` 及 `pnpm --dir frontend build`；只在新失败或新改动后重复相应检查。

## 任务 8：整合审查与交付

**Files:** 新建 `docs/verification/2026-09-22-eight-platform-integration.md`；扩展 `scripts/acceptance` 的隔离场景。

- [ ] 一次完整后端测试：`cargo test --manifest-path backend/Cargo.toml`。不得启用会访问真实凭证或收费接口的 ignored 测试。
- [ ] 按 `scripts/acceptance/README.md` 启动带 `.acceptance-profile` 的隔离应用；覆盖八个平台切换及已支持操作的成功／失败／缺失状态。测试页面不能修改真实 CLI 配置。
- [ ] 对有本机样本的平台执行只读计数核对，分别记录代码验证与本机联调结果。截图检查长平台名、来源实例、错误说明及积分单位不截断关键含义。
- [ ] 检查修改差异，确认未覆盖用户原改动、未新增真实凭证夹具、未移除旧平台有效行为。
- [ ] 逐平台报告连接、Token、缓存、积分、额度、会话、灵动岛的实现及验证状态。任何未实现项保留为未完成，不能用通过共享测试替代。
- [ ] 未执行 Git 状态修改；是否打包安装依据用户后续要求，构建产物与已安装程序分开说明。

## 本机核实记录

本节由只读调查补充来源细节；不保存凭证、用户身份和会话正文。当前已确认：

- Zcode 的 `.zcode/cli/db/db.sqlite` 存在 `model_usage`／`turn_usage`，包含缓存创建／读取字段。本机 model_usage 21 行：14 completed、7 error；两张用量表不可同时累加。
- Qoder 国内样本的 4 条 assistant 只有 2 个 message.id，最终记录才携带 usage；国际样本是 API 错误记录，不能据此断言国际版不提供 usage。
- Qoder 原生上下文快照有 `tokenCountsAvailable=false`，其零 Token 属占位；上下文比例不是 Token 数。
- Workbuddy 的真实 Token/缓存/积分来自 `projects/**/*.jsonl`，已核对 5 份会话；4 份非零积分日志合计与各自 `credit_json` 合计吻合。应用本体 `SqliteConversationUsagePort.persistUsage` 将 used 写为 `usage.totalTokens ?? usage.inputTokens`，size 写为 contextWindow；这是上下文快照，非账户额度。
- Workbuddy 同一 conversationRequestId 可以含多条有效 usage，不能以该字段聚合去重。`sessions/*.json` 是进程心跳，进程仍在不表示会话正在生成。
- Gemini 官方 `chatRecordingTypes.ts` 与 `chatRecordingService.ts` 定义的 JSONL 更新为替换语义，`$rewindTo`／checkpoint 需要处理。

## 已验证依据与输入缺口

- Gemini：[记录类型](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/services/chatRecordingTypes.ts)、[记录／回放](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/services/chatRecordingService.ts)、[认证](https://github.com/google-gemini/gemini-cli/blob/main/docs/get-started/authentication.mdx)。本机未找到 CLI 会话，真实联调需要一次 CLI 使用后的日志。
- xAI：[只读 Key 检查](https://docs.x.ai/developers/rest-api-reference/inference/other)、[管理 API](https://docs.x.ai/developers/management-api-guide)、[账单接口](https://docs.x.ai/developers/rest-api-reference/management/billing)。本机无可验证 Grok OAuth 样本，私有额度协议暂不能宣称实测。
- Trae IDE：本机未发现安装数据，官方检索仅找到独立的 Trae Agent 项目；不能使用其 trajectory 格式冒充 IDE。完成 Trae 真实采集需要实际 IDE 样本或明确接口文档。
- Qoder 国内版额度端点在本机安装包静态证据中为 `https://openapi.qoder.com.cn/sash/api/v2/me/usage`，但未完成有效凭证获取与响应验证。国际版域名未经核实，不猜测。两者线上额度作为独立未完成项，不影响本地会话与积分。
- Zcode 套餐缓存只有状态，Workbuddy account snapshot 不能证明数值额度；两者在线剩余额度还需核实接口与凭证作用域。本地 Token／缓存可独立实现。

## 审阅与执行

鼠鼠已确认由主任务在当前工作区直接实施。执行结果与计划差异见 [执行记录](../../verification/2026-09-22-eight-platform-progress.md)，逐平台状态见 [验收记录](../../verification/2026-09-22-eight-platform-integration.md)。本文件保留原计划清单；复合项目未经逐项验证不全部勾选。
