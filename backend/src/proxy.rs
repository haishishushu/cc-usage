//! 本地代理（阶段二）：CLI 的 base_url 指向本机监听端口，转发到真实上游并计时。
//!
//! 纪律（todo.md「请求日志『用时 / 首字』与本地代理」）：
//! - **默认关闭**：不开代理时应用行为与纯只读采集完全一致。
//! - **可用性优先于统计**：上游异常时错误原样回传，绝不让 CLI 拿到半成品响应；
//!   连续不可达且开关允许时自动回退直连（还原 CLI 配置并停止监听）。
//! - **两个性能死穴**：上游使用进程级单例 client 的长连接池；响应流零缓冲透传，
//!   首个字节到达即转发——首字延迟正是本需求要测的指标。
//! - **不注入凭证**：请求头透传，只剥离 hop-by-hop 及会干扰传输的头。
//!
//! 计时与关联：上游第一个字节记 `first_token_ms`，响应流结束记 `duration_ms`；
//! 从响应流头部（上限 [`HEAD_CAP`]）防御式提取去重键（Claude `message.id` /
//! Codex `response_id`），按 `(source='local', dedup_key)` 回填 [`crate::db`] 的
//! `requests` 行；记录尚未入库时转入待关联队列，由采集扫描后的
//! [`flush_pending`] 补写（CLI 写日志 → notify → 扫描 → flush，时序自然衔接）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use futures_util::StreamExt;
use tauri::{Emitter, Manager};

use crate::proxy_config::{is_self_base, ProxyPlatform};

/// 响应流头部快照上限：Claude `message_start` 与 Codex `response.created`
/// 都在流的最开头，64 KiB 足以覆盖，同时防止大响应常驻内存。
const HEAD_CAP: usize = 64 * 1024;
/// 请求体缓冲上限：长上下文请求可达数十 MB，取 512 MB（axum 默认 2 MB 会 413）。
const REQUEST_BODY_LIMIT: usize = 512 * 1024 * 1024;
/// 待关联队列 TTL：超过后放弃（响应结束到 CLI 写日志通常毫秒~秒级）。
const PENDING_TTL_MS: i64 = 10 * 60 * 1000;
/// 上游连续不可达次数达到该值即触发自动回退直连。
const FALLBACK_THRESHOLD: u32 = 3;

/// 待关联队列：响应结束时请求记录尚未入库，先暂存等采集扫描后补写。
#[derive(Debug, Clone)]
pub(crate) struct PendingTiming {
    pub dedup_key: String,
    pub first_token_ms: i64,
    pub duration_ms: i64,
    pub status_code: i64,
    pub created_ms: i64,
}

/// 生产环境的待关联队列与最近错误。独立于运行时存在，便于 [`flush_pending`] 随时访问。
static PENDING: LazyLock<Arc<Mutex<Vec<PendingTiming>>>> = LazyLock::new(|| Arc::new(Mutex::new(Vec::new())));
static LAST_ERROR: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
/// 自动回退的原因：设置页用它区分「手动关闭」与「已回退直连」。
static FALLBACK_REASON: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
static PROXY: LazyLock<Mutex<Option<Arc<ProxyRuntime>>>> = LazyLock::new(|| Mutex::new(None));

pub(crate) struct ProxyRuntime {
    pub port: u16,
    shutdown: Arc<tokio::sync::watch::Sender<bool>>,
    pub deps: Arc<ProxyDeps>,
}

/// 转发依赖。测试可独立构造，不经 [`PROXY`]。
pub(crate) struct ProxyDeps {
    client: reqwest::Client,
    db: Arc<Mutex<rusqlite::Connection>>,
    /// 已接管平台 → 真实上游 base（无尾斜杠）。
    upstreams: HashMap<ProxyPlatform, String>,
    pending: Arc<Mutex<Vec<PendingTiming>>>,
    consecutive_failures: Arc<AtomicU32>,
    fallback_enabled: bool,
    shutdown: Arc<tokio::sync::watch::Sender<bool>>,
    /// 触发回退时的收尾动作（生产：还原 CLI 配置 + 停止监听 + 通知前端）。
    on_fallback: Arc<dyn Fn(&str) + Send + Sync>,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn elapsed_ms(t0: Instant) -> i64 {
    t0.elapsed().as_millis().min(i64::MAX as u128) as i64
}

/* ─────────────────────────── 平台判定与去重键提取（纯函数） ─────────────────────────── */

/// 按请求特征判定属于哪个平台的上游：优先特征头，其次路径前缀。
/// 无法判定时返回 None，由调用方决定是否按「唯一接管平台」兜底。
pub(crate) fn detect_platform(headers: &HeaderMap, path: &str) -> Option<ProxyPlatform> {
    // Anthropic 特征头（Claude Code 全部请求都带 anthropic-version 或 beta 头）
    if headers.contains_key("anthropic-version")
        || headers.contains_key("x-api-key")
        || headers.contains_key("anthropic-beta")
    {
        return Some(ProxyPlatform::Claude);
    }
    // Codex CLI 特征头（originator: codex_cli_rs、chatgpt-account-id）
    if headers.contains_key("chatgpt-account-id")
        || headers
            .get("originator")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("codex"))
    {
        return Some(ProxyPlatform::Codex);
    }
    let path = path.to_ascii_lowercase();
    // Anthropic API 风格路径（/v1/messages、/v1/complete 与 /api/* 管理端点）
    if path.starts_with("/v1/messages")
        || path.starts_with("/v1/complete")
        || path.starts_with("/api/")
    {
        return Some(ProxyPlatform::Claude);
    }
    // OpenAI / Codex 风格路径
    if path.starts_with("/v1/responses")
        || path.starts_with("/v1/chat/completions")
        || path.starts_with("/v1/completions")
        || path.starts_with("/v1/embeddings")
        || path.starts_with("/backend-api")
    {
        return Some(ProxyPlatform::Codex);
    }
    None
}

fn trim_ws(bytes: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = bytes.len();
    while start < end && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &bytes[start..end]
}

/// 从响应体头部提取去重键。只认两种形态，提取不到不猜：
/// - 非流式：整体是一个 JSON，顶层 `id` 即去重键
/// - SSE：`data:` 行的 JSON 里，Claude 取 `message.id`（message_start 事件），
///   Codex 取 `response.id`（response.created / response.completed 事件）
pub(crate) fn extract_dedup_key(platform: ProxyPlatform, head: &[u8]) -> Option<String> {
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(head) {
        if let Some(key) = json_id(platform, &value) {
            return Some(key);
        }
    }
    for line in head.split(|byte| *byte == b'\n') {
        let line = trim_ws(line);
        let Some(payload) = line.strip_prefix(b"data:").map(trim_ws) else {
            continue;
        };
        if payload.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(payload) {
            if let Some(key) = json_id(platform, &value) {
                return Some(key);
            }
        }
    }
    None
}

fn json_id(platform: ProxyPlatform, value: &serde_json::Value) -> Option<String> {
    let candidate = match platform {
        ProxyPlatform::Claude => value
            .get("id")
            .and_then(|v| v.as_str())
            .or_else(|| value.get("message").and_then(|m| m.get("id")).and_then(|v| v.as_str())),
        // 事件顶层没有 id；`response.id` 才是去重键，不会误取 item.id（在 "item" 键下）
        ProxyPlatform::Codex => value
            .get("id")
            .and_then(|v| v.as_str())
            .or_else(|| value.get("response").and_then(|r| r.get("id")).and_then(|v| v.as_str())),
    };
    candidate
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// 上游请求 URL：base（去尾斜杠）+ 原样路径与 query。
fn build_upstream_url(base: &str, uri: &axum::http::Uri) -> String {
    let trimmed = base.trim().trim_end_matches('/');
    match uri.query() {
        Some(query) => format!("{trimmed}{}?{query}", uri.path()),
        None => format!("{trimmed}{}", uri.path()),
    }
}

/* ─────────────────────────── 请求头 / 响应头透传 ─────────────────────────── */

fn is_hop_by_hop(name: &str) -> bool {
    matches!(
        name,
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
    )
}

fn strip_request_headers(headers: &HeaderMap) -> HeaderMap {
    headers
        .iter()
        .filter(|(name, _)| {
            let name = name.as_str();
            // accept-encoding：交给 reqwest 自行协商并自动解压，转发语义才一致
            !is_hop_by_hop(name)
                && !name.starts_with("proxy-")
                && name != "host"
                && name != "content-length"
                && name != "accept-encoding"
        })
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

fn strip_response_headers(headers: &HeaderMap) -> HeaderMap {
    // content-encoding：请求端已剥离 accept-encoding，reqwest 自动协商并解压；
    // 保留旧头会让下游把明文当压缩体解析。content-length 同理已失效，交给 hyper 分块。
    headers
        .iter()
        .filter(|(name, _)| {
            let name = name.as_str();
            !is_hop_by_hop(name) && name != "content-length" && name != "content-encoding"
        })
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

fn error_response(status: StatusCode, message: &str) -> Response {
    (status, message.to_string()).into_response()
}

/* ─────────────────────────── 转发与计时 ─────────────────────────── */

impl ProxyDeps {
    fn upstream_for(&self, platform: Option<ProxyPlatform>) -> Option<(ProxyPlatform, &str)> {
        if let Some(platform) = platform {
            return self.upstreams.get(&platform).map(|base| (platform, base.as_str()));
        }
        // 无法判定的路径：仅当只有一个平台被接管时兜底转发，多个平台不猜
        if self.upstreams.len() == 1 {
            return self.upstreams.iter().next().map(|(p, base)| (*p, base.as_str()));
        }
        None
    }

    /// 计时落库：提取去重键 → 回填 requests 行；记录未入库时转入待关联队列。
    fn settle_timing(&self, platform: ProxyPlatform, head: &[u8], first: i64, duration: i64, status: i64) {
        let Some(dedup_key) = extract_dedup_key(platform, head) else {
            return;
        };
        let applied = {
            let Ok(conn) = self.db.lock() else { return };
            crate::db::apply_proxy_timing(&conn, &dedup_key, first, duration, status).unwrap_or(0)
        };
        if applied == 0 {
            if let Ok(mut queue) = self.pending.lock() {
                queue.push(PendingTiming {
                    dedup_key,
                    first_token_ms: first,
                    duration_ms: duration,
                    status_code: status,
                    created_ms: now_ms(),
                });
            }
        }
    }

    fn note_success(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
        if let Ok(mut slot) = LAST_ERROR.lock() {
            *slot = None;
        }
    }

    /// 上游不可达（发送阶段的错误，含连接 / 超时 / DNS）。达到阈值触发回退。
    fn note_upstream_unreachable(&self, reason: String) {
        if let Ok(mut slot) = LAST_ERROR.lock() {
            *slot = Some(reason.clone());
        }
        if !self.fallback_enabled {
            return;
        }
        let count = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        if count >= FALLBACK_THRESHOLD {
            self.consecutive_failures.store(0, Ordering::Relaxed);
            (self.on_fallback)(&reason);
            let _ = self.shutdown.send(true);
        }
    }
}

/// 计时状态机：跟随响应流转发，首个数据块与流结束时各记一次时间。
struct TimingCore {
    deps: Arc<ProxyDeps>,
    platform: ProxyPlatform,
    t0: Instant,
    status: StatusCode,
    first_ms: Option<i64>,
    head: Vec<u8>,
    head_full: bool,
    settled: bool,
}

impl TimingCore {
    /// 中途出错（上游断流）时计时数据不完整，不落库，让 CLI 看到真实的连接中断。
    fn settle(&mut self, complete: bool) {
        if self.settled {
            return;
        }
        self.settled = true;
        if !complete {
            return;
        }
        let duration = elapsed_ms(self.t0);
        let first = self.first_ms.unwrap_or(duration);
        self.deps
            .settle_timing(self.platform, &self.head, first, duration, self.status.as_u16() as i64);
    }
}

/// 零缓冲透传 + 计时：每个数据块原样转发，前 [`HEAD_CAP`] 字节同时快照供键提取。
fn timed_body(response: reqwest::Response, core: TimingCore) -> Body {
    let stream = response.bytes_stream();
    let timed = futures_util::stream::unfold((stream, Some(core)), |(mut stream, core)| async move {
        let Some(mut core) = core else {
            return None;
        };
        match stream.next().await {
            Some(Ok(chunk)) => {
                if core.first_ms.is_none() {
                    core.first_ms = Some(elapsed_ms(core.t0));
                }
                if !core.head_full {
                    let take = (HEAD_CAP - core.head.len()).min(chunk.len());
                    core.head.extend_from_slice(&chunk[..take]);
                    core.head_full = core.head.len() >= HEAD_CAP;
                }
                Some((Ok::<Bytes, reqwest::Error>(chunk), (stream, Some(core))))
            }
            // 上游断流：结束下游响应（头已发出，只能中断连接），计时数据不落库
            Some(Err(_)) => {
                core.settle(false);
                None
            }
            None => {
                core.settle(true);
                None
            }
        }
    });
    Body::from_stream(timed)
}

async fn forward(State(deps): State<Arc<ProxyDeps>>, request: Request) -> Response {
    let t0 = Instant::now();
    let (parts, body) = request.into_parts();
    let platform = detect_platform(&parts.headers, parts.uri.path());
    let Some((platform, base)) = deps.upstream_for(platform) else {
        return error_response(
            StatusCode::BAD_GATEWAY,
            "本地代理：无法判定该请求属于哪个平台的上游（多平台接管时不猜测）",
        );
    };

    let body_bytes = match axum::body::to_bytes(body, REQUEST_BODY_LIMIT).await {
        Ok(bytes) => bytes,
        Err(error) => {
            return error_response(StatusCode::BAD_REQUEST, &format!("本地代理：读取请求体失败：{error}"));
        }
    };

    let mut upstream_request = deps
        .client
        .request(parts.method.clone(), build_upstream_url(base, &parts.uri))
        .headers(strip_request_headers(&parts.headers));
    if !body_bytes.is_empty() {
        upstream_request = upstream_request.body(body_bytes);
    }

    let response = match upstream_request.send().await {
        Ok(response) => response,
        // 发送阶段错误 = 上游不可达（连接 / 超时 / DNS），计入回退计数；CLI 拿到明确的 502
        Err(error) => {
            deps.note_upstream_unreachable(format!("上游不可达：{error}"));
            return error_response(StatusCode::BAD_GATEWAY, &format!("本地代理：上游不可达，{error}"));
        }
    };
    deps.note_success();

    let status = response.status();
    let headers = strip_response_headers(response.headers());
    let core = TimingCore {
        deps: deps.clone(),
        platform,
        t0,
        status,
        first_ms: None,
        head: Vec::new(),
        head_full: false,
        settled: false,
    };
    // http::response::Builder 没有批量 headers 方法，先建 body 再整体替换头表
    let mut builder = Response::builder().status(status).header("x-cc-usage-proxy", "1");
    match builder.headers_mut() {
        Some(slot) => *slot = headers,
        None => return error_response(StatusCode::BAD_GATEWAY, "本地代理：构造响应失败"),
    }
    builder
        .body(timed_body(response, core))
        .unwrap_or_else(|error| error_response(StatusCode::BAD_GATEWAY, &format!("本地代理：构造响应失败：{error}")))
}

/* ─────────────────────────── 生命周期：启动 / 停止 / 状态 ─────────────────────────── */

/// 设置页入口：校验参数后执行完整的接管 / 启动（或还原 / 停止）流程。
/// 任何一步失败都会回滚已写入的 CLI 配置与设置，不留半成品状态。
pub fn set_proxy(
    app: &tauri::AppHandle,
    port: u16,
    enabled: bool,
    fallback_direct: bool,
) -> Result<crate::settings::Settings, String> {
    if !(1024..=65535).contains(&port) {
        return Err("端口须在 1024–65535 之间".into());
    }
    if let Ok(mut slot) = FALLBACK_REASON.lock() {
        *slot = None;
    }

    if !enabled {
        stop();
        if let Err(error) = crate::proxy_config::restore_all_saved(app) {
            eprintln!("[代理] 还原 CLI 配置出现问题：{error}");
        }
        let next = app
            .state::<crate::Cfg>()
            .0
            .try_update(|settings| {
                settings.proxy_enabled = false;
                settings.proxy_port = port;
                settings.proxy_fallback_direct = fallback_direct;
                // 刻意保留 upstream：它是「最近已知网关」，供常驻自愈修复
                // CC Switch 等工具之后的回写残留（下次开启时会被重新探测覆盖）
            })?;
        let _ = app.emit("settings-changed", next.clone());
        return Ok(next);
    }

    // 开启（或改端口重开）：先干净地停掉旧实例并还原，再按当前本机配置重新探测
    stop();
    if let Err(error) = crate::proxy_config::restore_all_saved(app) {
        eprintln!("[代理] 重新接管前还原旧配置出现问题：{error}");
    }

    let mut originals = HashMap::new();
    let mut skipped = Vec::new();
    for platform in [ProxyPlatform::Claude, ProxyPlatform::Codex] {
        match crate::proxy_config::probe_original(platform) {
            Ok(original) => {
                originals.insert(platform, original);
            }
            Err(reason) => skipped.push(format!("{}：{reason}", platform.display_name())),
        }
    }
    if originals.is_empty() {
        return Err(format!("没有可接管的平台（{}）", skipped.join("；")));
    }

    app.state::<crate::Cfg>()
        .0
        .try_update(|settings| {
            settings.proxy_enabled = true;
            settings.proxy_port = port;
            settings.proxy_fallback_direct = fallback_direct;
            settings.proxy_claude_upstream = originals.get(&ProxyPlatform::Claude).cloned();
            settings.proxy_codex_upstream = originals.get(&ProxyPlatform::Codex).cloned();
        })
        .map_err(|error| format!("保存代理设置失败：{error}"))?;

    let proxy_base = format!("http://127.0.0.1:{port}");
    if let Err(error) = crate::proxy_config::apply_takeover(app, &proxy_base) {
        rollback(app);
        return Err(error);
    }
    if let Err(error) = start(app) {
        rollback(app);
        return Err(format!("代理监听启动失败：{error}"));
    }

    let next = app.state::<crate::Cfg>().0.get();
    let _ = app.emit("settings-changed", next.clone());
    println!("[代理] 已启动 127.0.0.1:{port}");
    Ok(next)
}

/// 失败回滚：还原 CLI 配置并清空代理设置。仅在开启流程中调用。
fn rollback(app: &tauri::AppHandle) {
    if let Err(error) = crate::proxy_config::restore_all_saved(app) {
        eprintln!("[代理] 回滚还原 CLI 配置失败：{error}");
    }
    let _ = app.state::<crate::Cfg>().0.try_update(|settings| {
        settings.proxy_enabled = false;
        settings.proxy_claude_upstream = None;
        settings.proxy_codex_upstream = None;
    });
    let _ = app.emit("settings-changed", app.state::<crate::Cfg>().0.get());
}

/// 启动监听（接管已完成，上游来自设置）。幂等：先停旧实例。
pub fn start(app: &tauri::AppHandle) -> Result<(), String> {
    let settings = app.state::<crate::Cfg>().0.get();
    let port = settings.proxy_port;
    let mut upstreams = HashMap::new();
    if let Some(base) = settings.proxy_claude_upstream.as_deref().filter(|s| !s.trim().is_empty()) {
        upstreams.insert(ProxyPlatform::Claude, base.trim().trim_end_matches('/').to_string());
    }
    if let Some(base) = settings.proxy_codex_upstream.as_deref().filter(|s| !s.trim().is_empty()) {
        upstreams.insert(ProxyPlatform::Codex, base.trim().trim_end_matches('/').to_string());
    }
    if upstreams.is_empty() {
        return Err("没有已接管的平台上游，无法启动".into());
    }

    // 长连接池：无总超时无读超时（流式响应间隙可能很长），只限制连接建立阶段
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(16)
        .tcp_nodelay(true)
        .build()
        .map_err(|error| format!("构建上游客户端失败：{error}"))?;

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let shutdown = Arc::new(shutdown_tx);

    let app_for_fallback = app.clone();
    let on_fallback: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |reason: &str| {
        eprintln!("[代理] 上游连续不可达，自动回退直连：{reason}");
        if let Ok(mut slot) = FALLBACK_REASON.lock() {
            *slot = Some(reason.to_string());
        }
        if let Err(error) = crate::proxy_config::restore_all_saved(&app_for_fallback) {
            eprintln!("[代理] 回退还原 CLI 配置失败：{error}");
        }
        let next = app_for_fallback.state::<crate::Cfg>().0.update(|settings| {
            settings.proxy_enabled = false;
            // 保留 upstream：与手动关闭同理，常驻自愈需要它修复后续回写残留
        });
        let _ = app_for_fallback.emit("settings-changed", next);
        if let Ok(mut guard) = PROXY.lock() {
            *guard = None;
        }
    });

    let deps = Arc::new(ProxyDeps {
        client,
        db: app.state::<crate::Db>().0.clone(),
        upstreams,
        pending: PENDING.clone(),
        consecutive_failures: Arc::new(AtomicU32::new(0)),
        fallback_enabled: settings.proxy_fallback_direct,
        shutdown: shutdown.clone(),
        on_fallback,
    });

    let router = Router::new()
        .fallback(forward)
        .layer(DefaultBodyLimit::max(REQUEST_BODY_LIMIT))
        .with_state(deps.clone());

    // 同步上下文里绑定端口：占用等错误要直接反馈给设置页，不能吞进后台任务
    let listener = tauri::async_runtime::block_on(tokio::net::TcpListener::bind(("127.0.0.1", port)))
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AddrInUse {
                format!("端口 {port} 已被占用，请更换端口")
            } else {
                format!("监听 127.0.0.1:{port} 失败：{error}")
            }
        })?;

    let signal = async move {
        let mut receiver = shutdown_rx;
        loop {
            if *receiver.borrow_and_update() {
                break;
            }
            if receiver.changed().await.is_err() {
                break;
            }
        }
    };
    tauri::async_runtime::spawn(async move {
        if let Err(error) = axum::serve(listener, router).with_graceful_shutdown(signal).await {
            eprintln!("[代理] 服务异常退出：{error}");
        }
    });

    *PROXY.lock().expect("代理状态锁") = Some(Arc::new(ProxyRuntime { port, shutdown, deps }));
    Ok(())
}

/// 停止监听（CLI 配置的还原由调用方负责）。
pub fn stop() {
    let Some(runtime) = PROXY.lock().expect("代理状态锁").take() else {
        return;
    };
    let _ = runtime.shutdown.send(true);
}

/// 应用启动时的恢复 / 自愈（setup 调用；隔离测试模式不进入）。
pub fn startup(app: &tauri::AppHandle) {
    let settings = app.state::<crate::Cfg>().0.get();
    if settings.proxy_enabled {
        // 复用完整开启流程：先把崩溃残留的代理地址还原成保存的上游，再重新接管并监听
        if let Err(error) = set_proxy(app, settings.proxy_port, true, settings.proxy_fallback_direct) {
            eprintln!("[代理] 启动恢复失败，保持直连：{error}");
        } else {
            println!("[代理] 已按上次设置恢复接管（端口 {}）", settings.proxy_port);
        }
    }
    // 无论开关状态，启动时先清理一次孤儿代理地址（崩溃 / CC Switch 回写残留）
    heal_orphans(app);
}

/// 清理「指向本机代理端口但代理未接管该平台」的孤儿配置。
/// 典型来源：CC Switch 等工具在代理开启期间缓存了代理地址，之后又回写 CLI 配置——
/// 此时端口上没有监听，CLI 会全部失败，必须尽快还原。
/// - 代理运行中且接管着该平台 → 配置指向代理是正常态，绝不动；
/// - 还原成功后**保留**设置里的上游（最近已知网关），同一残留再次出现时仍可修复；
/// - 由 [`startup`] 与 [`on_config_changed`]（配置文件监听回调）共同调用，常驻生效。
pub(crate) fn heal_orphans(app: &tauri::AppHandle) {
    const MIN_HEAL_INTERVAL: Duration = Duration::from_secs(10);
    // 频率限制：还原写入本身会再次触发配置监听，避免极端情况下反复写盘
    static LAST_HEAL: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));
    if let Ok(mut last) = LAST_HEAL.lock() {
        if last.is_some_and(|t| t.elapsed() < MIN_HEAL_INTERVAL) {
            return;
        }
        *last = Some(Instant::now());
    }

    let settings = app.state::<crate::Cfg>().0.get();
    let port = settings.proxy_port;
    let runtime = PROXY.lock().ok().and_then(|guard| guard.clone());
    for (platform, saved) in [
        (ProxyPlatform::Claude, settings.proxy_claude_upstream.clone()),
        (ProxyPlatform::Codex, settings.proxy_codex_upstream.clone()),
    ] {
        // 代理正接管该平台：配置指向代理是预期状态
        if runtime.as_ref().is_some_and(|rt| rt.deps.upstreams.contains_key(&platform)) {
            continue;
        }
        let Ok(Some(current)) = crate::proxy_config::read_current_base(platform) else {
            continue;
        };
        if !is_self_base(&current, port) {
            continue;
        }
        match saved.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(original) => {
                if let Err(error) = crate::proxy_config::restore_platform(platform, Some(original), port) {
                    eprintln!("[代理] 清理 {} 残留失败：{error}", platform.display_name());
                } else {
                    println!(
                        "[代理] 检测到 {} 配置指向已停止的代理（疑似外部工具回写），已自动还原直连",
                        platform.display_name()
                    );
                }
            }
            None => {
                eprintln!(
                    "[代理] {} 配置指向本机代理但无已保存上游，无法自动还原；请手动改回网关地址或重启 CC Switch",
                    platform.display_name()
                );
            }
        }
    }
}

/// 本机 CLI 配置被外部修改（CC Switch 切换等）后的跟随逻辑：
/// 先清理孤儿代理地址（无论代理是否运行，见 [`heal_orphans`]）；
/// 代理运行中：读到代理地址 → 自己写的，忽略；读到新网关地址 → 更新保存的上游并重写
/// 代理地址保持接管；读到空 → 外部切回官方直连，放弃该平台接管并清掉代理地址。
pub fn on_config_changed(app: &tauri::AppHandle) {
    heal_orphans(app);
    let Some(runtime) = PROXY.lock().expect("代理状态锁").clone() else {
        return;
    };
    let proxy_base = format!("http://127.0.0.1:{}", runtime.port);
    let mut changed = false;
    for platform in [ProxyPlatform::Claude, ProxyPlatform::Codex] {
        if !runtime.deps.upstreams.contains_key(&platform) {
            continue;
        }
        let Ok(current) = crate::proxy_config::read_current_base(platform) else {
            continue;
        };
        match current.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(value) if is_self_base(value, runtime.port) => {}
            Some(external) => {
                // 外部切换了网关：记住新上游（规范化尾斜杠），重写代理地址
                let normalized = external.trim().trim_end_matches('/').to_string();
                if let Err(error) = crate::proxy_config::write_proxy_base(platform, &proxy_base) {
                    eprintln!("[代理] 跟随外部切换失败（{}）：{error}", platform.display_name());
                    continue;
                }
                if app
                    .state::<crate::Cfg>()
                    .0
                    .try_update(|settings| set_upstream(settings, platform, Some(normalized)))
                    .is_ok()
                {
                    changed = true;
                    println!("[代理] {} 上游已跟随外部配置更新并保持接管", platform.display_name());
                }
            }
            None => {
                // 外部删除了 base_url（切回官方直连）：放弃接管该平台
                if let Err(error) = crate::proxy_config::restore_platform(platform, None, runtime.port) {
                    eprintln!("[代理] 放弃 {} 接管时清理失败：{error}", platform.display_name());
                    continue;
                }
                if app
                    .state::<crate::Cfg>()
                    .0
                    .try_update(|settings| set_upstream(settings, platform, None))
                    .is_ok()
                {
                    changed = true;
                    println!("[代理] {} 已切回官方直连，放弃接管", platform.display_name());
                }
            }
        }
    }
    if changed {
        let _ = app.emit("settings-changed", app.state::<crate::Cfg>().0.get());
    }
}

fn set_upstream(settings: &mut crate::settings::Settings, platform: ProxyPlatform, value: Option<String>) {
    match platform {
        ProxyPlatform::Claude => settings.proxy_claude_upstream = value,
        ProxyPlatform::Codex => settings.proxy_codex_upstream = value,
    }
}

/// 应用退出时的收尾（RunEvent::Exit）：代理还开着就还原 CLI 配置，避免 CLI 指向失效端口。
pub fn shutdown_and_restore(app: &tauri::AppHandle) {
    stop();
    let settings = app.state::<crate::Cfg>().0.get();
    if settings.proxy_claude_upstream.is_some() || settings.proxy_codex_upstream.is_some() {
        if let Err(error) = crate::proxy_config::restore_all_saved(app) {
            eprintln!("[代理] 退出还原失败：{error}");
        }
    }
}

/// 采集扫描完成后补写待关联的计时（在已持有数据库锁的上下文里调用）。
pub fn flush_pending(conn: &rusqlite::Connection) {
    let Ok(mut queue) = PENDING.lock() else {
        return;
    };
    if queue.is_empty() {
        return;
    }
    let _ = flush_entries(conn, &mut queue);
}

/// 纯函数版补写：命中即回填；未命中且未过期的留下，过期（TTL）的丢弃。
pub(crate) fn flush_entries(conn: &rusqlite::Connection, queue: &mut Vec<PendingTiming>) -> rusqlite::Result<()> {
    let cutoff = now_ms() - PENDING_TTL_MS;
    queue.retain(|entry| entry.created_ms >= cutoff);
    let mut remaining = Vec::new();
    std::mem::swap(queue, &mut remaining);
    for entry in remaining {
        let applied = crate::db::apply_proxy_timing(
            conn,
            &entry.dedup_key,
            entry.first_token_ms,
            entry.duration_ms,
            entry.status_code,
        )
        .unwrap_or(0);
        if applied == 0 && entry.created_ms >= cutoff {
            queue.push(entry);
        }
    }
    Ok(())
}

/// 设置页状态查询（`proxy_status` 命令）。
#[derive(serde::Serialize)]
pub struct ProxyStatus {
    pub running: bool,
    pub port: u16,
    /// 自动回退的原因；None 表示没有发生过回退（或用户已重新开启）。
    pub fallback_reason: Option<String>,
    pub last_error: Option<String>,
    pub claude_upstream: Option<String>,
    pub codex_upstream: Option<String>,
    pub claude_taken_over: bool,
    pub codex_taken_over: bool,
}

pub fn status(app: &tauri::AppHandle) -> ProxyStatus {
    let settings = app.state::<crate::Cfg>().0.get();
    let running = PROXY.lock().map(|guard| guard.is_some()).unwrap_or(false);
    ProxyStatus {
        running,
        port: settings.proxy_port,
        fallback_reason: FALLBACK_REASON.lock().ok().and_then(|slot| slot.clone()),
        last_error: LAST_ERROR.lock().ok().and_then(|slot| slot.clone()),
        claude_upstream: settings.proxy_claude_upstream.clone(),
        codex_upstream: settings.proxy_codex_upstream.clone(),
        claude_taken_over: running && settings.proxy_claude_upstream.is_some(),
        codex_taken_over: running && settings.proxy_codex_upstream.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_db() -> rusqlite::Connection {
        crate::db::open(std::path::Path::new(":memory:")).unwrap()
    }

    fn record(key: &str) -> crate::db::RequestRecord {
        crate::db::RequestRecord {
            platform: "claude".into(),
            source: crate::collector::SOURCE_LOCAL.into(),
            dedup_key: key.into(),
            session_id: None,
            ts: 1,
            model: None,
            input_tokens: Some(1),
            output_tokens: Some(1),
            cache_read_tokens: Some(0),
            cache_write_tokens: Some(0),
            total_tokens: Some(2),
            effort: None,
        }
    }

    /// 测试用的独立转发依赖：不碰系统代理，回退动作可观测。
    fn test_deps(
        db: Arc<Mutex<rusqlite::Connection>>,
        upstreams: HashMap<ProxyPlatform, String>,
        fallback: Option<Arc<AtomicU32>>,
    ) -> (Arc<ProxyDeps>, tokio::sync::watch::Receiver<bool>) {
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let deps = Arc::new(ProxyDeps {
            client,
            db,
            upstreams,
            pending: Arc::new(Mutex::new(Vec::new())),
            consecutive_failures: Arc::new(AtomicU32::new(0)),
            fallback_enabled: fallback.is_some(),
            shutdown: Arc::new(shutdown_tx),
            on_fallback: Arc::new(move |reason: &str| {
                assert!(reason.contains("上游不可达"));
                if let Some(hits) = &fallback {
                    hits.fetch_add(1, Ordering::Relaxed);
                }
            }),
        });
        (deps, shutdown_rx)
    }

    /// 在随机端口起一个使用给定 deps 的代理，返回代理地址。
    async fn spawn_proxy(deps: Arc<ProxyDeps>) -> String {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = Router::new()
            .fallback(forward)
            .layer(DefaultBodyLimit::max(REQUEST_BODY_LIMIT))
            .with_state(deps);
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        format!("http://{addr}")
    }

    /// 手写 mock 上游：首块延迟 `first_delay_ms` 发出，第二块再等 `second_delay_ms`，
    /// 发出第二块前先发 "upstream_done" 信号。零缓冲断言依赖该信号与首块的到达顺序。
    async fn spawn_chunked_upstream(
        first_delay_ms: u64,
        second_delay_ms: u64,
    ) -> (std::net::SocketAddr, tokio::sync::mpsc::UnboundedReceiver<&'static str>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<&'static str>();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 8192];
            let _ = socket.read(&mut buf).await;
            let _ = socket
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n")
                .await;
            tokio::time::sleep(Duration::from_millis(first_delay_ms)).await;
            let first = b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_stream\"}}\n\n";
            let _ = socket.write_all(format!("{:x}\r\n", first.len()).as_bytes()).await;
            let _ = socket.write_all(first).await;
            let _ = socket.write_all(b"\r\n").await;
            tokio::time::sleep(Duration::from_millis(second_delay_ms)).await;
            event_tx.send("upstream_done").ok();
            let last = b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
            let _ = socket.write_all(format!("{:x}\r\n", last.len()).as_bytes()).await;
            let _ = socket.write_all(last).await;
            let _ = socket.write_all(b"\r\n0\r\n\r\n").await;
        });
        (addr, event_rx)
    }

    /// 手写 mock 上游：固定状态行与响应体，一次性返回后断开。
    async fn spawn_static_upstream(status_line: &str, body: &str) -> std::net::SocketAddr {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let response = format!(
            "HTTP/1.1 {status_line}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 8192];
            let _ = socket.read(&mut buf).await;
            let _ = socket.write_all(response.as_bytes()).await;
        });
        addr
    }

    #[test]
    fn platform_detection_prefers_headers_then_paths() {
        let mut headers = HeaderMap::new();
        headers.insert("anthropic-version", "2023-06-01".parse().unwrap());
        assert_eq!(detect_platform(&headers, "/v1/messages"), Some(ProxyPlatform::Claude));

        let mut headers = HeaderMap::new();
        headers.insert("originator", "codex_cli_rs".parse().unwrap());
        assert_eq!(detect_platform(&headers, "/v1/responses"), Some(ProxyPlatform::Codex));

        let headers = HeaderMap::new();
        assert_eq!(detect_platform(&headers, "/v1/messages?beta=true"), Some(ProxyPlatform::Claude));
        assert_eq!(detect_platform(&headers, "/v1/chat/completions"), Some(ProxyPlatform::Codex));
        assert_eq!(detect_platform(&headers, "/backend-api/codex/responses"), Some(ProxyPlatform::Codex));
        assert_eq!(detect_platform(&headers, "/v1/unknown"), None);
    }

    #[test]
    fn dedup_key_extraction_covers_sse_and_plain_json() {
        // Claude SSE：ping 在前、message_start 带 message.id
        let claude_sse = b"event: ping\ndata: {\"type\":\"ping\"}\n\nevent: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_01ABC\",\"model\":\"claude\"}}\n\n";
        assert_eq!(extract_dedup_key(ProxyPlatform::Claude, claude_sse).as_deref(), Some("msg_01ABC"));
        // Claude 非流式
        let claude_json = br#"{"id":"msg_01XYZ","type":"message","usage":{}}"#;
        assert_eq!(extract_dedup_key(ProxyPlatform::Claude, claude_json).as_deref(), Some("msg_01XYZ"));
        // Codex SSE：response.created 带 response.id，不得误取 item.id
        let codex_sse = b"data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_AA\"}}\n\ndata: {\"type\":\"response.output_item.added\",\"item\":{\"id\":\"item_BB\"},\"response\":{\"id\":\"resp_AA\"}}\n\n";
        assert_eq!(extract_dedup_key(ProxyPlatform::Codex, codex_sse).as_deref(), Some("resp_AA"));
        // Codex 非流式
        let codex_json = br#"{"id":"resp_CC","object":"response"}"#;
        assert_eq!(extract_dedup_key(ProxyPlatform::Codex, codex_json).as_deref(), Some("resp_CC"));
        // 提取不到不猜：非 JSON、JSON 无 id、空
        assert_eq!(extract_dedup_key(ProxyPlatform::Claude, b"event: done\n"), None);
        assert_eq!(extract_dedup_key(ProxyPlatform::Claude, br#"{"type":"ping"}"#), None);
        assert_eq!(extract_dedup_key(ProxyPlatform::Codex, b""), None);
    }

    #[test]
    fn upstream_url_keeps_path_and_query_and_trims_base_slash() {
        let uri: axum::http::Uri = "/v1/messages?beta=true".parse().unwrap();
        assert_eq!(build_upstream_url("https://gw.test/", &uri), "https://gw.test/v1/messages?beta=true");
        let uri: axum::http::Uri = "/v1/responses".parse().unwrap();
        assert_eq!(build_upstream_url("https://gw.test", &uri), "https://gw.test/v1/responses");
    }

    #[test]
    fn self_base_detection_matches_only_local_proxy_port() {
        assert!(is_self_base("http://127.0.0.1:12731", 12731));
        assert!(is_self_base("http://127.0.0.1:12731/", 12731));
        assert!(!is_self_base("http://127.0.0.1:9999", 12731));
        assert!(!is_self_base("https://gw.test", 12731));
    }

    #[test]
    fn pending_entries_flush_when_scan_lands_and_expire_after_ttl() {
        let mut conn = memory_db();
        crate::db::insert_records(&mut conn, &[record("msg_late")]).unwrap();

        let mut queue = vec![
            PendingTiming {
                dedup_key: "msg_late".into(),
                first_token_ms: 120,
                duration_ms: 900,
                status_code: 200,
                created_ms: now_ms(),
            },
            PendingTiming {
                dedup_key: "msg_expired".into(),
                first_token_ms: 1,
                duration_ms: 2,
                status_code: 200,
                created_ms: now_ms() - PENDING_TTL_MS - 1,
            },
        ];
        flush_entries(&conn, &mut queue).unwrap();
        // msg_late 已入库命中，msg_expired 过期丢弃
        assert!(queue.is_empty());
        let timing: (Option<i64>, Option<i64>) = conn
            .query_row(
                "SELECT first_token_ms, duration_ms FROM requests WHERE dedup_key = 'msg_late'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(timing, (Some(120), Some(900)));
    }

    #[test]
    fn pending_entry_kept_until_record_arrives() {
        let conn = memory_db();
        let mut queue = vec![PendingTiming {
            dedup_key: "msg_wait".into(),
            first_token_ms: 50,
            duration_ms: 400,
            status_code: 200,
            created_ms: now_ms(),
        }];
        flush_entries(&conn, &mut queue).unwrap();
        assert_eq!(queue.len(), 1, "记录尚未入库时保留待关联");

        let mut conn = conn;
        crate::db::insert_records(&mut conn, &[record("msg_wait")]).unwrap();
        flush_entries(&conn, &mut queue).unwrap();
        assert!(queue.is_empty(), "扫描入库后补写成功");
        let status: Option<i64> = conn
            .query_row("SELECT status_code FROM requests WHERE dedup_key = 'msg_wait'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(status, Some(200));
    }

    /// 端到端：mock 上游分两块、间隔发送；验证零缓冲（下游首块先于上游第二块到达）
    /// 与计时数据进入待关联队列，采集入库后补写成功。
    #[tokio::test]
    async fn streaming_is_forwarded_chunk_by_chunk_and_timings_are_recorded() {
        // 首块 150ms、第二块再等 500ms；若代理缓冲整条流，客户端首块会晚于
        // upstream_done（第二块发出前）到达，零缓冲断言即失败。
        let (upstream_addr, mut event_rx) = spawn_chunked_upstream(150, 500).await;

        let db = Arc::new(Mutex::new(memory_db()));
        let (deps, _shutdown_rx) = test_deps(
            db.clone(),
            HashMap::from([(ProxyPlatform::Claude, format!("http://{upstream_addr}"))]),
            None,
        );
        let proxy_addr = spawn_proxy(deps.clone()).await;

        // 经代理发起请求（带 Anthropic 特征头，走 Claude 上游）
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let response = client
            .post(format!("{proxy_addr}/v1/messages"))
            .header("anthropic-version", "2023-06-01")
            .header("x-api-key", "test-key")
            .body("{}")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);

        // 零缓冲断言：首块到达时上游第二块（500ms 后）尚未发出
        let mut stream = response.bytes_stream();
        let first = stream.next().await.unwrap().unwrap();
        assert!(
            first.windows(b"message_start".len()).any(|w| w == b"message_start"),
            "首块应包含 message_start 事件"
        );
        assert!(event_rx.try_recv().is_err(), "首块到达时上游整条流不应已发完（缓冲会失败此断言）");
        while stream.next().await.is_some() {}

        // 计时落库：CLI 此时尚未写日志 → 记录进 pending；模拟采集入库后 flush 补写
        let queue = deps.pending.lock().unwrap();
        assert_eq!(queue.len(), 1, "响应结束时 requests 尚无该行，应进入待关联队列");
        assert_eq!(queue[0].dedup_key, "msg_stream");
        assert_eq!(queue[0].status_code, 200);
        assert!(queue[0].first_token_ms >= 0);
        assert!(queue[0].duration_ms >= queue[0].first_token_ms);

        let mut conn = db.lock().unwrap();
        crate::db::insert_records(&mut conn, &[record("msg_stream")]).unwrap();
        drop(queue);
        flush_entries(&conn, &mut deps.pending.lock().unwrap()).unwrap();
        let timing: (Option<i64>, Option<i64>, Option<i64>) = conn
            .query_row(
                "SELECT first_token_ms, duration_ms, status_code FROM requests WHERE dedup_key = 'msg_stream'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(timing.2, Some(200));
        assert!(timing.0.unwrap() >= 0 && timing.1.unwrap() >= timing.0.unwrap());
    }

    /// 上游 4xx 透传：状态码与 body 原样到达 CLI，且不算代理故障（不触发回退）。
    #[tokio::test]
    async fn upstream_error_status_and_body_pass_through_unchanged() {
        let body = "{\"error\":{\"code\":\"rate_limited\"}}";
        let addr = spawn_static_upstream("429 Too Many Requests", body).await;

        let db = Arc::new(Mutex::new(memory_db()));
        let (deps, _shutdown_rx) = test_deps(
            db,
            HashMap::from([(ProxyPlatform::Codex, format!("http://{addr}"))]),
            None,
        );
        let proxy_addr = spawn_proxy(deps).await;

        let response = reqwest::Client::builder().no_proxy().build().unwrap()
            .post(format!("{proxy_addr}/v1/responses"))
            .header("originator", "codex_cli_rs")
            .body("{}")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 429);
        let text = response.text().await.unwrap();
        assert_eq!(text, body);
    }

    /// 上游拒绝连接：代理返回明确的 502 且不 panic；连续达到阈值触发一次回退回调。
    #[tokio::test]
    async fn unreachable_upstream_yields_502_and_triggers_fallback_once() {
        let db = Arc::new(Mutex::new(memory_db()));
        let fallback_hits = Arc::new(AtomicU32::new(0));
        let (deps, mut shutdown_rx) = test_deps(
            db,
            HashMap::from([(ProxyPlatform::Claude, "http://127.0.0.1:1".into())]),
            Some(fallback_hits.clone()),
        );
        let proxy_addr = spawn_proxy(deps.clone()).await;

        let client = reqwest::Client::builder()
            .no_proxy()
            .connect_timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        for attempt in 1..=FALLBACK_THRESHOLD {
            let response = client
                .post(format!("{proxy_addr}/v1/messages"))
                .header("anthropic-version", "2023-06-01")
                .body("{}")
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_GATEWAY, "第 {attempt} 次应拿到 502");
        }
        assert_eq!(
            fallback_hits.load(Ordering::Relaxed),
            1,
            "连续第 {FALLBACK_THRESHOLD} 次不可达应触发一次回退回调"
        );
        // 回退信号已发出
        shutdown_rx.changed().await.unwrap();
        assert!(*shutdown_rx.borrow());
        // 之后再次失败不再重复触发（服务已停，计数已清零）
        deps.note_upstream_unreachable("上游不可达：again".into());
        assert_eq!(fallback_hits.load(Ordering::Relaxed), 1);
    }
}
