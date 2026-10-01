//! CC Usage —— Tauri 应用入口
//!
//! 职责边界见需求文档 §7.7：
//!   Tauri 2 + Rust —— 窗口与托盘、只读网络查询、本地增量采集、去重、
//!                     统计计算与数据库查询；集中管理必要后台任务
//!
//! 包含双窗口、托盘、本地会话采集、SQLite 统计与连接只读查询。

mod platforms;
mod provider_key;
mod source_store;
mod native_parse;
mod native_sources;
mod collector;
mod context_menu;
mod tray_summary;
mod session_presence;
mod startup;
mod tray_icon;
mod connections;
mod creds;
mod db;
mod data_files;
mod dock;
mod island_clones;
mod main_window;
mod pricing;
mod proxy;
mod proxy_config;
mod quota;
mod coding_plan;
mod grok_quota;
mod zcode_quota;
mod settings;
mod session_titles;
mod watcher;
mod validation;
mod updater;
mod cli_apply;
mod effort_map;
#[cfg(test)]
mod perf_bench;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, LazyLock, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{
    Emitter,
    image::Image,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, PhysicalPosition, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_opener::OpenerExt;

#[derive(serde::Serialize)]
struct DataInfo {
    path: String,
    size_bytes: u64,
}

#[derive(Clone, serde::Serialize)]
struct SourceInfo {
    id: String,
    platform: String,
    name: String,
    available: bool,
    capabilities: Vec<String>,
    reason: Option<String>,
}

fn local_source(platform: &str) -> Option<SourceInfo> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(std::path::PathBuf::from);
    let path = match platform {
        "claude" => home.map(|path| path.join(".claude").join("projects")),
        "codex" => session_titles::codex_home().map(|path| path.join("sessions")),
        _ => return None,
    };
    let available = path.as_ref().is_some_and(|path| path.is_dir());
    Some(SourceInfo {
        id: format!("local:{platform}"),
        platform: platform.into(),
        name: format!("本机 {} 会话记录", platform_name(platform)),
        available,
        capabilities: vec!["token_totals".into(), "request_log".into(), "realtime_delta".into()],
        reason: (!available).then(|| format!("未检测到本机 {} 会话目录", platform_name(platform))),
    })
}

#[tauri::command]
fn list_sources(platform: String) -> Vec<SourceInfo> {
    if platforms::native(&platform) {
        return native_sources::sources(&platform).into_iter().map(|s|SourceInfo {
            id:s.id, platform:s.platform,name:s.name,available:s.available,capabilities:s.capabilities,reason:s.reason,
        }).collect();
    }
    local_source(&platform).into_iter().collect()
}

/// 全局写连接。采集、导入、连接管理等写入共用同一个后台，不为每个视图重复采集（§7.7）
pub struct Db(pub Arc<Mutex<rusqlite::Connection>>);

/// 只读查询连接池：统计、日志与实时快照走这里。WAL 允许读写并发，
/// 查询不再排在采集写入后面，采集也不必等界面查询。
pub struct DbRead(pub Arc<ReadPool>);

pub struct ReadPool {
    conns: Vec<Mutex<rusqlite::Connection>>,
    next: std::sync::atomic::AtomicUsize,
}

impl ReadPool {
    /// 主面板一次刷新会并发发出多条查询，灵动岛与托盘还各有一条；三条连接足够不排队。
    const SIZE: usize = 3;

    fn open(path: &std::path::Path) -> rusqlite::Result<Self> {
        let conns = (0..Self::SIZE)
            .map(|_| db::open_reader(path).map(Mutex::new))
            .collect::<rusqlite::Result<_>>()?;
        Ok(Self { conns, next: std::sync::atomic::AtomicUsize::new(0) })
    }

    /// 优先取空闲连接；都在忙时轮转排队，绝不退回写连接。
    pub fn with<T>(&self, work: impl FnOnce(&rusqlite::Connection) -> T) -> T {
        for conn in &self.conns {
            if let Ok(guard) = conn.try_lock() {
                return work(&guard);
            }
        }
        let index = self.next.fetch_add(1, Ordering::Relaxed) % self.conns.len();
        let guard = self.conns[index].lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        work(&guard)
    }
}

/// 在阻塞线程池上借一条只读连接执行查询。
async fn with_reader<T: Send + 'static>(
    pool: Arc<ReadPool>,
    work: impl FnOnce(&rusqlite::Connection) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || pool.with(work))
        .await
        .map_err(|e| e.to_string())?
}

/// 在阻塞线程池等待数据库锁，避免扫描期间卡住窗口或异步运行时。
async fn with_database<T: Send + 'static>(
    db: Arc<Mutex<rusqlite::Connection>>,
    work: impl FnOnce(&mut rusqlite::Connection) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut conn = db.lock().map_err(|e| e.to_string())?;
        work(&mut conn)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod responsiveness_tests {
    use super::*;
    use std::{future::Future, sync::mpsc, task::{Context, Poll, Wake, Waker}, time::{Duration, Instant}};

    struct NoopWake;
    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    #[test]
    fn database_contention_yields_without_blocking_the_caller() {
        let db = Arc::new(Mutex::new(rusqlite::Connection::open_in_memory().unwrap()));
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let busy_db = db.clone();
        let collector = std::thread::spawn(move || {
            let _guard = busy_db.lock().unwrap();
            ready_tx.send(()).unwrap();
            // 即使回归为同步等待，也会超时释放锁，让测试失败而非永久挂起。
            let _ = release_rx.recv_timeout(Duration::from_secs(2));
        });
        ready_rx.recv().unwrap();

        let caller = std::thread::current().id();
        let mut query = Box::pin(with_database(db, move |conn| {
            assert_ne!(std::thread::current().id(), caller);
            conn.query_row("SELECT 42", [], |row| row.get::<_, i64>(0))
                .map_err(|e| e.to_string())
        }));
        let started = Instant::now();
        let waker = Waker::from(Arc::new(NoopWake));
        let first_poll = query.as_mut().poll(&mut Context::from_waker(&waker));
        let elapsed = started.elapsed();
        let _ = release_tx.send(());
        collector.join().unwrap();

        assert!(matches!(first_poll, Poll::Pending));
        assert!(elapsed < Duration::from_millis(250), "查询阻塞调用线程: {elapsed:?}");
        assert_eq!(tauri::async_runtime::block_on(query).unwrap(), 42);
    }

    #[test]
    fn read_queries_do_not_wait_for_the_collector_write_lock() {
        let dir = std::env::temp_dir().join(format!("cc-usage-read-pool-{}", std::process::id()));
        let path = dir.join("usage.db");
        let writer = Arc::new(Mutex::new(db::open(&path).unwrap()));
        let pool = ReadPool::open(&path).unwrap();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let busy = writer.clone();
        let collector = std::thread::spawn(move || {
            let _guard = busy.lock().unwrap();
            ready_tx.send(()).unwrap();
            let _ = release_rx.recv_timeout(Duration::from_secs(2));
        });
        ready_rx.recv().unwrap();
        let started = Instant::now();
        let count: i64 = pool.with(|conn| conn.query_row("SELECT COUNT(*) FROM requests", [], |row| row.get(0))).unwrap();
        let elapsed = started.elapsed();
        let _ = release_tx.send(());
        collector.join().unwrap();
        assert_eq!(count, 0);
        assert!(elapsed < Duration::from_millis(250), "只读查询被写锁阻塞: {elapsed:?}");
        assert!(pool.with(|conn| conn.execute("DELETE FROM requests", [])).is_err(), "只读连接不得写入");
        drop(pool);
        drop(writer);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reversed_custom_ranges_are_rejected_before_database_work() {
        let range = db::CustomRange { start: 2_000, end: Some(1_000) };
        assert_eq!(
            validate_query_range(Some(range), 3_000).unwrap_err(),
            "开始时间必须早于结束时间",
        );
        assert!(validate_query_range(Some(db::CustomRange { start: 1_000, end: None }), 2_000).is_ok());
    }

    #[test]
    fn query_cache_deduplicates_concurrent_requests_and_allows_forced_success_refresh() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let cache = Arc::new(QueryCache::<usize>::default());
        let calls = Arc::new(AtomicUsize::new(0));
        let mut workers = Vec::new();
        for _ in 0..4 {
            let cache = cache.clone();
            let calls = calls.clone();
            workers.push(std::thread::spawn(move || cached_query(
                &cache,
                "same".into(),
                false,
                |_| CacheOutcome::Success,
                || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(30));
                    7
                },
            )));
        }
        assert!(workers.into_iter().all(|worker| worker.join().unwrap() == 7));
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        assert_eq!(cached_query(
            &cache,
            "same".into(),
            true,
            |_| CacheOutcome::Success,
            || { calls.fetch_add(1, Ordering::SeqCst); 8 },
        ), 8);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn periodic_refresh_reuses_a_recent_fetch_but_not_an_older_one() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let cache = QueryCache::<usize>::default();
        let calls = AtomicUsize::new(0);
        let fetch = || { calls.fetch_add(1, Ordering::SeqCst); 1 };
        cached_query(&cache, "k".into(), false, |_| CacheOutcome::Success, fetch);
        // 另一个窗口的定时刷新落在同一周期内：复用刚取得的结果
        cached_query(&cache, "k".into(), refresh_mode(false, Some(60_000)), |_| CacheOutcome::Success, fetch);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // 缓存已比刷新间隔旧：即使尚未过期也重新查询
        std::thread::sleep(Duration::from_millis(20));
        cached_query(&cache, "k".into(), refresh_mode(false, Some(10)), |_| CacheOutcome::Success, fetch);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(refresh_mode(true, Some(10)), Refresh::Force);
    }

    #[test]
    fn forced_refresh_respects_rate_limit_backoff() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let cache = QueryCache::<usize>::default();
        let calls = AtomicUsize::new(0);
        let first = cached_query(
            &cache,
            "limited".into(),
            false,
            |_| CacheOutcome::RateLimited(60),
            || { calls.fetch_add(1, Ordering::SeqCst); 429 },
        );
        let forced = cached_query(
            &cache,
            "limited".into(),
            true,
            |_| CacheOutcome::Success,
            || { calls.fetch_add(1, Ordering::SeqCst); 200 },
        );
        assert_eq!((first, forced), (429, 429));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn single_scan_failure_does_not_mark_collection_broken_but_persistent_does() {
        let errors = vec!["rollout-x.jsonl: boom".to_string()];
        // 首次失败：可能是写入竞争的瞬时错误，不置异常
        assert_eq!(evaluate_collection_status(0, &errors), (true, 1));
        // 连续第二次失败：确认采集异常
        assert_eq!(evaluate_collection_status(1, &errors), (false, 2));
        // 期间任一轮成功：计数清零、恢复正常
        assert_eq!(evaluate_collection_status(1, &[]), (true, 0));
        // 长期失败保持异常，计数继续累计供诊断
        assert_eq!(evaluate_collection_status(5, &errors), (false, 6));
    }
}

/// 持久化设置。托盘与设置界面共享同一份值（§2.6 / §7.3）
pub struct Cfg(pub settings::Store);
static TRAY_REFRESHING: AtomicBool = AtomicBool::new(false);
#[derive(Clone, serde::Serialize)]
pub(crate) struct CollectionStatus {
    ok: bool,
    errors: Vec<String>,
    last_success_ms: Option<i64>,
    /// 本轮扫描中出错文件所属的平台。采集异常按平台定界：
    /// 只有出错平台的会话状态不可判定，其他平台照常显示真实状态。
    failed_sources: Vec<String>,
    /// 连续失败轮数。单次瞬时错误（如 CLI 正在写入时的读取竞争）在下一轮心跳
    /// 就会自愈，不足以把会话打成「状态未知」；连续达到阈值才认定采集异常。
    consecutive_failures: u32,
}

/// 连续失败达到该轮数才置「采集异常」。心跳 10 秒一轮，即约 20 秒的确认窗口：
/// 真·持续故障很快上报，单次抖动不再污染所有会话的状态展示。
const COLLECTION_FAILURE_THRESHOLD: u32 = 2;

/// 纯函数：输入上一轮的连续失败数与本轮错误，输出 (是否异常, 新的连续失败数)。
fn evaluate_collection_status(consecutive: u32, errors: &[String]) -> (bool, u32) {
    if errors.is_empty() {
        return (true, 0);
    }
    let next = consecutive + 1;
    (next < COLLECTION_FAILURE_THRESHOLD, next)
}
static COLLECTION_STATUS: LazyLock<Mutex<CollectionStatus>> = LazyLock::new(|| Mutex::new(CollectionStatus {
    ok: true,
    errors: Vec::new(),
    last_success_ms: None,
    failed_sources: Vec::new(),
    consecutive_failures: 0,
}));

#[derive(Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum MainIntent {
    Overview { settings: settings::Settings },
    Settings { section: String },
    About { version: String },
}

static MAIN_INTENT: LazyLock<Mutex<Option<MainIntent>>> = LazyLock::new(|| Mutex::new(None));

struct QueryCacheState<T> {
    /// 值、过期时刻、是否处于退避期、取得时刻
    entries: HashMap<String, (T, Instant, bool, Instant)>,
    in_flight: HashSet<String>,
    failures: HashMap<String, u32>,
    generations: HashMap<String, u64>,
}

impl<T> Default for QueryCacheState<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            in_flight: HashSet::new(),
            failures: HashMap::new(),
            generations: HashMap::new(),
        }
    }
}

struct QueryCache<T> {
    state: Mutex<QueryCacheState<T>>,
    ready: Condvar,
}

impl<T> Default for QueryCache<T> {
    fn default() -> Self {
        Self { state: Mutex::new(QueryCacheState::default()), ready: Condvar::new() }
    }
}

#[derive(Clone, Copy)]
enum CacheOutcome { Success, RateLimited(u64), Failed, StableFailure }

/// 调用方对缓存新鲜度的要求。
#[derive(Clone, Copy, Debug, PartialEq)]
enum Refresh {
    /// 有效缓存即可
    Cached,
    /// 用户显式刷新：跳过成功缓存
    Force,
    /// 定时刷新：缓存取得不超过这么久就复用。多个窗口各自按刷新间隔轮询同一连接，
    /// 同一周期内只打一次上游，又不会因缓存把间隔拖长。
    MaxAge(Duration),
}

impl From<bool> for Refresh {
    fn from(force: bool) -> Self {
        if force { Refresh::Force } else { Refresh::Cached }
    }
}

/// 命令参数到刷新模式：显式 force 优先，其次是定时刷新给出的最大缓存年龄。
fn refresh_mode(force: bool, max_age_ms: Option<u64>) -> Refresh {
    match (force, max_age_ms) {
        (true, _) => Refresh::Force,
        (false, Some(ms)) => Refresh::MaxAge(Duration::from_millis(ms)),
        (false, None) => Refresh::Cached,
    }
}

fn cached_query<T: Clone>(
    cache: &QueryCache<T>,
    key: String,
    refresh: impl Into<Refresh>,
    classify: impl Fn(&T) -> CacheOutcome,
    query: impl FnOnce() -> T,
) -> T {
    let refresh = refresh.into();
    let mut waited = false;
    let generation;
    loop {
        let Ok(mut state) = cache.state.lock() else { return query() };
        if let Some((value, expires, backoff, fetched)) = state.entries.get(&key) {
            let wants_fresh = match refresh {
                Refresh::Cached => false,
                Refresh::Force => true,
                Refresh::MaxAge(age) => fetched.elapsed() >= age,
            };
            // 显式刷新可以跳过成功缓存；限流和网络失败的退避期仍必须遵守。
            if waited || (*expires > Instant::now() && (!wants_fresh || *backoff)) {
                return value.clone();
            }
        }
        if state.in_flight.insert(key.clone()) {
            generation = state.generations.get(&key).copied().unwrap_or(0);
            break;
        }
        let Ok(next) = cache.ready.wait(state) else { return query() };
        drop(next);
        waited = true;
    }

    let value = query();
    if let Ok(mut state) = cache.state.lock() {
        state.in_flight.remove(&key);
        let outcome = classify(&value);
        let failures = match outcome {
            CacheOutcome::Failed => {
                let next = state.failures.get(&key).copied().unwrap_or(0).saturating_add(1);
                state.failures.insert(key.clone(), next);
                next
            }
            _ => {
                state.failures.remove(&key);
                0
            }
        };
        let ttl = match outcome {
            CacheOutcome::Success => Duration::from_secs(5 * 60),
            CacheOutcome::RateLimited(seconds) => Duration::from_secs(seconds.clamp(1, 30 * 60)),
            CacheOutcome::Failed => Duration::from_secs((15u64.saturating_mul(1u64 << failures.min(4))).min(5 * 60)),
            CacheOutcome::StableFailure => Duration::from_secs(60),
        };
        // 凭证在查询期间被替换时，不把旧凭证的响应重新放回缓存。
        if state.generations.get(&key).copied().unwrap_or(0) == generation {
            let backoff = matches!(outcome, CacheOutcome::RateLimited(_) | CacheOutcome::Failed);
            let now = Instant::now();
            state.entries.insert(key, (value.clone(), now + ttl, backoff, now));
        }
        cache.ready.notify_all();
    }
    value
}

fn retry_after_seconds(reason: &str) -> u64 {
    reason.split("请等待 ").nth(1)
        .and_then(|tail| tail.split(' ').next())
        .and_then(|value| value.parse().ok())
        .unwrap_or(60)
}

fn quota_cache_outcome(value: &quota::QuotaState) -> CacheOutcome {
    match value {
        quota::QuotaState::Ok { .. } => CacheOutcome::Success,
        quota::QuotaState::RateLimited { reason } => CacheOutcome::RateLimited(retry_after_seconds(reason)),
        quota::QuotaState::Failed { .. } => CacheOutcome::Failed,
        _ => CacheOutcome::StableFailure,
    }
}

fn balance_cache_outcome(value: &quota::BalanceState) -> CacheOutcome {
    match value {
        quota::BalanceState::Ok { .. } => CacheOutcome::Success,
        quota::BalanceState::RateLimited { reason } => CacheOutcome::RateLimited(retry_after_seconds(reason)),
        quota::BalanceState::Failed { .. } => CacheOutcome::Failed,
        _ => CacheOutcome::StableFailure,
    }
}

fn api_usage_cache_outcome(value: &quota::ApiUsageState) -> CacheOutcome {
    match value {
        quota::ApiUsageState::Ok { .. } => CacheOutcome::Success,
        quota::ApiUsageState::RateLimited { reason } => CacheOutcome::RateLimited(retry_after_seconds(reason)),
        quota::ApiUsageState::Failed { .. } => CacheOutcome::Failed,
        _ => CacheOutcome::StableFailure,
    }
}

static QUOTA_CACHE: LazyLock<QueryCache<quota::QuotaState>> = LazyLock::new(QueryCache::default);
static BALANCE_CACHE: LazyLock<QueryCache<quota::BalanceState>> = LazyLock::new(QueryCache::default);
static API_USAGE_CACHE: LazyLock<QueryCache<quota::ApiUsageState>> = LazyLock::new(QueryCache::default);

fn invalidate_network_cache(id: &str) {
    fn invalidate<T>(cache: &QueryCache<T>, id: &str) {
        if let Ok(mut state) = cache.state.lock() {
            state.entries.remove(id);
            state.failures.remove(id);
            let next = state.generations.get(id).copied().unwrap_or(0).wrapping_add(1);
            state.generations.insert(id.to_string(), next);
        }
    }
    invalidate(&QUOTA_CACHE, id);
    invalidate(&BALANCE_CACHE, id);
    invalidate(&API_USAGE_CACHE, id);
}

#[tauri::command]
fn take_main_intent() -> Option<MainIntent> {
    MAIN_INTENT.lock().ok().and_then(|mut intent| intent.take())
}

pub(crate) fn emit_collection_status(app: &tauri::AppHandle, result: &collector::ScanResult) {
    let status = if let Ok(mut current) = COLLECTION_STATUS.lock() {
        let (ok, consecutive) = evaluate_collection_status(current.consecutive_failures, &result.errors);
        if !ok {
            eprintln!("[采集] 连续 {} 轮扫描出错：{}", consecutive, result.errors.join("；"));
        }
        current.ok = ok;
        current.consecutive_failures = consecutive;
        current.errors = result.errors.clone();
        // 只在确认异常时定界到出错平台；正常轮次清空，避免历史残留影响展示
        current.failed_sources = if ok { Vec::new() } else { result.failed_sources.clone() };
        if current.ok {
            current.last_success_ms = Some(chrono::Local::now().timestamp_millis());
        }
        current.clone()
    } else {
        CollectionStatus { ok: false, errors: vec!["采集状态锁不可用".into()], last_success_ms: None, failed_sources: Vec::new(), consecutive_failures: 0 }
    };
    let _ = app.emit("collection-status", status);
}

#[tauri::command]
fn collection_status() -> CollectionStatus {
    COLLECTION_STATUS.lock().map(|status| status.clone()).unwrap_or(CollectionStatus {
        ok: false,
        errors: vec!["采集状态锁不可用".into()],
        last_success_ms: None,
        failed_sources: Vec::new(),
        consecutive_failures: 0,
    })
}

#[tauri::command]
async fn local_codex_quota(app: tauri::AppHandle, force: bool, max_age_ms: Option<u64>) -> Result<quota::QuotaState, String> {
    let force = refresh_mode(force, max_age_ms);
    let state = tauri::async_runtime::spawn_blocking(move || cached_query(
        &QUOTA_CACHE,
        "local-codex".into(),
        force,
        quota_cache_outcome,
        quota::local_codex_quota,
    ))
        .await.map_err(|e| e.to_string())?;
    let settings = app.state::<Cfg>().0.get();
    if settings.island_platform == "codex"
        && settings.island_kind == "auth"
        && settings.island_connection_id.is_none()
    {
        update_tray_quota_icon(&app, &state);
    }
    Ok(state)
}

/// 查本机 Grok 订阅额度（~/.grok/auth.json 的 OAuth 凭证）。
/// 灵动岛托盘图标逻辑目前只覆盖 claude/codex，Grok 额度仅在主面板展示。
#[tauri::command]
async fn local_grok_quota(force: bool, max_age_ms: Option<u64>) -> Result<quota::QuotaState, String> {
    let force = refresh_mode(force, max_age_ms);
    tauri::async_runtime::spawn_blocking(move || cached_query(
        &QUOTA_CACHE,
        "local-grok".into(),
        force,
        quota_cache_outcome,
        grok_quota::local_grok_quota,
    ))
        .await.map_err(|e| e.to_string())
}

/// 查询本机 ZCode BigModel Coding Plan API Key 的套餐额度。
#[tauri::command]
async fn local_zcode_quota(force: bool, max_age_ms: Option<u64>) -> Result<quota::QuotaState, String> {
    let force = refresh_mode(force, max_age_ms);
    tauri::async_runtime::spawn_blocking(move || cached_query(
        &QUOTA_CACHE,
        "local-zcode".into(),
        force,
        quota_cache_outcome,
        zcode_quota::local_zcode_quota,
    ))
        .await.map_err(|error| error.to_string())
}

/// 套餐查询辅助凭证的掩码视图（智谱团队组织/项目 ID、火山 AK/SK）。
#[tauri::command]
fn get_plan_query_status(store: State<'_, settings::PlanQueryStore>) -> settings::PlanQueryStatus {
    store.status()
}

/// 保存套餐查询辅助凭证。`volc_secret_access_key` 为 None 表示保持不变
/// （前端编辑其他字段时不必重填 Secret），Some 空串表示清除。
#[tauri::command]
fn set_plan_query(
    store: State<'_, settings::PlanQueryStore>,
    zhipu_team_organization_id: String,
    zhipu_team_project_id: String,
    volc_access_key_id: String,
    volc_secret_access_key: Option<String>,
) -> Result<settings::PlanQueryStatus, String> {
    store.update(zhipu_team_organization_id, zhipu_team_project_id, volc_access_key_id, volc_secret_access_key)?;
    Ok(store.status())
}

/* --------------------------- Tauri 命令 --------------------------- */

#[tauri::command]
async fn scan_local_sessions(app: tauri::AppHandle, db: State<'_, Db>) -> Result<collector::ScanResult, String> {
    if validation::isolated() { return Err("隔离验收不读取用户日志".into()); }
    let result = with_database(db.0.clone(), move |conn| {
        Ok(collector::scan(conn))
    })
    .await?;
    emit_collection_status(&app, &result);
    Ok(result)
}

/// 调用方窗口必须是灵动岛（本体或分身），返回其分身 id（本体为 None）。
fn island_id_of(window: &WebviewWindow) -> Result<Option<String>, String> {
    island_clones::id_from_label(window.label()).ok_or_else(|| "只有灵动岛窗口能执行此操作".to_string())
}

fn island_dock(settings: &settings::Settings, id: Option<&str>) -> dock::DockState {
    island_clones::profile(settings, id).map(|profile| profile.dock).unwrap_or_default()
}

fn island_window(app: &tauri::AppHandle, id: Option<&str>) -> Option<WebviewWindow> {
    app.get_webview_window(&island_clones::label(id))
}

/// 当前存活的全部灵动岛窗口（本体 + 分身）。
fn island_windows(app: &tauri::AppHandle) -> Vec<WebviewWindow> {
    app.webview_windows().into_iter()
        .filter(|(label, _)| island_clones::is_island_label(label))
        .map(|(_, window)| window)
        .collect()
}

/// 本体在前、分身在后的全部岛 id。
fn island_ids(settings: &settings::Settings) -> Vec<Option<String>> {
    std::iter::once(None).chain(settings.island_clones.iter().map(|clone| Some(clone.id.clone()))).collect()
}

/// 停靠事件带上岛 id；各岛窗口只处理自己的，不会被别的岛的停靠变化重置形态。
#[derive(Clone, serde::Serialize)]
struct DockChangedEvent { island: Option<String>, dock: dock::DockState }

#[derive(Clone, serde::Serialize)]
struct DockHintEvent { island: Option<String>, hint: Option<dock::SnapHint> }

fn emit_dock_changed(app: &tauri::AppHandle, id: Option<&str>, dock: &dock::DockState) {
    let _ = app.emit("dock-changed", DockChangedEvent { island: id.map(str::to_string), dock: dock.clone() });
}

fn store_island_dock(app: &tauri::AppHandle, id: Option<&str>, dock: dock::DockState) -> settings::Settings {
    app.state::<Cfg>().0.update(|settings| { island_clones::set_dock(settings, id, dock); })
}

/// 分身的自由态位置只在拖动结束、创建与重置时记录，不在每次移动回调里落盘。
fn remember_clone_position(app: &tauri::AppHandle, id: &str, window: &WebviewWindow) {
    let Ok(position) = window.outer_position() else { return };
    let store = &app.state::<Cfg>().0;
    let changed = store.get().island_clones.iter()
        .any(|clone| clone.id == id && clone.position != Some((position.x, position.y)));
    if changed {
        store.update(|settings| { island_clones::set_position(settings, id, (position.x, position.y)); });
    }
}

/// 灵动岛窗口尺寸跟随内容。收缩 / 展开 / 停靠三态高度差很大（展开态有会话列表），
/// 写死会把内容截断——由前端测量实际内容后调用本命令。调用方窗口即目标岛。
#[tauri::command]
fn resize_island(app: tauri::AppHandle, window: WebviewWindow, width: f64, height: f64) -> Result<Option<f64>, String> {
    let id = island_id_of(&window)?;
    let w = window;
    if !width.is_finite() || !height.is_finite() || width < 8.0 || height < 8.0 {
        return Err("灵动岛尺寸无效".into());
    }
    // 用户主动展开本体后，不再按首次安装计时强制收成停靠条。
    if id.is_none() && height > 150.0 { FIRST_INSTALL_INTRO.store(false, Ordering::Release); }
    let cfg = app.state::<Cfg>().0.get();
    // 改尺寸后重新确认置顶：对应设置里默认开启的「灵动岛置顶」
    let _ = w.set_always_on_top(cfg.always_on_top);
    let dock_state = island_dock(&cfg, id.as_deref());
    if let Some(edge) = dock_state.edge {
        dock::anchor(&w, edge, dock_state.offset, width, height)?;
        // 把停靠窗口的实际工作区上限交给前端，避免动画等待不可达高度。
        return Ok(dock::work_area(&w).map(|area| area.3));
    } else {
        dock::resize_free(&w, width, height)?;
        if id.is_none() && FIRST_INSTALL_INTRO.load(Ordering::Acquire) {
            dock::place_free_top_center(&w)?;
        }
    }
    Ok(None)
}

#[tauri::command]
async fn token_totals(
    db: State<'_, DbRead>,
    platform: String,
    custom: Option<db::CustomRange>,
    model: Option<String>,
    query_end_ms: i64,
) -> Result<db::PeriodTotals, String> {
    validate_query_range(custom, query_end_ms)?;
    with_reader(db.0.clone(), move |conn| {
        db::token_totals_at(conn, &platform, custom, model.as_deref(), query_end_ms)
            .map_err(|e| e.to_string())
    })
    .await
}

/// 区间内的 Token 分项（新增输入 / 输出 / 缓存写入 / 缓存命中）、请求数与命中率。
/// 未知字段保持 null 交给界面显示「—」，不在这里补零。
#[tauri::command]
async fn source_metrics(db: State<'_, DbRead>, platform:String, period:String,
    custom:Option<db::CustomRange>, model:Option<String>, query_end_ms:i64) -> Result<source_store::Metrics,String> {
    validate_query_range(custom,query_end_ms)?;
    if !platforms::known(&platform) {return Err("未知平台".into());}
    with_reader(db.0.clone(),move |conn| {
        let (start,end)=db::resolve_range_at(conn,&platform,&period,custom,query_end_ms);
        source_store::metrics(conn,&platform,start,end,model.as_deref()).map_err(|e|e.to_string())
    }).await
}

#[tauri::command]
async fn usage_breakdown(
    db: State<'_, DbRead>,
    platform: String,
    period: String,
    custom: Option<db::CustomRange>,
    model: Option<String>,
    query_end_ms: i64,
) -> Result<db::UsageBreakdown, String> {
    validate_query_range(custom, query_end_ms)?;
    with_reader(db.0.clone(), move |conn| {
        db::usage_breakdown_at(conn, &platform, &period, custom, model.as_deref(), query_end_ms)
            .map_err(|e| e.to_string())
    })
    .await
}

/// 当前范围内出现过的模型名，供筛选下拉使用
#[tauri::command]
async fn list_models(
    db: State<'_, DbRead>,
    platform: String,
    period: String,
    custom: Option<db::CustomRange>,
    query_end_ms: i64,
) -> Result<Vec<String>, String> {
    validate_query_range(custom, query_end_ms)?;
    with_reader(db.0.clone(), move |conn| {
        db::list_models_at(conn, &platform, &period, custom, query_end_ms).map_err(|e| e.to_string())
    })
    .await
}

/// 本机为该平台配置的接入方式：绿色 Auth / 蓝色 API（§6.3）
/// 只返回方式与来源说明，绝不返回凭证内容
#[tauri::command]
fn connection_kind(platform: String) -> creds::ConnectionInfo {
    creds::detect(&platform)
}

/// 前端日志通道：webview 没有可用的控制台时，把错误送到终端
#[tauri::command]
fn log_front(msg: String) {
    println!("[前端] {msg}");
}

/// 灵动岛实时用量：自 `since` 游标以来的新增 Token、涉及的会话、最近活跃会话
#[tauri::command]
async fn live_usage(
    db: State<'_, DbRead>,
    platform: String,
    since: Option<i64>,
) -> Result<db::LiveUsage, String> {
    with_reader(db.0.clone(), move |conn| {
        let r = session_presence::live_usage(conn, &platform, since).map_err(|e| e.to_string())?;
        if since.is_none() || r.delta_tokens > 0 {
            println!(
                "[实时] since={:?} 新增={} token, 运行中={} 个会话, 今日={:?}",
                since, r.delta_tokens, r.active_sessions.len(), r.today_tokens
            );
        }
        Ok(r)
    })
    .await
}

#[tauri::command]
async fn usage_trend(
    db: State<'_, DbRead>,
    platform: String,
    period: String,
    custom: Option<db::CustomRange>,
    model: Option<String>,
    query_end_ms: i64,
) -> Result<db::Trend, String> {
    validate_query_range(custom, query_end_ms)?;
    with_reader(db.0.clone(), move |conn| {
        db::trend_at(conn, &platform, &period, custom, model.as_deref(), query_end_ms)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
async fn request_log(
    db: State<'_, DbRead>,
    platform: String,
    period: String,
    page: u32,
    custom: Option<db::CustomRange>,
    model: Option<String>,
    query_end_ms: i64,
) -> Result<db::LogPage, String> {
    validate_query_range(custom, query_end_ms)?;
    with_reader(db.0.clone(), move |conn| {
        db::request_log_at(conn, &platform, &period, page, custom, model.as_deref(), query_end_ms)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
fn island_drag(app: tauri::AppHandle, window: WebviewWindow) -> Result<(), String> {
    let id = island_id_of(&window)?;
    if id.is_none() { FIRST_INSTALL_INTRO.store(false, Ordering::Release); }
    let w = window;
    dock::DRAGGING.store(true, std::sync::atomic::Ordering::Relaxed);
    let result = dock::drag(&w);
    dock::DRAGGING.store(false, std::sync::atomic::Ordering::Relaxed);
    let _ = app.emit("dock-hint", DockHintEvent { island: id.clone(), hint: None });
    result?;
    let snapped = dock_release_for(&app, &w, id.as_deref());
    if let (Some(id), None) = (id.as_deref(), snapped) { remember_clone_position(&app, id, &w); }
    let _ = app.emit("settings-changed", app.state::<Cfg>().0.get());
    Ok(())
}

/// 置顶是全局开关：对本体与全部分身同时生效。
#[tauri::command]
fn island_topmost(app: tauri::AppHandle, on: bool) -> Result<settings::Settings, String> {
    let windows = island_windows(&app);
    if windows.is_empty() { return Err("找不到灵动岛".into()); }
    let previous = app.state::<Cfg>().0.get().always_on_top;
    for w in &windows { w.set_always_on_top(on).map_err(|e| e.to_string())?; }
    let next = match app.state::<Cfg>().0.try_update(|s| s.always_on_top = on) {
        Ok(next) => next,
        Err(error) => { for w in &windows { let _ = w.set_always_on_top(previous); } return Err(error); }
    };
    let _ = app.emit("settings-changed", &next);
    Ok(next)
}

/// 右键菜单「显示位置」：`island` 为目标岛（None 为本体）。
#[tauri::command]
fn island_position(app: tauri::AppHandle, edge: Option<dock::Edge>, island: Option<String>) {
    let id = island.as_deref();
    if id.is_none() { FIRST_INSTALL_INTRO.store(false, Ordering::Release); }
    let Some(w) = island_window(&app, id) else { return };
    if let Some(edge) = edge {
        let Some(offset) = dock::centered_offset(&w, edge) else { return };
        dock::apply_dock(&w, edge, offset);
        // 窗口 API 会跨线程等待主事件循环；禁止在设置锁内调用，移动回调也会读取设置。
        let monitor = w.current_monitor().ok().flatten().and_then(|m| m.name().cloned());
        store_island_dock(&app, id, dock::DockState { edge: Some(edge), offset, monitor });
    } else {
        dock_undock_for(&app, &w, id);
    }
    let settings = app.state::<Cfg>().0.get();
    let _ = app.emit("settings-changed", &settings);
    emit_dock_changed(&app, id, &island_dock(&settings, id));
}

fn data_paths(app: &tauri::AppHandle) -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    let directory = match validation::data_dir() {
        Some(path) => path,
        None => app.path().app_data_dir().map_err(|error| error.to_string())?,
    };
    Ok((directory.clone(), directory.join("usage.db")))
}

#[tauri::command]
fn data_info(app: tauri::AppHandle) -> Result<DataInfo, String> {
    let (_, database) = data_paths(&app)?;
    let size_bytes = [database.clone(), database.with_extension("db-wal"), database.with_extension("db-shm")]
        .into_iter()
        .filter_map(|path| std::fs::metadata(path).ok().map(|metadata| metadata.len()))
        .sum();
    Ok(DataInfo {
        path: database.to_string_lossy().into_owned(),
        size_bytes,
    })
}

#[tauri::command]
fn open_data_directory(app: tauri::AppHandle) -> Result<(), String> {
    let (directory, _) = data_paths(&app)?;
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    app.opener()
        .open_path(directory.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|error| error.to_string())
}

const AUTOSTART_VALUE_NAME: &str = "CCUsage";
const AUTOSTART_REGISTRY_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

#[cfg(target_os = "windows")]
fn windows_autostart_status() -> Result<bool, String> {
    let status = windows_command("reg.exe")
        .args(["QUERY", AUTOSTART_REGISTRY_KEY, "/v", AUTOSTART_VALUE_NAME])
        .status()
        .map_err(|error| format!("无法读取开机启动状态：{error}"))?;
    Ok(status.success())
}

#[cfg(target_os = "windows")]
fn windows_command(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;
    let mut command = std::process::Command::new(program);
    command.creation_flags(0x0800_0000);
    // 注册项不存在是“未启用”的正常状态；不要让 reg.exe 的本地代码页错误文本污染日志。
    command.stdout(Stdio::null()).stderr(Stdio::null());
    command
}

#[cfg(not(target_os = "windows"))]
fn windows_autostart_status() -> Result<bool, String> {
    Err("当前系统暂不支持开机启动配置".into())
}

#[tauri::command]
fn autostart_status() -> Result<bool, String> {
    windows_autostart_status()
}

#[tauri::command]
fn set_autostart(on: bool) -> Result<bool, String> {
    if validation::isolated() { return Err("隔离验收不修改开机启动".into()); }
    #[cfg(target_os = "windows")]
    {
        let status = if on {
            let executable = std::env::current_exe()
                .map_err(|error| format!("无法获取程序路径：{error}"))?;
            let command = startup::command(&executable);
            windows_command("reg.exe")
                .args([
                    "ADD",
                    AUTOSTART_REGISTRY_KEY,
                    "/v",
                    AUTOSTART_VALUE_NAME,
                    "/t",
                    "REG_SZ",
                    "/d",
                    &command,
                    "/f",
                ])
                .status()
        } else {
            windows_command("reg.exe")
                .args(["DELETE", AUTOSTART_REGISTRY_KEY, "/v", AUTOSTART_VALUE_NAME, "/f"])
                .status()
        }
        .map_err(|error| format!("无法更新开机启动：{error}"))?;
        if !status.success() && (on || windows_autostart_status()?) {
            return Err("系统拒绝更新开机启动注册项".into());
        }
        return windows_autostart_status();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = on;
        Err("当前系统暂不支持开机启动配置".into())
    }
}

#[tauri::command]
fn set_dock_enabled(app: tauri::AppHandle, on: bool) -> Result<settings::Settings, String> {
    if !on { FIRST_INSTALL_INTRO.store(false, Ordering::Release); }
    let next = app.state::<Cfg>().0.try_update(|settings| {
        settings.dock_enabled = on;
        if !on {
            settings.dock = Default::default();
            for clone in &mut settings.island_clones { clone.dock = Default::default(); }
        }
    })?;
    if !on {
        for window in island_windows(&app) { dock::undock(&window); }
    }
    for id in island_ids(&next) { emit_dock_changed(&app, id.as_deref(), &island_dock(&next, id.as_deref())); }
    let _ = app.emit("settings-changed", next.clone());
    Ok(next)
}

#[tauri::command]
fn set_silent_startup(app: tauri::AppHandle, on: bool) -> Result<settings::Settings, String> {
    // 已启用的旧版注册项也在用户保存选择时补齐启动标记。
    if !validation::isolated() && windows_autostart_status()? {
        set_autostart(true)?;
    }
    let next = app.state::<Cfg>().0.try_update(|settings| settings.silent_startup = on)?;
    let _ = app.emit("settings-changed", &next);
    Ok(next)
}

#[tauri::command]
fn set_theme(app: tauri::AppHandle, theme: String) -> Result<settings::Settings, String> {
    if !matches!(theme.as_str(), "light" | "dark" | "system") {
        return Err("主题必须是 light、dark 或 system".into());
    }
    let next = app.state::<Cfg>().0.try_update(|settings| settings.theme = theme)?;
    let _ = app.emit("settings-changed", next.clone());
    Ok(next)
}

#[tauri::command]
fn set_display_preferences(app: tauri::AppHandle, opacity: u8, scale: u16, shrink_scale: u16, refresh_minutes: u32) -> Result<settings::Settings, String> {
    settings::validate_display_preferences(opacity, scale, shrink_scale, refresh_minutes)?;
    let next = app.state::<Cfg>().0.try_update(|settings| {
        settings.island_opacity = opacity;
        settings.island_scale = scale;
        settings.island_shrink_scale = shrink_scale;
        settings.refresh_minutes = refresh_minutes;
    })?;
    let _ = app.emit("settings-changed", next.clone());
    Ok(next)
}

#[tauri::command]
fn set_balance_alert(app: tauri::AppHandle, threshold: Option<f64>, currency: String) -> Result<settings::Settings, String> {
    if threshold.is_some_and(|value| !value.is_finite() || value <= 0.0 || value > 1_000_000_000.0)
        || currency.len() != 3 || !currency.bytes().all(|c| c.is_ascii_uppercase()) {
        return Err("请输入有效的三位大写币种代码，以及大于 0、不超过 10 亿的阈值".into());
    }
    let next = app.state::<Cfg>().0.try_update(|settings| {
        settings.balance_alert_threshold = threshold;
        settings.balance_alert_currency = currency;
    })?;
    let _ = app.emit("settings-changed", next.clone());
    Ok(next)
}

fn retention_cutoff(days: u32) -> Result<i64, String> {
    if !matches!(days, 30 | 90 | 180 | 365) {
        return Err("历史保留天数必须是 30、90、180 或 365".into());
    }
    Ok(chrono::Utc::now().timestamp_millis() - i64::from(days) * 86_400_000)
}

#[tauri::command]
fn set_retention_days(app: tauri::AppHandle, days: Option<u32>) -> Result<settings::Settings, String> {
    if let Some(days) = days { retention_cutoff(days)?; }
    let next = app.state::<Cfg>().0.try_update(|settings| settings.retention_days = days)?;
    let _ = app.emit("settings-changed", next.clone());
    Ok(next)
}

#[tauri::command]
async fn cleanup_preview(db: State<'_, DbRead>, days: u32) -> Result<db::CleanupPreview, String> {
    let cutoff = retention_cutoff(days)?;
    with_reader(db.0.clone(), move |conn| {
        db::cleanup_preview(conn, cutoff).map_err(|error| error.to_string())
    }).await
}

#[tauri::command]
async fn cleanup_history(app: tauri::AppHandle, db: State<'_, Db>, days: u32) -> Result<db::CleanupPreview, String> {
    let cutoff = retention_cutoff(days)?;
    let result = with_database(db.0.clone(), move |conn| {
        db::cleanup_history(conn, cutoff).map_err(|error| error.to_string())
    }).await?;
    let _ = app.emit("refresh-requested", ());
    Ok(result)
}

#[tauri::command]
async fn export_data(db: State<'_, DbRead>, platform: Option<String>) -> Result<String, String> {
    if platform.as_deref().is_some_and(|value| !platforms::known(value)) {
        return Err("导出范围必须是全部、Claude 或 Codex".into());
    }
    with_reader(db.0.clone(), move |conn| db::export_backup(conn, platform.as_deref())).await
}

#[tauri::command]
async fn import_data(app: tauri::AppHandle, db: State<'_, Db>, json: String) -> Result<db::ImportSummary, String> {
    let result = with_database(db.0.clone(), move |conn| db::import_backup(conn, &json)).await?;
    let _ = app.emit("refresh-requested", ());
    Ok(result)
}

#[tauri::command]
fn main_drag(app: tauri::AppHandle) -> Result<(), String> {
    let window = app.get_webview_window(MAIN).ok_or("找不到主面板")?;
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
async fn island_menu(app: tauri::AppHandle, window: WebviewWindow, refreshing: bool) -> Result<(), String> {
    let _ = refreshing;
    let id = island_id_of(&window)?;
    context_menu::open(&app, true, id)
}

#[tauri::command]
fn dock_hover(app: tauri::AppHandle, window: WebviewWindow) -> Option<dock::Edge> {
    if !app.state::<Cfg>().0.get().dock_enabled {
        return None;
    }
    island_id_of(&window).ok()?;
    dock::hover_edge(&window)
}

/// 松手：够近则吸附并落位，否则保持自由态
#[tauri::command]
fn dock_release(app: tauri::AppHandle, window: WebviewWindow) -> Result<Option<dock::SnapResult>, String> {
    let id = island_id_of(&window)?;
    Ok(dock_release_for(&app, &window, id.as_deref()))
}

fn dock_release_for(app: &tauri::AppHandle, w: &WebviewWindow, id: Option<&str>) -> Option<dock::SnapResult> {
    let store = &app.state::<Cfg>().0;
    if !store.get().dock_enabled {
        store.update(|settings| { island_clones::set_dock(settings, id, Default::default()); });
        emit_dock_changed(app, id, &Default::default());
        return None;
    }
    let r = dock::snap_on_release(w);
    let monitor = w.current_monitor().ok().flatten().and_then(|m| m.name().cloned());
    let dock_state = match &r {
        Some(sr) => dock::DockState { edge: sr.edge, offset: sr.offset, monitor },
        None => Default::default(),
    };
    store.update(|settings| { island_clones::set_dock(settings, id, dock_state.clone()); });
    emit_dock_changed(app, id, &dock_state);
    r
}

/// 解除停靠，回到自由态收缩尺寸
#[tauri::command]
fn dock_undock(app: tauri::AppHandle, window: WebviewWindow) -> Result<(), String> {
    let id = island_id_of(&window)?;
    dock_undock_for(&app, &window, id.as_deref());
    Ok(())
}

fn dock_undock_for(app: &tauri::AppHandle, w: &WebviewWindow, id: Option<&str>) {
    dock::undock(w);
    let next = app.state::<Cfg>().0.update(|settings| { island_clones::set_dock(settings, id, Default::default()); });
    let _ = app.emit("settings-changed", &next);
    emit_dock_changed(app, id, &Default::default());
}

/// 区间内的估算费用（§2.4）。本地记录不含账单，界面必须标注「估算」
#[tauri::command]
async fn cost_estimate(
    db: State<'_, DbRead>,
    platform: String,
    period: String,
    custom: Option<db::CustomRange>,
    model: Option<String>,
    query_end_ms: i64,
) -> Result<db::CostEstimate, String> {
    validate_query_range(custom, query_end_ms)?;
    with_reader(db.0.clone(), move |conn| {
        db::cost_estimate_at(conn, &platform, &period, custom, model.as_deref(), query_end_ms)
            .map_err(|e| e.to_string())
    })
    .await
}

/* --------------------- 额度与余额查询（§2.5 / §7.4） --------------------- */

/// 查某个连接的额度。
///
/// 路由规则：填了 `base_url` 时先按域名识别编程套餐供应商（智谱/Kimi/MiniMax/
/// ZenMux/OpenCode Go/火山，详见 `coding_plan`），未命中再视为 sub2api 自建网关；
/// 否则按平台走官方端点。**官方 OAuth 用量端点只认 OAuth token**，因此 API Key
/// 连接返回「不支持」而非「凭证失效」——后者会让用户白跑一趟重新授权。
#[tauri::command]
async fn connection_quota(app: tauri::AppHandle, db: State<'_, Db>, id: String, force: bool, max_age_ms: Option<u64>) -> Result<quota::QuotaState, String> {
    let force = refresh_mode(force, max_age_ms);
    let pool = db.0.clone();
    let lookup_id = id.clone();
    let Some(c) = with_database(pool.clone(), move |conn| {
        connections::query_creds(conn, &lookup_id)
    }).await? else {
        return Ok(quota::QuotaState::Failed { reason: "连接不存在".into() });
    };
    if platforms::native(&c.platform) {
        if c.platform == "grok" && c.kind == "auth" {
            return tauri::async_runtime::spawn_blocking(move || cached_query(&QUOTA_CACHE,id,force,quota_cache_outcome,grok_quota::local_grok_quota)).await.map_err(|e|e.to_string());
        }
        return Ok(quota::QuotaState::Unsupported {reason:platforms::limitation(&c.platform)});
    }
    let Some(secret) = c.secret.filter(|s| !s.trim().is_empty()) else {
        return Ok(quota::QuotaState::Unauthorized {
            reason: "该连接尚未保存凭证，请在 CC Switch 中配置后重新读取".into(),
        });
    };
    // 套餐查询辅助凭证（团队版组织/项目 ID、火山 AK/SK）在闭包外取出，闭包只借引用
    let plan = app.state::<settings::PlanQueryStore>().get();
    let local_account = if c.platform == "codex" && c.kind == "auth" {
        creds::local_codex_auth().and_then(|(local_token, account)| (local_token == secret).then_some(account).flatten())
    } else {
        None
    };
    let cache_id = id.clone();
    let state = tauri::async_runtime::spawn_blocking(move || cached_query(
        &QUOTA_CACHE,
        cache_id,
        force,
        quota_cache_outcome,
        || match c.base_url.as_deref() {
            Some(base) if c.kind == "api" && !base.is_empty() => {
                // 套餐供应商优先于 sub2api 网关：按 base_url 域名识别（智谱/Kimi/MiniMax 等）
                if coding_plan::detect_provider(base).is_some() {
                    let extras = coding_plan::PlanExtras::from_secrets(&plan);
                    coding_plan::coding_plan_quota(base, &secret, &extras)
                } else {
                    let state = quota::sub2api_quota(base, &secret);
                    // 404 = 网关没有 sub2api 账单端点（Claude 式反代、本地中转）：
                    // key 往往就是某家编程套餐的官方 key，探测识别后直接走套餐查询
                    match &state {
                        quota::QuotaState::Unsupported { reason }
                            if reason == quota::S2_ENDPOINT_MISSING =>
                        {
                            match coding_plan::probe_plan_cached(&secret) {
                                Some(provider) => coding_plan::query_plan_direct(provider, &secret),
                                None => state,
                            }
                        }
                        _ => state,
                    }
                }
            }
            _ => match (c.platform.as_str(), c.kind.as_str()) {
                ("claude", "auth") => quota::claude_oauth_quota(&secret),
                ("codex", "auth") => quota::codex_oauth_quota(&secret, local_account.as_deref()),
                ("claude" | "codex", "api") => quota::QuotaState::Unsupported {
                    reason: "API Key 不提供订阅额度；将显示官方组织用量（需 Admin Key）或本机会话统计".into(),
                },
                (platform, _) => quota::QuotaState::Unsupported {
                    reason: format!("{platform} 暂未接入官方额度查询"),
                },
            },
        },
    )).await.map_err(|e| e.to_string())?;

    let check_id = id.clone();
    with_database(pool.clone(), move |conn| connections::query_creds(conn, &check_id).map(|_| ())).await?;
    let update_id = id.clone();
    let success = matches!(state, quota::QuotaState::Ok { .. });
    let unauthorized = matches!(state, quota::QuotaState::Unauthorized { .. });
    if success || unauthorized {
        let changed = with_database(pool, move |conn| connections::record_query_status(conn, &update_id, success)).await?;
        if changed { let _ = app.emit("connections-changed", ()); }
    }
    if app.state::<Cfg>().0.get().island_connection_id.as_deref() == Some(id.as_str()) {
        update_tray_quota_icon(&app, &state);
    }
    Ok(state)
}

fn validate_query_range(custom: Option<db::CustomRange>, query_end_ms: i64) -> Result<(), String> {
    if query_end_ms <= 0 {
        return Err("查询结束时间无效".into());
    }
    if let Some(range) = custom {
        let end = range.end.unwrap_or(query_end_ms);
        if range.start < 0 || end > 253_402_300_799_999 {
            return Err("查询时间须在 1970 至 9999 年之间".into());
        }
        if range.start >= end {
            return Err("开始时间必须早于结束时间".into());
        }
    }
    Ok(())
}

/// 查直连 API Key 的官方组织用量与费用。普通调用 Key 返回权限不足，前端继续使用
/// 本机会话记录和估算费用；Admin Key 则优先显示官方聚合结果。
#[tauri::command]
async fn connection_api_usage(app: tauri::AppHandle, db: State<'_, Db>, id: String, force: bool, max_age_ms: Option<u64>) -> Result<quota::ApiUsageState, String> {
    let force = refresh_mode(force, max_age_ms);
    let pool = db.0.clone();
    let lookup_id = id.clone();
    let Some(c) = with_database(pool.clone(), move |conn| {
        connections::query_creds(conn, &lookup_id)
    }).await? else {
        return Ok(quota::ApiUsageState::Failed { reason: "连接不存在".into() });
    };
    if c.kind != "api" {
        return Ok(quota::ApiUsageState::Unsupported {
            reason: "Auth 连接使用订阅额度与本机会话统计，不查询组织 API 用量".into(),
        });
    }
    if c.base_url.as_deref().is_some_and(|base| !base.is_empty()) {
        return Ok(quota::ApiUsageState::Unsupported {
            reason: "自建网关继续使用网关额度与余额接口".into(),
        });
    }
    if platforms::native(&c.platform) {
        return Ok(quota::ApiUsageState::Unsupported {reason:platforms::limitation(&c.platform)});
    }
    let Some(secret) = c.secret.filter(|s| !s.trim().is_empty()) else {
        return Ok(quota::ApiUsageState::Unauthorized {
            reason: "该连接尚未保存 API Key".into(),
        });
    };
    let cache_id = id.clone();
    let state = tauri::async_runtime::spawn_blocking(move || cached_query(
        &API_USAGE_CACHE,
        cache_id,
        force,
        api_usage_cache_outcome,
        || match c.platform.as_str() {
            "claude" | "codex" => quota::official_api_usage(&c.platform, &secret),
            platform => quota::ApiUsageState::Unsupported {
                reason: format!("{platform} 暂未接入官方组织用量查询"),
            },
        },
    )).await.map_err(|e| e.to_string())?;
    let check_id = id.clone();
    with_database(pool.clone(), move |conn| connections::query_creds(conn, &check_id).map(|_| ())).await?;
    let valid = matches!(state, quota::ApiUsageState::Ok { .. });
    let unauthorized = matches!(state, quota::ApiUsageState::Unauthorized { .. });
    if valid || unauthorized {
        let update_id = id;
        let changed = with_database(pool, move |conn| connections::record_query_status(conn, &update_id, valid)).await?;
        if changed { let _ = app.emit("connections-changed", ()); }
    }
    Ok(state)
}

/// 查余额。目前只有 sub2api 网关提供钱包余额；
/// 官方订阅没有「余额」概念，如实返回不支持。
#[tauri::command]
async fn connection_balance(app: tauri::AppHandle, db: State<'_, Db>, id: String, force: bool, max_age_ms: Option<u64>) -> Result<quota::BalanceState, String> {
    let force = refresh_mode(force, max_age_ms);
    let pool = db.0.clone();
    let lookup_id = id.clone();
    let Some(c) = with_database(pool.clone(), move |conn| {
        connections::query_creds(conn, &lookup_id)
    }).await? else {
        return Ok(quota::BalanceState::Failed { reason: "连接不存在".into() });
    };
    if platforms::native(&c.platform) {
        return Ok(quota::BalanceState::Unsupported {reason:platforms::limitation(&c.platform)});
    }
    let Some(secret) = c.secret.filter(|s| !s.trim().is_empty()) else {
        return Ok(quota::BalanceState::Unauthorized {
            reason: "该连接尚未保存凭证，请在 CC Switch 中配置后重新读取".into(),
        });
    };
    let Some(base) = c.base_url.filter(|base| !base.is_empty()) else {
        return Ok(quota::BalanceState::Unsupported {
            reason: "官方订阅与直连 API Key 不提供钱包余额；填写 sub2api 部署地址后可查询".into(),
        });
    };
    // 编程套餐供应商（智谱/Kimi/MiniMax 等）没有钱包余额接口；硬打 sub2api 端点
    // 只会得到 404，让用户误以为是配置错误。额度窗口请看订阅额度查询。
    if coding_plan::detect_provider(&base).is_some() {
        return Ok(quota::BalanceState::Unsupported {
            reason: "该编程套餐供应商未提供钱包余额接口，请看订阅额度窗口".into(),
        });
    }
    let cache_id = id.clone();
    let state = tauri::async_runtime::spawn_blocking(move || {
        let state = cached_query(
            &BALANCE_CACHE,
            cache_id,
            force,
            balance_cache_outcome,
            || quota::sub2api_balance(&base, &secret),
        );
        // 404 = 网关没有 sub2api 账单端点：识别 key 所属套餐，指明「不支持」而非「失败」
        match &state {
            quota::BalanceState::Unsupported { reason } if reason == quota::S2_ENDPOINT_MISSING => {
                match coding_plan::probe_plan_cached(&secret) {
                    Some(provider) => quota::BalanceState::Unsupported {
                        reason: format!(
                            "{}套餐未提供钱包余额接口，请看订阅额度窗口",
                            provider.display_name()
                        ),
                    },
                    None => state,
                }
            }
            _ => state,
        }
    })
        .await.map_err(|e| e.to_string())?;
    let check_id = id.clone();
    with_database(pool.clone(), move |conn| connections::query_creds(conn, &check_id).map(|_| ())).await?;
    let success = matches!(state, quota::BalanceState::Ok { .. });
    let unauthorized = matches!(state, quota::BalanceState::Unauthorized { .. });
    if success || unauthorized {
        let changed = with_database(pool, move |conn| connections::record_query_status(conn, &id, success)).await?;
        if changed { let _ = app.emit("connections-changed", ()); }
    }
    Ok(state)
}

/* ------------------------ 持久化设置（§2.6 / §7.3） ---------------------- */

#[tauri::command]
fn get_settings(cfg: State<Cfg>) -> settings::Settings {
    cfg.0.get()
}

/// 设置界面改灵动岛平台：与托盘子菜单是同一个值，改完要刷新托盘勾选态
#[tauri::command]
fn set_island_platform(
    app: tauri::AppHandle,
    cfg: State<Cfg>,
    platform: String,
) -> Result<settings::Settings, String> {
    if !platforms::known(&platform) {
        return Err("未知平台".into());
    }
    let next = cfg.0.try_update(|s| {
        s.island_platform = platform.clone();
        s.island_connection_id = None;
        s.island_connection_name = None;
        s.island_source_id = Some(format!("local:{platform}"));
    })?;
    let _ = app.emit("island-platform-changed", &platform);
    let _ = app.emit("settings-changed", &next);
    refresh_tray_menu(&app);
    Ok(next)
}

#[tauri::command]
fn set_island_source(
    app: tauri::AppHandle,
    cfg: State<Cfg>,
    id: Option<String>,
) -> Result<settings::Settings, String> {
    let platform = cfg.0.get().island_platform;
    let valid = local_source(&platform).map(|source| source.id);
    if id.is_some() && id != valid {
        return Err("所选统计来源不存在或不属于当前平台".into());
    }
    let next = cfg.0.try_update(|settings| settings.island_source_id = id)?;
    let _ = app.emit("settings-changed", &next);
    Ok(next)
}

/// `island` 为目标岛（None 为本体）。托盘摘要与主面板默认连接只跟随本体。
#[tauri::command]
async fn set_island_connection(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    cfg: State<'_, Cfg>,
    id: Option<String>,
    island: Option<String>,
) -> Result<settings::Settings, String> {
    let selected = match id.as_ref() {
        Some(id) => {
            let lookup_id = id.clone();
            let Some(c) = with_database(db.0.clone(), move |conn| {
                connections::selectable_creds(conn, &lookup_id)
            }).await? else {
                return Err("所选连接不存在".into());
            };
            let source_id = native_sources::monitor_id(id).map(str::to_string).unwrap_or_else(|| format!("local:{}", c.platform));
            Some(island_clones::SelectedConnection { id: id.clone(), name: c.name, platform: c.platform, kind: c.kind, source_id })
        }
        None => None,
    };
    let mut found = false;
    let next = cfg.0.try_update(|s| { found = island_clones::apply_connection(s, island.as_deref(), selected.as_ref()); })?;
    if !found { return Err("目标灵动岛不存在".into()); }
    let _ = app.emit("settings-changed", &next);
    if island.is_none() {
        let _ = app.emit("island-platform-changed", &next.island_platform);
        refresh_tray_menu(&app);
    }
    Ok(next)
}

#[tauri::command]
fn set_dnd(app: tauri::AppHandle, cfg: State<Cfg>, on: bool) -> Result<settings::Settings, String> {
    let next = cfg.0.try_update(|s| s.dnd = on)?;
    let _ = app.emit("dnd-changed", on);
    let _ = app.emit("settings-changed", &next);
    refresh_tray_menu(&app);
    Ok(next)
}

/* --------------------------- 本地代理（阶段二） --------------------------- */

/// 代理开关 / 端口 / 回退开关；切换即执行接管（或还原）并启动（或停止）监听。
/// 同步命令：接管与端口绑定都是毫秒级文件写与 bind，失败原因直接返回设置页。
#[tauri::command]
fn set_proxy(
    app: tauri::AppHandle,
    port: u16,
    enabled: bool,
    fallback_direct: bool,
) -> Result<settings::Settings, String> {
    proxy::set_proxy(&app, port, enabled, fallback_direct)
}

#[tauri::command]
fn proxy_status(app: tauri::AppHandle) -> proxy::ProxyStatus {
    proxy::status(&app)
}

/* --------------------------- 连接管理（§6.3） --------------------------- */

#[tauri::command]
async fn list_connections(db: State<'_, DbRead>) -> Result<Vec<connections::ConnectionDto>, String> {
    with_reader(db.0.clone(), move |conn| {
        connections::list(conn).map_err(|e| e.to_string())
    })
    .await
}

/// 「获取模型」（画布 18）：用用户填写的地址与 Key 拉取网关模型列表。
/// Key 只在本命令内使用，不落库、不打印。
#[tauri::command]
async fn fetch_remote_models(platform: String, base_url: String, secret: String) -> Result<Vec<String>, String> {
    if !matches!(platform.as_str(), "claude" | "codex") {
        return Err("不支持的平台".into());
    }
    tauri::async_runtime::spawn_blocking(move || connections::fetch_models(&platform, &base_url, &secret))
        .await
        .map_err(|e| e.to_string())?
}

/// 当前模型支持的思考强度档位（数据库规则匹配，无规则命中回落全部已知档位）。
#[tauri::command]
async fn get_effort_options(db: State<'_, Db>, model: Option<String>) -> Result<Vec<String>, String> {
    with_database(db.0.clone(), move |conn| {
        Ok(effort_map::efforts_for_model(conn, model.as_deref().unwrap_or("")))
    })
    .await
}

/// 「获取强度」（画布 18，鼠鼠需求）：直接调官方 `/v1/models`（用弹窗里的
/// 地址与 Key 鉴权），解析每个模型的 `capabilities.effort` 元数据，
/// 把"模型 → 支持档位"写入数据库，之后下拉即按模型过滤可选档位。
/// 无副作用：只写 effort 两张表，不动连接与凭证。
#[tauri::command]
async fn refresh_effort_levels(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    platform: String,
    base_url: String,
    secret: String,
) -> Result<String, String> {
    if !matches!(platform.as_str(), "claude" | "codex") {
        return Err("不支持的平台".into());
    }
    let entries = tauri::async_runtime::spawn_blocking(move || {
        effort_map::fetch_model_efforts(&platform, &base_url, &secret)
    })
    .await
    .map_err(|e| e.to_string())??;
    let count = entries.len();
    let level_total: usize = entries.iter().map(|(_, levels)| levels.len()).sum();
    with_database(db.0.clone(), move |conn| {
        effort_map::upsert_model_levels(conn, &entries)
    })
    .await?;
    let _ = app.emit("effort-levels-changed", ());
    Ok(format!("已从官方接口获取 {count} 个模型、{level_total} 条档位记录，并存入本机"))
}

/// 启用连接（画布 18 后续，对齐 cc-switch）：把连接保存的凭证与地址写入
/// 对应 CLI 的配置文件（Claude settings.json / .credentials.json、Codex auth.json
/// / config.toml），写前自动备份；同时在本应用内恢复该连接的额度查询。
/// 返回写入说明文案（供 Toast 展示），不含任何凭证内容。
#[tauri::command]
async fn enable_connection(app: tauri::AppHandle, db: State<'_, Db>, id: String) -> Result<String, String> {
    let info = with_database(db.0.clone(), {
        let id = id.clone();
        move |conn| connections::creds_of(conn, &id).map_err(|e| e.to_string())
    })
    .await?
    .ok_or("连接不存在")?;
    if provider_key::supported(&info.platform) && info.kind == "api" {
        let secret=info.secret.clone().ok_or("该连接没有保存 API Key")?;
        tauri::async_runtime::spawn_blocking(move || provider_key::validate(&info.platform,&secret,info.base_url.as_deref()))
            .await.map_err(|e|e.to_string())??;
        let key=id.clone();
        with_database(db.0.clone(),move |conn|connections::set_paused(conn,&key,false)).await?;
        invalidate_network_cache(&id);
        let _=app.emit("connections-changed",());
        return Ok("官方 Key 检测通过；仅保存连接，不修改外部应用登录或授予余额权限".into());
    }
    if platforms::native(&info.platform) {
        native_sources::check(&id,&info.platform)?;
        let key=id.clone();
        with_database(db.0.clone(),move |conn|connections::set_paused(conn,&key,false)).await?;
        invalidate_network_cache(&id);
        let _=app.emit("connections-changed",());
        let _=app.emit("refresh-requested",id);
        return Ok("已启用本机数据监控；外部应用账号与配置未更改".into());
    }
    let secret = info.secret.clone().ok_or("该连接没有保存凭证，无法写入 CLI 配置")?;
    let backup_dir = data_paths(&app).map_err(|e| e.to_string())?.0.join("cli-backups");
    let note = tauri::async_runtime::spawn_blocking(move || {
        cli_apply::apply_to_cli(&info.platform, &info.kind, &secret, info.base_url.as_deref(), &backup_dir)
    })
    .await
    .map_err(|e| e.to_string())??;
    with_database(db.0.clone(), {
        let id = id.clone();
        move |conn| connections::set_paused(conn, &id, false)
    })
    .await?;
    invalidate_network_cache(&id);
    let _ = app.emit("connections-changed", ());
    Ok(note.message)
}

/// 编辑回显：返回连接保存的完整凭证。这是凭证纪律的**显式例外**——
/// 连接管理本质是本机的凭证管理器（对齐 cc-switch），编辑弹窗回填原值
/// 必须拿到原 Key；仅编辑弹窗打开/点小眼睛时调用，日志与 Toast 均不含原值。
#[derive(serde::Serialize)]
struct ConnectionTest {
    ok: bool,
    latency_ms: u64,
    /// 检测完成但不可用时给出原因；过程性错误直接走 Err
    message: Option<String>,
}

/// 检测连接可用性（画布 18 后续，对齐 cc-switch 的检测按钮）：
/// 用保存的凭证对上游做一次轻量验证并计时。成功时记录最近同步时间，
/// 失败只如实报告，不把失败检测写成成功同步。
#[tauri::command]
async fn test_connection(app: tauri::AppHandle, db: State<'_, Db>, id: String) -> Result<ConnectionTest, String> {
    let Some(current) = with_database(db.0.clone(), {
        let id = id.clone();
        move |conn| connections::creds_of(conn, &id).map_err(|e| e.to_string())
    })
    .await?
    else {
        return Err("连接不存在".into());
    };
    let result = if platforms::native(&current.platform) && current.kind == "auth" {
        match native_sources::check(&id,&current.platform) {
            Ok(source)=>ConnectionTest {ok:true,latency_ms:0,message:Some(format!("{} 本机来源可读；未验证线上账号或余额",source.name))},
            Err(reason)=>ConnectionTest {ok:false,latency_ms:0,message:Some(reason)},
        }
    } else {
        let secret = current.secret.clone().ok_or("该连接没有保存凭证，无法检测")?;
        let plan = app.state::<settings::PlanQueryStore>().get();
        tauri::async_runtime::spawn_blocking(move || {
            let started = std::time::Instant::now();
            let extras = coding_plan::PlanExtras::from_secrets(&plan);
            let latency_ms = || started.elapsed().as_millis() as u64;
            match quota::validate_connection(&current.platform, &current.kind, &secret, current.base_url.as_deref(), &extras) {
                Ok(()) => ConnectionTest { ok: true, latency_ms: latency_ms(), message: None },
                Err(reason) => ConnectionTest { ok: false, latency_ms: latency_ms(), message: Some(reason) },
            }
        }).await.map_err(|e| e.to_string())?
    };
    if result.ok {
        with_database(db.0.clone(), move |conn| connections::record_query_status(conn, &id, true)).await?;
        let _ = app.emit("connections-changed", ());
    }
    Ok(result)
}

#[tauri::command]
async fn reveal_connection_secret(db: State<'_, Db>, id: String) -> Result<String, String> {
    with_database(db.0.clone(), move |conn| {
        connections::creds_of(conn, &id).map_err(|e| e.to_string())
    })
    .await?
    .and_then(|info| info.secret)
    .ok_or_else(|| "该连接没有保存凭证".into())
}

/// 统一编辑连接（画布 18）：完整快照式更新。
/// auth 连接只接受名称（凭证由 CLI 管理）；api 连接可改网关地址、Key 与默认参数。
/// secret 为空表示不更换 Key；地址或 Key 变化时先用组合后的凭证做一次网络验证，
/// 验证通过才落库，避免把不可用的配置存进去。
#[tauri::command]
async fn update_connection(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    cfg: State<'_, Cfg>,
    id: String,
    name: String,
    base_url: Option<String>,
    secret: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    context_1m: Option<bool>,
) -> Result<(), String> {
    let lookup_id = id.clone();
    let Some(current) = with_database(db.0.clone(), move |conn| {
        connections::creds_of(conn, &lookup_id).map_err(|e| e.to_string())
    })
    .await?
    else {
        return Err("连接不存在".into());
    };
    let trimmed_name = name.trim().to_string();

    if current.kind != "api" {
        // 官方订阅：凭证由 CLI 登录管理，编辑仅限名称
        let rename_id = id.clone();
        let rename_name = trimmed_name.clone();
        with_database(db.0.clone(), move |conn| {
            connections::rename_connection(conn, &rename_id, &rename_name)
        })
        .await?;
    } else {
        let final_base = base_url
            .map(|value| value.trim().trim_end_matches('/').to_string())
            .filter(|value| !value.is_empty());
        let final_base = connections::normalized_base_for(&current.platform, final_base);
        if provider_key::supported(&current.platform) {
            provider_key::check_base(&current.platform, final_base.as_deref())?;
        }
        let new_secret = secret
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let base_changed = final_base != current.base_url;
        let key_changed = new_secret.as_deref().is_some_and(|s| Some(s) != current.secret.as_deref());
        if base_changed || key_changed {
            let platform = current.platform.clone();
            let probe_secret = new_secret.clone().or_else(|| current.secret.clone()).ok_or("该连接没有保存凭证，请填写 API Key")?;
            let probe_base = final_base.clone();
            let plan = app.state::<settings::PlanQueryStore>().get();
            tauri::async_runtime::spawn_blocking(move || {
                let extras = coding_plan::PlanExtras::from_secrets(&plan);
                quota::validate_connection(&platform, "api", &probe_secret, probe_base.as_deref(), &extras)
            })
            .await
            .map_err(|e| e.to_string())??;
        }
        let update_id = id.clone();
        let rename_name = trimmed_name.clone();
        with_database(db.0.clone(), move |conn| {
            connections::rename_connection(conn, &update_id, &rename_name)?;
            if let Some(new_secret) = new_secret {
                connections::replace_api_key(conn, &update_id, &new_secret)?;
            }
            connections::update_params(
                conn,
                &update_id,
                final_base.as_deref(),
                model.as_deref().map(str::trim).filter(|m| !m.is_empty()),
                effort.as_deref().map(str::trim).filter(|e| !e.is_empty()),
                context_1m,
            )
        })
        .await?;
        if key_changed {
            let _ = app.emit("refresh-requested", id.clone());
        }
    }
    // 名称若是任一灵动岛的当前连接，同步快照（与 rename_connection 命令同一套逻辑）
    let mut renamed = false;
    let next = cfg.0.try_update(|s| { renamed = island_clones::rename_connection(s, &id, &trimmed_name); })?;
    if renamed {
        let _ = app.emit("settings-changed", &next);
        refresh_tray_menu(&app);
    }
    invalidate_network_cache(&id);
    let _ = app.emit("connections-changed", ());
    Ok(())
}

#[tauri::command]
async fn add_connection(app: tauri::AppHandle, db: State<'_, Db>, input: connections::NewConnection) -> Result<String, String> {
    if validation::isolated() && input.secret.as_deref().map_or(true, |s| s.trim().is_empty()) {
        return Err("隔离验收必须显式提供测试凭证".into());
    }
    let input = connections::prepare_new(input)?;
    let platform = input.platform.clone();
    let kind = input.kind.clone();
    let secret = input.secret.clone().ok_or("连接尚未准备好凭证")?;
    let base_url = input.base_url.clone();
    let plan = app.state::<settings::PlanQueryStore>().get();
    tauri::async_runtime::spawn_blocking(move || {
        let extras = coding_plan::PlanExtras::from_secrets(&plan);
        quota::validate_connection(&platform, &kind, &secret, base_url.as_deref(), &extras)
    }).await.map_err(|e| e.to_string())??;
    let id = with_database(db.0.clone(), move |conn| {
        connections::add(conn, input)
    })
    .await?;
    invalidate_network_cache(&id);
    let _ = app.emit("connections-changed", ());
    Ok(id)
}

/// 移除连接：只删凭证与配置，历史统计保留（§6.3）
#[tauri::command]
async fn remove_connection(app: tauri::AppHandle, db: State<'_, Db>, id: String) -> Result<(), String> {
    let remove_id = id.clone();
    with_database(db.0.clone(), move |conn| {
        connections::remove(conn, &remove_id).map_err(|e| e.to_string())
    })
    .await?;
    invalidate_network_cache(&id);
    let _ = app.emit("connections-changed", ());
    Ok(())
}

#[tauri::command]
async fn set_connection_paused(app: tauri::AppHandle, db: State<'_, Db>, id: String, paused: bool) -> Result<(), String> {
    if !paused {
        let lookup_id = id.clone();
        let current = with_database(db.0.clone(), move |conn| {
            connections::creds_of(conn, &lookup_id).map_err(|e| e.to_string())
        }).await?.ok_or("连接不存在")?;
        if platforms::native(&current.platform) && current.kind == "auth" {
            native_sources::check(&id,&current.platform)?;
        } else {
        let secret = current.secret.ok_or("该连接没有保存凭证，请在 CC Switch 中配置后重新读取")?;
        let plan = app.state::<settings::PlanQueryStore>().get();
        tauri::async_runtime::spawn_blocking(move || {
            let extras = coding_plan::PlanExtras::from_secrets(&plan);
            quota::validate_connection(&current.platform, &current.kind, &secret, current.base_url.as_deref(), &extras)
        }).await.map_err(|e| e.to_string())??;
        }
    }
    let update_id = id.clone();
    with_database(db.0.clone(), move |conn| connections::set_paused(conn, &update_id, paused)).await?;
    invalidate_network_cache(&id);
    if app.state::<Cfg>().0.get().island_connection_id.as_deref() == Some(id.as_str()) {
        update_tray_quota_icon(&app, &quota::QuotaState::Failed {
            reason: if paused { "连接已断开" } else { "等待连接数据更新" }.into(),
        });
    }
    let _ = app.emit("connections-changed", ());
    // 重新连接后立刻取数，否则界面要停在「等待连接数据更新」直到下一轮轮询。
    // 断开则不必：此时界面本就该显示已断开，多发一次查询只是白跑。
    if !paused {
        let _ = app.emit("refresh-requested", ());
    }
    Ok(())
}

/// 「获取」：重读本机凭证刷新已有连接。失败不破坏现有连接（§6.3）
#[tauri::command]
async fn fetch_credentials(app: tauri::AppHandle, db: State<'_, Db>, id: String) -> Result<connections::FetchResult, String> {
    if validation::isolated() { return Err("隔离验收不读取本机凭证".into()); }
    let fetch_id = id.clone();
    let result = with_database(db.0.clone(), move |conn| {
        if native_sources::monitor_id(&fetch_id).is_some() {
            let info=connections::creds_of(conn,&fetch_id).map_err(|e|e.to_string())?.ok_or("连接不存在")?;
            let source=native_sources::check(&fetch_id,&info.platform)?;
            return Ok(connections::FetchResult {ok:true,masked:None,message:"本机来源已重新检测；未读取或改写外部登录凭证".into(),scanned:vec![source.path.display().to_string()],changed:false});
        }
        connections::fetch_local(conn, &fetch_id).map_err(|e| e.to_string())
    })
    .await?;
    if result.ok {
        invalidate_network_cache(&id);
        let _ = app.emit("connections-changed", ());
        // 缓存刚被清空，无论凭证是否变化都立即重查。用户按「更新」的意图是
        // 「我要最新数据」：凭证没变时同样该看到刷新，而不是毫无动静——
        // 只在 changed 时通知，会让「本机凭证未变化」这条最常见的路径彻底没反馈。
        // 携带目标连接 ID：只有这条连接被灵动岛/总览选中时才重查，避免更新一行广播刷新另一条。
        let _ = app.emit("refresh-requested", id);
    }
    Ok(result)
}

#[tauri::command]
async fn replace_api_key(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    id: String,
    secret: String,
) -> Result<(), String> {
    let secret = secret.trim().to_string();
    if secret.is_empty() { return Err("API Key 不能为空".into()); }
    let lookup_id = id.clone();
    let Some(current) = with_database(db.0.clone(), move |conn| {
        connections::creds_of(conn, &lookup_id).map_err(|e| e.to_string())
    }).await? else { return Err("连接不存在".into()); };
    if current.paused { return Err("连接已断开，请先重新连接".into()); }
    if current.kind != "api" { return Err("该连接不是 API Key 连接".into()); }
    let platform = current.platform;
    let base_url = current.base_url;
    let probe_secret = secret.clone();
    let plan = app.state::<settings::PlanQueryStore>().get();
    tauri::async_runtime::spawn_blocking(move || {
        let extras = coding_plan::PlanExtras::from_secrets(&plan);
        quota::validate_connection(&platform, "api", &probe_secret, base_url.as_deref(), &extras)
    }).await.map_err(|e| e.to_string())??;
    let replace_id = id.clone();
    with_database(db.0.clone(), move |conn| connections::replace_api_key(conn, &replace_id, &secret)).await?;
    invalidate_network_cache(&id);
    let _ = app.emit("connections-changed", ());
    // 换了 Key 必然要用新凭证重查，否则界面会一直停在旧 Key 的结果上
    // 更换 API Key 同样只刷新目标连接；全局刷新入口仍发送空 payload。
    let _ = app.emit("refresh-requested", id);
    Ok(())
}

/// 本机可自动发现的连接候选，供「+ 添加连接」预填
#[tauri::command]
fn discover_connections() -> Vec<creds::Candidate> {
    if validation::isolated() { return vec![]; }
    creds::discover()
}

/// 重命名连接：只改显示名称。若改的正是灵动岛当前连接，
/// 需同步刷新设置里的名称快照（托盘/灵动岛显示用的是快照，不实时查库）。
#[tauri::command]
async fn rename_connection(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    cfg: State<'_, Cfg>,
    id: String,
    name: String,
) -> Result<(), String> {
    let rename_id = id.clone();
    let rename_name = name.clone();
    with_database(db.0.clone(), move |conn| {
        connections::rename_connection(conn, &rename_id, &rename_name)
    }).await?;
    let mut renamed = false;
    let next = cfg.0.try_update(|s| { renamed = island_clones::rename_connection(s, &id, &name); })?;
    if renamed {
        let _ = app.emit("settings-changed", &next);
        refresh_tray_menu(&app);
    }
    let _ = app.emit("connections-changed", ());
    Ok(())
}

#[derive(serde::Serialize)]
struct LocalReadSummary { added: usize, existing: usize, warnings: Vec<String> }

#[tauri::command]
async fn read_local_connections(app: tauri::AppHandle, db: State<'_, Db>, platform: String, kind: String, name: Option<String>) -> Result<LocalReadSummary, String> {
    if validation::isolated() { return Err("隔离验收环境禁止读取真实凭证".into()); }
    if platforms::native(&platform) {
        if kind != "auth" { return Err("该平台请使用本机来源获取".into()); }
        let result=with_database(db.0.clone(), move |conn| {
            native_sources::register(conn,&platform,name.as_deref()).map(|(added,existing,warnings)|LocalReadSummary {added,existing,warnings})
        }).await?;
        let _=app.emit("connections-changed",());
        return Ok(result);
    }
    let inputs = tauri::async_runtime::spawn_blocking(move || creds::local_connections(&platform, &kind)).await.map_err(|e| e.to_string())??;
    let result = with_database(db.0.clone(), move |conn| {
        // 用户在添加时填了名称就覆盖自动生成名；校验失败在这里整体报错，不写库
        let inputs = connections::with_custom_name(inputs, name)?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let mut summary = LocalReadSummary { added: 0, existing: 0, warnings: vec![] };
        for input in inputs {
            if connections::register_local(&tx, input)?.1 { summary.added += 1; } else { summary.existing += 1; }
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(summary)
    }).await?;
    let _ = app.emit("connections-changed", ());
    Ok(result)
}

const ISLAND: &str = "island";
const MAIN: &str = "main";
const STARTUP: &str = "startup";

static STARTUP_READY: AtomicBool = AtomicBool::new(false);
static STARTUP_INITIALIZING: AtomicBool = AtomicBool::new(false);
static STARTUP_REQUESTED: AtomicBool = AtomicBool::new(false);
static MAIN_READY: AtomicBool = AtomicBool::new(false);
static FIRST_INSTALL: AtomicBool = AtomicBool::new(false);
static FIRST_INSTALL_INTRO: AtomicBool = AtomicBool::new(false);
static MAIN_PLACEMENT: LazyLock<Mutex<Option<main_window::Placement>>> = LazyLock::new(|| Mutex::new(None));
static STARTUP_ERROR: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

fn persist_main_placement(app: &tauri::AppHandle, placement: main_window::Placement) {
    if !placement.valid() { return; }
    app.state::<Cfg>().0.update(|settings| settings.main_window = Some(placement));
}

fn record_main_window_event(window: &tauri::Window, event: &WindowEvent) {
    if !MAIN_READY.load(Ordering::Acquire) { return; }
    let placement = {
        let Ok(mut current) = MAIN_PLACEMENT.lock() else { return };
        let Some(placement) = current.as_mut() else { return };
        let mut next = placement.clone();
        match event {
            WindowEvent::Moved(position) => { next.x = position.x; next.y = position.y; }
            WindowEvent::Resized(size) => { next.width = size.width; next.height = size.height; }
            WindowEvent::CloseRequested { .. } => {},
            _ => return,
        }
        if !next.valid() { return; }
        *placement = next.clone();
        next
    };
    if matches!(event, WindowEvent::CloseRequested { .. }) {
        persist_main_placement(window.app_handle(), placement);
    }
}

fn show_startup(app: &tauri::AppHandle) -> tauri::Result<()> {
    let window = if let Some(window) = app.get_webview_window(STARTUP) {
        window
    } else {
        WebviewWindowBuilder::new(app, STARTUP, WebviewUrl::App("startup.html".into()))
            .title("CC Usage · 启动中")
            // 卡片 360×220，四周各留 10px 透明边距给页面自绘的阴影
            .inner_size(380.0, 240.0)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .skip_taskbar(true)
            .center()
            .build()?
    };
    window.show()?;
    let _ = window.set_focus();
    Ok(())
}

fn report_startup_error(app: &tauri::AppHandle, error: String) {
    eprintln!("[启动] {error}");
    if let Ok(mut current) = STARTUP_ERROR.lock() { *current = Some(error.clone()); }
    if STARTUP_REQUESTED.load(Ordering::Acquire) {
        let _ = show_startup(app);
        let _ = app.emit("startup-error", error);
    }
}

#[tauri::command]
fn startup_error() -> Option<String> {
    STARTUP_ERROR.lock().ok().and_then(|current| current.clone())
}

#[tauri::command]
fn quit_startup(app: tauri::AppHandle) { app.exit(1); }

#[tauri::command]
fn retry_startup(app: tauri::AppHandle) -> Result<&'static str, String> {
    if STARTUP_INITIALIZING.load(Ordering::Acquire) {
        return Ok("initializing");
    }
    if let Ok(mut current) = STARTUP_ERROR.lock() { *current = None; }
    if STARTUP_READY.load(Ordering::Acquire) {
        if !MAIN_READY.load(Ordering::Acquire) {
            if let Some(window) = app.get_webview_window(MAIN) {
                window.destroy().map_err(|error| error.to_string())?;
            }
            show_main(&app);
        } else {
            show_main(&app);
        }
    } else {
        let (data_dir, _) = data_paths(&app)?;
        start_initialization(app, data_dir);
    }
    Ok("restarting")
}

#[tauri::command]
fn main_panel_ready(app: tauri::AppHandle) -> Result<(), String> {
    let window = app.get_webview_window(MAIN).ok_or("主面板窗口不存在")?;
    window.show().map_err(|error| error.to_string())?;
    window.unminimize().map_err(|error| error.to_string())?;
    if let Ok(mut placement) = MAIN_PLACEMENT.lock() {
        *placement = main_window::capture(&window);
    }
    MAIN_READY.store(true, Ordering::Release);
    let _ = window.set_focus();
    if let Some(splash) = app.get_webview_window(STARTUP) { let _ = splash.close(); }
    Ok(())
}

fn activate_existing_instance(app: &tauri::AppHandle) {
    STARTUP_REQUESTED.store(true, Ordering::Release);
    if STARTUP_READY.load(Ordering::Acquire) {
        let settings = app.try_state::<Cfg>().map(|cfg| cfg.0.get()).unwrap_or_default();
        show_main_with_intent(app, MainIntent::Overview { settings });
    } else if let Err(error) = show_startup(app) {
        eprintln!("[启动] 无法显示启动窗口: {error}");
    }
}

/// 本体由 tauri.conf 创建、分身在启动与「开启分身」时创建；WebView 被外部关闭或崩溃后，
/// 托盘仍能按同一配置重建。`id` 为 None 表示本体。
fn ensure_island_window_for(app: &tauri::AppHandle, id: Option<&str>) -> Result<WebviewWindow, String> {
    let label = island_clones::label(id);
    if let Some(window) = app.get_webview_window(&label) {
        return Ok(window);
    }
    let settings = app.state::<Cfg>().0.get();
    let profile = island_clones::profile(&settings, id).ok_or("灵动岛配置不存在")?;
    let url = match id {
        Some(id) => format!("index.html?window=island&clone={id}"),
        None => "index.html?window=island".to_string(),
    };
    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(url.into()))
        .title("CC Usage")
        .inner_size(428.0, 124.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(settings.always_on_top)
        .skip_taskbar(true)
        .visible(false)
        .center()
        .build().map_err(|e| e.to_string())?;
    if settings.dock_enabled && profile.dock.edge.is_some() {
        let _ = dock::restore_dock(&window, &profile.dock);
    } else if let Some((x, y)) = id
        .and_then(|id| settings.island_clones.iter().find(|clone| clone.id == id))
        .and_then(|clone| clone.position)
    {
        // 分身自由态：先挪到记录的位置，再夹回当前工作区，显示器变化后不会落到屏幕外。
        let _ = window.set_position(PhysicalPosition::new(x, y));
        let _ = dock::resize_free(&window, dock::FREE_W, dock::FREE_H);
    }
    Ok(window)
}

/// 「开启分身」：以来源岛的连接为初始值，新窗口在来源岛右下 24px 以自由态出现。
fn create_island_clone(app: &tauri::AppHandle, source: Option<&str>) -> Result<(), String> {
    let store = &app.state::<Cfg>().0;
    // 先在副本上校验上限与来源，失败不落盘。
    island_clones::create_clone(&mut store.get(), source)?;
    let mut created = None;
    let next = store.try_update(|settings| { created = island_clones::create_clone(settings, source).ok(); })?;
    let clone = created.ok_or("创建分身失败")?;
    let window = ensure_island_window_for(app, Some(&clone.id))?;
    if let Some(origin) = island_window(app, source) {
        if let (Ok(position), Ok(scale)) = (origin.outer_position(), origin.scale_factor()) {
            let offset = (24.0 * scale).round() as i32;
            let _ = window.set_position(PhysicalPosition::new(position.x + offset, position.y + offset));
            let _ = dock::resize_free(&window, dock::FREE_W, dock::FREE_H);
        }
    }
    remember_clone_position(app, &clone.id, &window);
    let _ = window.set_always_on_top(next.always_on_top);
    if next.island_visible {
        window.show().map_err(|e| e.to_string())?;
        let _ = window.set_focus();
    }
    let _ = app.emit("settings-changed", store.get());
    let _ = app.emit("menu-state-changed", ());
    Ok(())
}

/// 「销毁分身」：销毁右键所在的岛；本体被销毁时第一个分身接任，本体窗口挪到它的位置。
fn destroy_island(app: &tauri::AppHandle, target: Option<&str>) -> Result<(), String> {
    let store = &app.state::<Cfg>().0;
    island_clones::destroy(&mut store.get(), target)?;
    let mut plan = None;
    store.try_update(|settings| { plan = island_clones::destroy(settings, target).ok(); })?;
    match plan.ok_or("销毁分身失败")? {
        island_clones::DestroyPlan::CloseClone(id) => {
            // 灵动岛的关闭请求会被拦成隐藏，这里必须直接销毁。
            if let Some(window) = island_window(app, Some(&id)) { window.destroy().map_err(|e| e.to_string())?; }
        }
        island_clones::DestroyPlan::PromoteClone { closed } => {
            let clone_window = island_window(app, Some(&closed.id));
            if let Some(primary) = app.get_webview_window(ISLAND) {
                if let Some(window) = &clone_window {
                    if let (Ok(position), Ok(size)) = (window.outer_position(), window.outer_size()) {
                        let _ = primary.set_size(size);
                        let _ = primary.set_position(position);
                    }
                }
                if closed.dock.edge.is_some() { let _ = dock::restore_dock(&primary, &closed.dock); }
                emit_dock_changed(app, None, &closed.dock);
            }
            if let Some(window) = clone_window { window.destroy().map_err(|e| e.to_string())?; }
            refresh_tray_menu(app);
        }
    }
    let _ = app.emit("settings-changed", store.get());
    let _ = app.emit("menu-state-changed", ());
    Ok(())
}

/// 主面板首次使用时才创建；关闭只隐藏窗口并保留 WebView，后台采集与灵动岛继续运行。
fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN) {
        if MAIN_READY.load(Ordering::Acquire) {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        } else { let _ = show_startup(app); }
        return;
    }
    MAIN_READY.store(false, Ordering::Release);
    if let Err(error) = show_startup(app) { eprintln!("[启动] 无法显示启动窗口: {error}"); }
    let window = {
        WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("index.html".into()))
            .title("CC Usage")
            .inner_size(1128.0, 860.0)
            .min_inner_size(900.0, 600.0)
            .decorations(false)
            .visible(false)
            .center()
            .build()
    };
    match window {
        Ok(window) => {
            if let Some(saved) = app.try_state::<Cfg>().and_then(|cfg| cfg.0.get().main_window) {
                if let Err(error) = main_window::restore(&window, &saved) {
                    eprintln!("[窗口] 恢复主面板位置失败: {error}");
                }
            }
        },
        Err(error) => report_startup_error(app, format!("创建主面板失败：{error}")),
    }
}

fn show_main_with_intent(app: &tauri::AppHandle, intent: MainIntent) {
    if let Ok(mut pending) = MAIN_INTENT.lock() {
        *pending = Some(intent);
    }
    show_main(app);
    // 已存在窗口会立即消费；新窗口若此时尚未加载，会在挂载后主动 take。
    let _ = app.emit("main-intent", ());
}

fn start_initialization(app: tauri::AppHandle, data_dir: std::path::PathBuf) {
    if STARTUP_INITIALIZING.swap(true, Ordering::AcqRel) { return; }
    tauri::async_runtime::spawn_blocking(move || {
        let result = initialize_app(&app, &data_dir);
        STARTUP_INITIALIZING.store(false, Ordering::Release);
        match result {
            Ok(()) => {
                STARTUP_READY.store(true, Ordering::Release);
                // 灵动岛 WebView 可能早于后台数据库初始化完成，通知它重读已保存的选择和连接。
                let _ = app.emit("settings-changed", app.state::<Cfg>().0.get());
                let _ = app.emit("connections-changed", ());
                if STARTUP_REQUESTED.load(Ordering::Acquire) { show_main(&app); }
            }
            Err(error) => report_startup_error(&app, error),
        }
    });
}

fn initialize_app(app: &tauri::AppHandle, data_dir: &std::path::Path) -> Result<(), String> {
    if app.try_state::<Db>().is_none() {
        let conn = db::open(&data_dir.join("usage.db")).map_err(|error| error.to_string())?;
        effort_map::seed(&conn).map_err(|error| error.to_string())?;
        app.manage(Db(Arc::new(Mutex::new(conn))));
    }
    if app.try_state::<DbRead>().is_none() {
        // 写连接已完成建表与迁移，只读连接此后打开才能看到完整结构。
        let pool = ReadPool::open(&data_dir.join("usage.db")).map_err(|error| error.to_string())?;
        app.manage(DbRead(Arc::new(pool)));
    }

    // 恢复已有用户的停靠状态；首次安装先展示自由态灵动岛，再自动贴上边缘。
    {
        let cfg = app.state::<Cfg>().0.get();
        let first_intro = FIRST_INSTALL.swap(false, Ordering::AcqRel)
            && cfg.island_visible && cfg.dock_enabled && cfg.dock.edge.is_none();
        if let Some(w) = app.get_webview_window(ISLAND) {
            let _ = w.set_always_on_top(cfg.always_on_top);
            if first_intro {
                FIRST_INSTALL_INTRO.store(true, Ordering::Release);
                if let Err(error) = dock::place_free_top_center(&w) {
                    eprintln!("[启动] 灵动岛首次居中失败: {error}");
                    let _ = w.center();
                }
            }
        }
        if let (true, Some(edge), Some(w)) = (cfg.dock_enabled, cfg.dock.edge, app.get_webview_window(ISLAND)) {
            let _ = edge;
            if let Some(monitor) = dock::restore_dock(&w, &cfg.dock) {
                if cfg.dock.monitor.as_deref() != Some(&monitor) {
                    app.state::<Cfg>().0.update(|settings| settings.dock.monitor = Some(monitor));
                }
            }
        }
        if let Some(w) = app.get_webview_window(ISLAND) {
            if cfg.island_visible { let _ = w.show(); }
            else { let _ = w.hide(); }
        }
        // 重建上次运行时开启的分身；各自按保存的停靠或自由态位置落位。
        for clone in &cfg.island_clones {
            match ensure_island_window_for(app, Some(&clone.id)) {
                Ok(w) => { if cfg.island_visible { let _ = w.show(); } }
                Err(error) => eprintln!("[启动] 重建灵动岛分身 {} 失败: {error}", clone.id),
            }
        }
        if first_intro {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(5)).await;
                if !FIRST_INSTALL_INTRO.swap(false, Ordering::AcqRel) { return; }
                let settings = app.state::<Cfg>().0.get();
                if settings.island_visible && settings.dock_enabled && settings.dock.edge.is_none() {
                    island_position(app, Some(dock::Edge::Top), None);
                }
            });
        }
    }

    if app.tray_by_id("main-tray").is_none() {
        TrayIconBuilder::with_id("main-tray")
            .icon({
                let settings = app.state::<Cfg>().0.get();
                tray_quota_icon(app, &tray_summary::current_state(app), settings.island_kind == "auth")
            })
            .show_menu_on_left_click(false)
            .on_tray_icon_event(|tray, event| {
                match &event {
                    TrayIconEvent::Enter { position, .. } => tray_summary::enter(tray.app_handle(), position.x, position.y),
                    TrayIconEvent::Leave { .. } | TrayIconEvent::Click { .. } | TrayIconEvent::DoubleClick { .. } => tray_summary::close(tray.app_handle()),
                    _ => {}
                }
                if matches!(event, TrayIconEvent::Click { button: MouseButton::Right, button_state: MouseButtonState::Up, .. }) {
                    if let Err(error) = context_menu::open(tray.app_handle(), false, None) { eprintln!("[菜单] {error}"); }
                    return;
                }
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event {
                    let app = tray.app_handle();
                    show_main_with_intent(
                        app,
                        MainIntent::Overview { settings: app.state::<Cfg>().0.get() },
                    );
                }
            })
            .build(app)
            .map_err(|error| error.to_string())?;
    }

    // 后台采集与代理在数据库和托盘就位后启动，不阻塞启动窗口绘制。
    if !validation::isolated() { watcher::start(app.clone()); }
    if !validation::isolated() { watcher::start_config_sync(app.clone()); }
    if !validation::isolated() { proxy::startup(app); }

    if let Some(days) = app.state::<Cfg>().0.get().retention_days {
        if let Ok(cutoff) = retention_cutoff(days) {
            let database = app.state::<Db>().0.clone();
            let cleanup_app = app.clone();
            tauri::async_runtime::spawn(async move {
                match with_database(database, move |conn| {
                    db::cleanup_history(conn, cutoff).map_err(|error| error.to_string())
                }).await {
                    Ok(result) if result.requests > 0 => {
                        println!("[数据] 自动清理 {} 条过期请求", result.requests);
                        let _ = cleanup_app.emit("refresh-requested", ());
                    }
                    Ok(_) => {}
                    Err(error) => eprintln!("[数据] 自动清理失败：{error}"),
                }
            });
        }
    }

    Ok(())
}

/// 窗口操作成功后再同步设置、前端和托盘，避免显示状态与实际窗口脱节。
/// 显示 / 隐藏是全局开关：本体与全部分身一起动。
fn set_island_visible(app: &tauri::AppHandle, visible: bool) -> Result<(), String> {
    if !visible { FIRST_INSTALL_INTRO.store(false, Ordering::Release); }
    if visible {
        for id in island_ids(&app.state::<Cfg>().0.get()) {
            let w = ensure_island_window_for(app, id.as_deref())?;
            // show 不会解除 Windows 的最小化状态；灵动岛也没有任务栏恢复入口。
            w.unminimize().map_err(|e| e.to_string())?;
            w.show().map_err(|e| e.to_string())?;
            let _ = w.set_focus();
        }
    } else {
        for w in island_windows(app) { w.hide().map_err(|e| e.to_string())?; }
    }
    let next = app.state::<Cfg>().0.update(|s| s.island_visible = visible);
    let _ = app.emit("settings-changed", next);
    refresh_tray_menu(app);
    Ok(())
}

#[tauri::command]
async fn open_main_panel(app: tauri::AppHandle) {
    // Windows WebView2 不允许在同步 IPC 回调里创建新窗口，否则会等待自身事件循环。
    let settings = app.try_state::<Cfg>().map(|cfg| cfg.0.get()).unwrap_or_default();
    show_main_with_intent(&app, MainIntent::Overview { settings });
}

fn toggle_island(app: &tauri::AppHandle) -> Result<(), String> {
    // 窗口缺失时视为隐藏并重建；最小化窗口也恢复，避免用户没有任务栏入口。
    let visible = match app.get_webview_window(ISLAND) {
        Some(window) => !window.is_visible().map_err(|e| e.to_string())? || window.is_minimized().map_err(|e| e.to_string())?,
        None => true,
    };
    set_island_visible(app, visible)
}

/// 重置窗口位置（§2.6）：把灵动岛拉回主显示器、解除停靠，
/// 主面板恢复默认尺寸与位置，同时恢复灵动岛的可见状态。
fn reset_window_layout(app: &tauri::AppHandle) {
    FIRST_INSTALL_INTRO.store(false, Ordering::Release);
    app.state::<Cfg>().0.update(|settings| settings.main_window = None);
    if let Ok(mut placement) = MAIN_PLACEMENT.lock() { *placement = None; }
    if let Some(w) = app.get_webview_window(ISLAND) {
        let next = app.state::<Cfg>().0.update(|s| {
            s.dock = Default::default();
            for clone in &mut s.island_clones { clone.dock = Default::default(); clone.position = None; }
        });
        let _ = app.emit("settings-changed", &next);
        for id in island_ids(&next) { emit_dock_changed(app, id.as_deref(), &Default::default()); }
        let _ = w.set_size(tauri::LogicalSize::new(428.0, 124.0));
        let _ = w.center();
        // 分身解除停靠后从本体位置依次向右下错开 24px，避免叠在一起看不见。
        let origin = w.outer_position().ok();
        let scale = w.scale_factor().unwrap_or(1.0);
        for (index, clone) in next.island_clones.iter().enumerate() {
            let Some(cw) = island_window(app, Some(&clone.id)) else { continue };
            let _ = cw.set_size(tauri::LogicalSize::new(428.0, 124.0));
            if let Some(position) = origin {
                let offset = ((index as f64 + 1.0) * 24.0 * scale).round() as i32;
                let _ = cw.set_position(PhysicalPosition::new(position.x + offset, position.y + offset));
                let _ = dock::resize_free(&cw, dock::FREE_W, dock::FREE_H);
            }
            remember_clone_position(app, &clone.id, &cw);
        }
        if let Err(e) = set_island_visible(app, true) {
            eprintln!("[窗口] 恢复灵动岛失败: {e}");
        }
    }
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.set_size(tauri::LogicalSize::new(1128.0, 860.0));
        let _ = w.center();
        if let Ok(mut placement) = MAIN_PLACEMENT.lock() {
            *placement = main_window::capture(&w);
        }
    }
}

/// 平台是否已接入（未接入的在菜单里禁用并标「待接入」）
fn platform_name(id: &str) -> &str {
    platforms::get(id).map(|p|p.name.as_str()).unwrap_or(id)
}

/// 系统托盘右键菜单 —— 结构与文案见需求文档 §2.6
///
/// 勾选项用 CheckMenuItem，状态取自持久化设置，与设置界面是同一个值。
fn refresh_tray_menu(app: &tauri::AppHandle) {
    update_tray_quota_icon(app, &tray_summary::current_state(app));
    let _ = app.emit("menu-state-changed", ());
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrayQuotaTone {
    Normal,
    Warning,
    Critical,
    Unknown,
}

fn tray_quota_summary(state: &quota::QuotaState) -> (TrayQuotaTone, Option<f64>) {
    let quota::QuotaState::Ok { windows, .. } = state else {
        return (TrayQuotaTone::Unknown, None);
    };
    let worst = windows
        .iter()
        .filter_map(|window| window.used_percent)
        .filter(|value| value.is_finite())
        .fold(None::<f64>, |current, value| Some(current.map_or(value, |old| old.max(value))))
        .map(|value| value.clamp(0.0, 100.0));
    match worst {
        Some(value) if value >= 90.0 => (TrayQuotaTone::Critical, Some(value)),
        Some(value) if value >= 75.0 => (TrayQuotaTone::Warning, Some(value)),
        Some(value) => (TrayQuotaTone::Normal, Some(value)),
        None => (TrayQuotaTone::Unknown, None),
    }
}

fn tray_quota_color(state: &quota::QuotaState, is_auth: bool) -> [u8; 3] {
    let (tone, _) = tray_quota_summary(state);
    // API Key 没有订阅额度窗口，图标只表达“已配置/可用”的绿色状态；
    // 橙、红、灰等额度状态只对 Auth 连接有意义。
    let tone = if is_auth { tone } else { TrayQuotaTone::Normal };
    match tone {
        TrayQuotaTone::Normal => [6, 193, 103],
        TrayQuotaTone::Warning => [217, 133, 0],
        TrayQuotaTone::Critical => [220, 38, 38],
        TrayQuotaTone::Unknown => [154, 160, 170],
    }
}

/// 托盘图标按系统实际索取的尺寸原生渲染（SM_CXSMICON = 16 × DPI/96）。
/// 缩放倍数从任一存活窗口读取；窗口全关时按 100% 兜底。
fn tray_scale_factor(app: &tauri::AppHandle) -> f64 {
    app.get_webview_window(MAIN)
        .or_else(|| app.get_webview_window(ISLAND))
        .and_then(|window| window.scale_factor().ok())
        .unwrap_or(1.0)
}

fn tray_quota_icon(app: &tauri::AppHandle, state: &quota::QuotaState, is_auth: bool) -> Image<'static> {
    let color = tray_quota_color(state, is_auth);
    let size = tray_icon::output_size(tray_scale_factor(app));
    tray_icon::badge(&tray_icon::base(size), color)
}

/// 上次设置到托盘的 (颜色, 像素尺寸)。每次额度返回都会调用更新，
/// 图标内容没变时不重绘、不重复调用系统托盘接口。
static TRAY_ICON_SHOWN: Mutex<Option<([u8; 3], usize)>> = Mutex::new(None);

fn update_tray_quota_icon(app: &tauri::AppHandle, _state: &quota::QuotaState) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let state = tray_summary::current_state(app);
        let is_auth = app.state::<Cfg>().0.get().island_kind == "auth";
        let look = (tray_quota_color(&state, is_auth), tray_icon::output_size(tray_scale_factor(app)));
        let Ok(mut shown) = TRAY_ICON_SHOWN.lock() else { return };
        if *shown == Some(look) {
            return;
        }
        if tray.set_icon(Some(tray_icon::badge(&tray_icon::base(look.1), look.0))).is_ok() {
            *shown = Some(look);
        }
    }
}

#[cfg(test)]
mod tray_icon_tests {
    use super::*;

    fn quota(percentages: &[Option<f64>]) -> quota::QuotaState {
        quota::QuotaState::Ok {
            windows: percentages.iter().enumerate().map(|(index, used_percent)| quota::QuotaWindow {
                key: format!("window-{index}"),
                window_name: format!("窗口 {index}"),
                used_percent: *used_percent,
                amount_text: None,
                remaining_text: None,
                resets_at: None,
            }).collect(),
            plan: None,
        }
    }

    #[test]
    fn tray_uses_the_worst_available_quota_window() {
        assert_eq!(tray_quota_summary(&quota(&[Some(24.0), Some(78.0)])), (TrayQuotaTone::Warning, Some(78.0)));
        assert_eq!(tray_quota_summary(&quota(&[Some(91.0), Some(12.0)])), (TrayQuotaTone::Critical, Some(91.0)));
    }

    #[test]
    fn tray_keeps_unknown_distinct_from_zero_usage() {
        assert_eq!(tray_quota_summary(&quota(&[None])), (TrayQuotaTone::Unknown, None));
        assert_eq!(tray_quota_summary(&quota(&[Some(0.0)])), (TrayQuotaTone::Normal, Some(0.0)));
    }

    #[test]
    fn api_key_tray_icon_stays_green_for_all_quota_tones() {
        let warning = quota(&[Some(78.0)]);
        let critical = quota(&[Some(95.0)]);
        assert_eq!(tray_quota_color(&warning, false), [6, 193, 103]);
        assert_eq!(tray_quota_color(&critical, false), [6, 193, 103]);
    }
}

fn finish_tray_refresh_after_cooldown(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        // 同一轮前端网络查询尚未返回时不允许连续触发，也避免贴着平台限流重试。
        std::thread::sleep(std::time::Duration::from_secs(5));
        TRAY_REFRESHING.store(false, Ordering::Relaxed);
        refresh_tray_menu(&app);
    });
}

/// 托盘菜单事件。灵动岛可见性与平台选择都写回持久化设置，
/// 并通过事件通知前端，保证托盘与设置界面显示同一个值。
/// `island` 是右键菜单所属的岛（None 为本体或托盘）；位置与分身操作都作用于它。
fn handle_tray_menu(app: &tauri::AppHandle, id: &str, island: Option<&str>) -> Result<(), String> {
    match id {
        "open_main" => show_main_with_intent(
            app,
            MainIntent::Overview { settings: app.state::<Cfg>().0.get() },
        ),
        "island_refresh" => {
            if TRAY_REFRESHING.swap(true, Ordering::Relaxed) { return Ok(()); }
            refresh_tray_menu(app);
            let _ = app.emit("island-refresh", ());
            let _ = app.emit("refresh-requested", ());
            finish_tray_refresh_after_cooldown(app.clone());
        }
        "topmost" => { let _ = island_topmost(app.clone(), !app.state::<Cfg>().0.get().always_on_top); }
        "pos_free" => island_position(app.clone(), None, island.map(str::to_string)),
        "pos_top" => island_position(app.clone(), Some(dock::Edge::Top), island.map(str::to_string)),
        "pos_bottom" => island_position(app.clone(), Some(dock::Edge::Bottom), island.map(str::to_string)),
        "pos_left" => island_position(app.clone(), Some(dock::Edge::Left), island.map(str::to_string)),
        "pos_right" => island_position(app.clone(), Some(dock::Edge::Right), island.map(str::to_string)),
        "clone_create" => create_island_clone(app, island)?,
        "clone_destroy" => destroy_island(app, island)?,
        "refresh" => {
            if TRAY_REFRESHING.swap(true, Ordering::Relaxed) { return Ok(()); }
            refresh_tray_menu(app);
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let result = with_database(app.state::<Db>().0.clone(), |conn| {
                    Ok(collector::scan(conn))
                }).await;
                match result {
                    Ok(r) => println!("[托盘] 立即刷新：新增 {} 条", r.records_inserted),
                    Err(e) => eprintln!("[托盘] 刷新失败: {e}"),
                }
                let _ = app.emit("island-refresh", ());
                let _ = app.emit("refresh-requested", ());
                finish_tray_refresh_after_cooldown(app);
            });
        }
        "toggle_island" => {
            if let Err(e) = toggle_island(app) {
                eprintln!("[托盘] 切换灵动岛失败: {e}");
            }
        }
        "dnd" => {
            if let Some(c) = app.try_state::<Cfg>() {
                let next = c.0.update(|s| s.dnd = !s.dnd);
                // 免打扰只影响展示，采集继续
                let _ = app.emit("dnd-changed", next.dnd);
                let _ = app.emit("settings-changed", &next);
            }
            refresh_tray_menu(app);
        }
        "reset_layout" => reset_window_layout(app),
        "about" => {
            show_main_with_intent(app, MainIntent::About {
                version: env!("CARGO_PKG_VERSION").into(),
            });
        }
        "open_source_settings" => {
            show_main_with_intent(app, MainIntent::Settings { section: "island".into() });
        }
        "quit" => app.exit(0),
        other => {
            if let Some(p) = other.strip_prefix("plat_") {
                if let Err(error) = set_island_platform(app.clone(), app.state::<Cfg>(), p.to_string()) {
                    eprintln!("[托盘] 平台切换失败：{error}");
                }
            }
        }
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            activate_existing_instance(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            tray_summary::tray_summary_get, tray_summary::tray_summary_fit,
            scan_local_sessions,
            collection_status,
            data_info,
            open_data_directory,
            autostart_status,
            set_autostart,
            set_silent_startup,
            resize_island,
            live_usage,
            log_front,
            connection_kind,
            token_totals,
            usage_breakdown,
            source_metrics,
            list_models,
            usage_trend,
            request_log,
            list_connections,
            add_connection,
            update_connection,
            enable_connection,
            reveal_connection_secret,
            test_connection,
            get_effort_options,
            refresh_effort_levels,
            remove_connection,
            set_connection_paused,
            fetch_credentials,
            replace_api_key,
            rename_connection,
            discover_connections,
            read_local_connections,
            data_files::import_data_file,
            data_files::export_data_file,
            get_settings,
            list_sources,
            set_island_platform,
            set_island_source,
            set_island_connection,
            set_dnd,
            set_proxy,
            proxy_status,
            dock_hover,
            dock_release,
            dock_undock,
            cost_estimate,
            connection_quota,
            connection_api_usage,
            local_codex_quota,
            local_grok_quota,
            local_zcode_quota,
            get_plan_query_status,
            set_plan_query,
            connection_balance,
            island_menu,
            context_menu::menu_action,
            context_menu::menu_fit,
            context_menu::menu_show,
            context_menu::menu_close,
            context_menu::menu_refreshing,
            island_drag,
            main_drag,
            island_position,
            island_topmost,
            set_dock_enabled,
            set_theme,
            set_display_preferences,
            set_balance_alert,
            set_retention_days,
            cleanup_preview,
            cleanup_history,
            export_data,
            import_data,
            open_main_panel,
            main_panel_ready,
            startup_error,
            retry_startup,
            quit_startup,
            take_main_intent,
            fetch_remote_models,
            updater::check_app_update_available,
            updater::download_app_update,
            updater::install_downloaded_update_and_restart
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            app.manage(updater::UpdateDownloadState::default());
            let automatic = std::env::args_os().any(|arg| arg == "--autostart");
            if !automatic {
                STARTUP_REQUESTED.store(true, Ordering::Release);
                let _ = show_startup(&handle);
            }

            let data_dir = match data_paths(&handle) {
                Ok((path, _)) => path,
                Err(error) => {
                    report_startup_error(&handle, error);
                    return Ok(());
                }
            };
            let first_install = !data_dir.join("settings.json").exists()
                && !data_dir.join("usage.db").exists();
            FIRST_INSTALL.store(first_install, Ordering::Release);
            app.manage(Cfg(settings::Store::load(&data_dir)));
            app.manage(settings::PlanQueryStore::load(&data_dir));
            let startup_plan = startup::plan(automatic, app.state::<Cfg>().0.get().silent_startup, first_install);
            STARTUP_REQUESTED.store(startup_plan.show_main, Ordering::Release);
            if startup_plan.show_main { let _ = show_startup(&handle); }
            start_initialization(handle, data_dir);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == MAIN {
                record_main_window_event(window, event);
                // 关闭只隐藏、保留 WebView，托盘再次打开时秒开，不再重建主面板并走启动窗。
                // 未就绪的主面板（加载失败等）照常关闭，下次打开会重新创建。
                if let WindowEvent::CloseRequested { api, .. } = event {
                    if MAIN_READY.load(Ordering::Acquire) {
                        api.prevent_close();
                        if let Err(error) = window.hide() {
                            eprintln!("[窗口] 隐藏主面板失败: {error}");
                        }
                        return;
                    }
                }
            }
            if island_clones::is_island_label(window.label()) {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    // 无边框灵动岛没有用户可点的关闭入口；系统关闭请求多见于应用退出或开发热重载。
                    // 只隐藏当前原生窗口，不把用户的“显示灵动岛”偏好错误持久化为 false。
                    if let Err(error) = window.hide() {
                        eprintln!("[窗口] 隐藏灵动岛失败: {error}");
                    }
                    return;
                }
            }
            if island_clones::is_island_label(window.label())
                && matches!(event, WindowEvent::Moved(_) | WindowEvent::ScaleFactorChanged { .. })
            {
                // Windows 回调中不能同步等待窗口查询；交给后台线程并按窗口合并密集移动事件。
                static PENDING: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
                let label = window.label().to_string();
                let queued = PENDING.lock().map(|mut set| set.insert(label.clone())).unwrap_or(false);
                if queued {
                    let app = window.app_handle().clone();
                    let scale_changed = matches!(event, WindowEvent::ScaleFactorChanged { .. });
                    tauri::async_runtime::spawn_blocking(move || {
                        if let (Some(webview), Some(store)) = (app.get_webview_window(&label), app.try_state::<Cfg>()) {
                            let id = island_clones::id_from_label(&label).flatten();
                            let cfg = store.0.get();
                            let dock_state = island_dock(&cfg, id.as_deref());
                            if dock::DRAGGING.load(Ordering::Relaxed) {
                                let hint = if cfg.dock_enabled { dock::hover_hint(&webview) } else { None };
                                let _ = app.emit("dock-hint", DockHintEvent { island: id.clone(), hint });
                            } else if dock_state.edge.is_some()
                                && (scale_changed || !dock::monitor_exists(&webview, dock_state.monitor.as_deref())) {
                                if let Some(monitor) = dock::restore_dock(&webview, &dock_state) {
                                    let next = store.0.update(|settings| {
                                        island_clones::set_dock(settings, id.as_deref(), dock::DockState { monitor: Some(monitor), ..dock_state.clone() });
                                    });
                                    let _ = app.emit("settings-changed", next);
                                }
                            }
                        }
                        if let Ok(mut set) = PENDING.lock() { set.remove(&label); }
                    });
                }
            }
            // 主面板使用默认关闭行为销毁 WebView，释放图表、订阅与网络定时器。
        })
        .build(tauri::generate_context!())
        .expect("error while building CC Usage");
    app.run(|app, event| {
        // 正常退出前还原 CLI 直连配置，避免 CLI 指向已失效的本机代理端口；
        // 崩溃 / 强杀的残留由下次启动的 proxy::startup 自愈。
        if matches!(event, tauri::RunEvent::Exit { .. }) {
            if let Ok(placement) = MAIN_PLACEMENT.lock() {
                if let Some(placement) = placement.clone() {
                    persist_main_placement(app, placement);
                }
            }
            proxy::shutdown_and_restore(app);
        }
    });
}
