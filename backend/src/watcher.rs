//! 实时监听本地会话记录（§7.7）
//!
//! CLI 每收到一次响应就往 jsonl 追加一行，因此**文件写入即用量事件**。
//! 这里用 notify 监听目录，写入发生时才增量采集并向前端推事件——
//! 前端不再定时轮询，延迟从「最多一个轮询周期」降到「文件写入后几十毫秒」。
//!
//! 去抖：一次响应可能触发多次写事件（内容 + 元数据），
//! 因此收到事件后合并 DEBOUNCE 窗口内的抖动，再做一次采集。

use crate::db::{LiveUsage, Store};
use crate::{collector, Cfg, Db, DbRead};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// 事件名：前端 listen 这个名字即可拿到实时用量
pub const EVENT_LIVE: &str = "live-usage";

/// 写事件合并窗口。一次响应的多次写抖动合并成一次采集
const DEBOUNCE: Duration = Duration::from_millis(120);

/// 兜底心跳：监听失效（网络盘、权限、句柄耗尽）时仍能恢复，并续期运行中会话的存活判定。
/// 未变化的文件只比对大小与修改时间、不拿写锁，常态开销很小。
const HEARTBEAT: Duration = Duration::from_secs(10);

/// 每隔这么多次心跳整体重绑一次监听，覆盖底层句柄悄悄失效的情况；
/// 其余心跳只补绑新出现或此前绑定失败的目录，不反复拆装全部递归监听。
const FULL_REBIND_EVERY: u32 = 6;

const PLATFORMS: [&str; 8] = ["claude", "codex", "gemini", "grok", "zcode", "trae", "qoder", "workbuddy"];

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn claude_registry() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from)
        .or_else(|| home().map(|h| h.join(".claude")))
        .map(|root| root.join("sessions"))
}

/// 监听目标：Claude / Codex 会话目录、Claude 进程登记、Codex 标题索引与各本机来源目录。
fn watch_targets() -> Vec<(PathBuf, RecursiveMode)> {
    let Some(home) = home() else { return Vec::new() };
    let codex_home = crate::session_titles::codex_home().unwrap_or_else(|| home.join(".codex"));
    let mut targets = vec![
        (home.join(".claude").join("projects"), RecursiveMode::Recursive),
        (claude_registry().unwrap_or_else(|| home.join(".claude/sessions")), RecursiveMode::NonRecursive),
        (codex_home.join("sessions"), RecursiveMode::Recursive),
        (codex_home.join("session_index.jsonl"), RecursiveMode::NonRecursive),
    ];
    targets.extend(crate::native_sources::watch_roots().into_iter().map(|path| (path, RecursiveMode::Recursive)));
    targets
}

/// 绑定监听。`full` 时拆掉重绑全部目标（覆盖底层 watcher 丢失句柄）；
/// 否则只补绑尚未成功绑定、以及已被删除需要解绑的目标。`bound` 记录当前生效的目标。
fn rebind_sources(watcher: &mut RecommendedWatcher, bound: &mut HashSet<PathBuf>, full: bool, announce: bool) {
    for (path, mode) in watch_targets() {
        let exists = path.exists();
        if !full && exists == bound.contains(&path) {
            continue;
        }
        if bound.remove(&path) {
            let _ = watcher.unwatch(&path);
        }
        if !exists {
            continue;
        }
        match watcher.watch(&path, mode) {
            Ok(()) => {
                bound.insert(path.clone());
                if announce { println!("[实时] 已监听 {}", path.display()); }
            }
            Err(error) => eprintln!("[实时] 监听 {} 失败，将由心跳重试: {error}", path.display()),
        }
    }
}

/// 写事件路径是否值得采集：会话日志（jsonl）、Claude 进程登记，或本机来源目录里的
/// 数据文件。SQLite 的共享内存文件（-shm）在只读打开时也会变化，排除以免自我触发。
fn relevant(path: &Path, registry: Option<&Path>, native_roots: &[PathBuf]) -> bool {
    if path.to_string_lossy().ends_with("-shm") {
        return false;
    }
    path.extension().and_then(|value| value.to_str()) == Some("jsonl")
        || registry.is_some_and(|root| path == root || path.parent() == Some(root))
        || native_roots.iter().any(|root| path.starts_with(root))
}

/// 自 `since` 以来有新增或更正的平台。游标是全局事件序号，删除（如 Gemini 回退）
/// 只推进序号不留事件，此时无法定位平台，按全部平台处理。
fn changed_platforms(reader: &crate::ReadPool, since: Option<i64>, inserted: bool) -> BTreeSet<String> {
    let all = || PLATFORMS.iter().map(|platform| platform.to_string()).collect();
    let Some(since) = since else {
        return if inserted { all() } else { BTreeSet::new() };
    };
    let found = reader.with(|conn| -> rusqlite::Result<BTreeSet<String>> {
        let mut stmt = conn.prepare_cached("SELECT DISTINCT platform FROM usage_events WHERE id > ?1")?;
        let rows = stmt.query_map([since], |row| row.get::<_, String>(0))?;
        rows.collect()
    }).unwrap_or_default();
    if found.is_empty() && inserted { all() } else { found }
}

/// 采集一次并把最新实时用量推给前端。
/// `cursor` 为上次推送时的最大 id，只有真的有新增才发事件。
/// 采集按文件短暂持有写锁；实时快照走只读连接，不与界面查询和后续写入互相等待。
fn collect_and_emit(
    app: &AppHandle,
    cursor: &mut Option<i64>,
    had_active: &mut bool,
    last_today: &mut Option<Option<i64>>,
    changed_paths: Option<Vec<PathBuf>>,
) {
    let Some(state) = app.try_state::<Db>() else { return };
    let Some(reader) = app.try_state::<DbRead>() else { return };
    let mut writer: &std::sync::Mutex<rusqlite::Connection> = &state.0;

    let previous_cursor = *cursor;
    let scan = match changed_paths {
        Some(paths) => collector::scan_paths(&mut writer, paths),
        None => collector::scan(&mut writer),
    };
    crate::emit_collection_status(app, &scan);
    // 本地代理的计时可能在请求记录入库前到达，扫描后按去重键补写（无队列时零开销）
    writer.write(|conn| crate::proxy::flush_pending(conn));

    // 跟随灵动岛配置的平台（§7.3），与托盘子菜单同一个值
    let settings = app
        .try_state::<Cfg>()
        .map(|c| c.0.get())
        .unwrap_or_default();
    let platform = settings.island_platform;

    let mut usage: LiveUsage = match reader.0.with(|conn| crate::session_presence::live_usage(conn, &platform, *cursor)) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("[实时] 查询失败: {e}");
            return;
        }
    };

    let first = cursor.is_none();
    let advanced = usage.cursor != cursor.unwrap_or(-1);
    let active = !usage.active_sessions.is_empty();
    let today_changed = last_today.as_ref().is_some_and(|previous| *previous != usage.today_tokens);
    *cursor = Some(usage.cursor);
    *last_today = Some(usage.today_tokens);

    // 推送时机：首次填初值 / 游标前进（有新消耗）/ 有会话在跑（活跃窗口需要续期）/
    // 会话刚刚全部结束（前端据此收起数字，漏掉这条数字就会一直挂着）
    let should = first || advanced || active || *had_active || today_changed || scan.records_inserted > 0;
    *had_active = active;
    if !should {
        return;
    }
    // 游标是全局的：别的平台有新增也会前进。只有本平台真有变化，统计视图才需要重查；
    // 仅为续期运行中会话的心跳推送不触发统计重查。
    let changed = if advanced || scan.records_inserted > 0 {
        changed_platforms(&reader.0, previous_cursor, scan.records_inserted > 0)
    } else {
        BTreeSet::new()
    };
    usage.data_changed = first || today_changed || changed.contains(&platform);
    if advanced && !first {
        println!("[实时] +{} token → 前端", usage.delta_tokens);
    }
    let _ = app.emit(EVENT_LIVE, &usage);
    // 其他平台只推真正有变化的，供主面板对应视图刷新
    for other in changed.iter().filter(|other| **other != platform) {
        if let Ok(mut snapshot) = reader.0.with(|conn| crate::session_presence::live_usage(conn, other, previous_cursor)) {
            snapshot.data_changed = true;
            let _ = app.emit(EVENT_LIVE, &snapshot);
        }
    }
}

/// 在后台线程启动监听。调用后立即返回。
pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        // 容量 1 即可表达“需要扫描”；事件洪峰只合并信号，不线性堆积内存。
        let (tx, rx) = mpsc::sync_channel(1);
        // 路径集合与容量 1 的唤醒信号分离：信号被合并时路径仍会保留，
        // 去抖窗口结束后一次取走，避免事件洪峰漏掉第二个文件。
        let pending_paths = Arc::new(Mutex::new(HashSet::<PathBuf>::new()));
        let callback_paths = pending_paths.clone();
        let registry = claude_registry();
        let native_roots = crate::native_sources::watch_roots();

        // notify 的 watcher 必须在整个监听期间存活，故绑定到本线程局部变量
        let mut watcher: Option<RecommendedWatcher> =
            match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    let relevant_kind = matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_));
                    let relevant_path = event.paths.iter().any(|path| relevant(path, registry.as_deref(), &native_roots));
                    if relevant_kind && relevant_path {
                        if let Ok(mut pending) = callback_paths.lock() {
                            // 进程登记只触发一次实时快照重算，不作为待采集文件
                            pending.extend(event.paths.into_iter().filter(|path| {
                                relevant(path, None, &native_roots)
                            }));
                        }
                        let _ = tx.try_send(());
                    }
                }
            }) {
                Ok(w) => Some(w),
                Err(e) => {
                    eprintln!("[实时] 无法创建监听器，回落到心跳采集: {e}");
                    None
                }
            };

        let mut bound = HashSet::new();
        if let Some(watcher) = watcher.as_mut() {
            rebind_sources(watcher, &mut bound, true, true);
        }
        let mut heartbeats = 0u32;

        let mut cursor: Option<i64> = None;
        let mut had_active = false;
        let mut last_today = None;
        // 先推一次初值，灵动岛打开就有数据，不用等第一次写入
        collect_and_emit(&app, &mut cursor, &mut had_active, &mut last_today, None);

        loop {
            // 等第一个写事件；没有事件时按心跳兜底
            let got = match rx.recv_timeout(HEARTBEAT) {
                Ok(_) => true,
                Err(mpsc::RecvTimeoutError::Timeout) => false,
                // 监听器创建失败或运行中断开时继续按心跳补采，不能退出后台采集。
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    std::thread::sleep(HEARTBEAT);
                    false
                }
            };

            if got {
                // 去抖：把紧随其后的抖动一起吃掉，只采集一次
                let deadline = Instant::now() + DEBOUNCE;
                while let Some(left) = deadline.checked_duration_since(Instant::now()) {
                    if rx.recv_timeout(left).is_err() {
                        break;
                    }
                }
            } else if let Some(watcher) = watcher.as_mut() {
                heartbeats = heartbeats.wrapping_add(1);
                rebind_sources(watcher, &mut bound, heartbeats % FULL_REBIND_EVERY == 0, false);
            }

            let changed_paths = if got {
                Some(
                    pending_paths
                        .lock()
                        .map(|mut pending| pending.drain().collect())
                        .unwrap_or_default(),
                )
            } else {
                None
            };
            collect_and_emit(
                &app,
                &mut cursor,
                &mut had_active,
                &mut last_today,
                changed_paths,
            );
        }
    });
}


/// 连接相关本机配置：CC Switch 切换后写这些文件，写入即同步信号。
fn config_targets() -> Vec<PathBuf> {
    let Some(home) = home() else { return vec![] };
    let claude_dir = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"));
    let codex_home = crate::session_titles::codex_home().unwrap_or_else(|| home.join(".codex"));
    vec![
        claude_dir.join("settings.json"),
        claude_dir.join(".credentials.json"),
        codex_home.join("auth.json"),
        codex_home.join("config.toml"),
    ]
}

/// 同步一次连接；有实际变更才推事件（前端重载列表并立刻重查额度）。
fn sync_connections_and_emit(app: &AppHandle) {
    let Some(state) = app.try_state::<Db>() else { return };
    let Ok(mut conn) = state.0.lock() else { return };
    let synced = crate::connections::sync_from_local(&mut conn);
    // 代理跟随配置时可能要写库（proxy::on_config_changed），先释放写锁
    drop(conn);
    match synced {
        Ok(true) => {
            println!("[连接] 本机配置有变化，已自动更新连接并触发重查");
            let _ = app.emit("connections-changed", ());
            let _ = app.emit("refresh-requested", ());
        }
        Ok(false) => {}
        Err(error) => eprintln!("[连接] 本机配置同步失败: {error}"),
    }
    // 代理开着时跟随外部改动：外部换网关 → 更新保存的上游并重写代理地址；切回官方 → 放弃接管
    crate::proxy::on_config_changed(app);
}

/// 监听 CC Switch 应用到本机的配置文件，写入即自动同步连接（默认常开）。
/// 启动时先对齐一次，覆盖「先切 Key 后开应用」的顺序。
pub fn start_config_sync(app: AppHandle) {
    std::thread::spawn(move || {
        sync_connections_and_emit(&app);

        let (tx, rx) = mpsc::sync_channel::<()>(1);
        let mut watcher = match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                let relevant = matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_));
                if relevant {
                    let _ = tx.try_send(());
                }
            }
        }) {
            Ok(w) => w,
            Err(error) => {
                eprintln!("[连接] 无法创建配置监听器，自动同步退化为仅启动时对齐: {error}");
                return;
            }
        };

        let bind = |watcher: &mut RecommendedWatcher| {
            for path in config_targets() {
                let _ = watcher.unwatch(&path);
                if path.exists() {
                    if let Err(error) = watcher.watch(&path, RecursiveMode::NonRecursive) {
                        eprintln!("[连接] 监听 {} 失败: {error}", path.display());
                    }
                }
            }
        };
        bind(&mut watcher);

        loop {
            if rx.recv().is_err() { return; }
            // 去抖：一次切换可能触发多次写事件（写入 + 元数据）
            let deadline = Instant::now() + DEBOUNCE;
            while let Some(left) = deadline.checked_duration_since(Instant::now()) {
                if rx.recv_timeout(left).is_err() { break; }
            }
            sync_connections_and_emit(&app);
            // 文件可能晚于启动才出现（如首次 CLI 登录），每次同步后重绑一次
            bind(&mut watcher);
        }
    });
}
