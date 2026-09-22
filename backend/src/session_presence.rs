//! 会话日志的运行标记须同时得到实时写入者的存活确认。
use crate::db::{self, LiveUsage};
use std::path::{Path, PathBuf};

/// 锁探测结果。区分“锁文件不存在”与“锁文件存在但当前未持锁”是本模块的关键：
/// 前者才是可判定的孤儿证据，后者在思考/等待/压缩等静默期很常见，不得误判为结束。
#[derive(Debug, PartialEq, Eq)]
enum Presence {
    /// 锁文件存在且正被独占锁定：写入者确认为存活。
    Held,
    /// 锁文件存在但此刻读不到字节锁：写入者仍在（文件未被删除），只是暂时未写。
    NotHeld,
    /// 锁文件不存在：写入者进程已退出并清理，属于历史遗留（孤儿）。
    Missing,
    /// 无法确认（非 Windows、路径非法、或读取失败）。
    Unknown,
}

fn probe_lock(path: &Path) -> Presence {
    #[cfg(windows)]
    {
        use std::io::Read;
        // Windows 对被独占锁覆盖的区间返回 ERROR_LOCK_VIOLATION (33)。
        // 只读一个字节，不创建、不删除、不尝试取得锁，避免干扰 Codex 写入者。
        let mut file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Presence::Missing,
            Err(_) => return Presence::Unknown,
        };
        match file.read(&mut [0u8; 1]) {
            Err(error) if error.raw_os_error() == Some(33) => Presence::Held,
            Ok(_) => Presence::NotHeld,
            Err(_) => Presence::Unknown,
        }
    }
    #[cfg(not(windows))]
    { let _ = path; Presence::Unknown }
}

fn presence(root: Option<&Path>, session: &str) -> Presence {
    // 日志里的标识只用于匹配锁，不能成为任意文件路径。
    if session.len() != 36 || !session.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        return Presence::Unknown;
    }
    root.map(|root| probe_lock(&root.join(format!("{session}.lock")))).unwrap_or(Presence::Unknown)
}

fn filter_with(usage: &mut LiveUsage, probe: impl Fn(&str) -> Presence) {
    if !matches!(usage.platform.as_str(), "codex" | "claude") { return; }
    // 会话“结束”的唯一权威信号是日志生命周期（task_complete / final_answer /
    // turn_aborted），它在 db::live_usage 里已按 activity_state = 'running' 过滤。
    //
    // 锁探测在这里只承担“剔除孤儿”的职责，且只认一个强证据：锁文件已不存在
    // （Missing，写入者进程早已退出并清理）。绝不能把“锁文件仍在、只是此刻读不到
    // 字节锁”（NotHeld）或“无法确认”（Unknown）当作结束证据——思考、等待用户输入、
    // 压缩上下文这些静默阶段写入者会暂时释放字节范围锁，但会话生命周期仍为 running，
    // 若据此删除会把活跃会话误判为结束、导致灵动岛 Token 中断。
    usage.active_sessions.retain(|session| probe(&session.session_id) != Presence::Missing);
    // 与 db::live_usage 同口径：运行合计只统计 running，失败会话仅保留展示。
    usage.running_tokens = db::sum_running_tokens(&usage.active_sessions);
}

/// 只查询进程句柄，不终止或注入 CLI。权限不足不能当作已退出。
fn process_alive(pid: u32) -> Option<bool> {
    if pid == 0 { return Some(false); }
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        #[link(name = "kernel32")]
        extern "system" {
            fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
            fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
            fn CloseHandle(handle: *mut c_void) -> i32;
        }
        // SYNCHRONIZE，零超时查询：退出进程为 signaled，存活进程为 timeout。
        let handle = unsafe { OpenProcess(0x0010_0000, 0, pid) };
        if handle.is_null() {
            return if std::io::Error::last_os_error().raw_os_error() == Some(87) { Some(false) } else { None };
        }
        let state = unsafe { WaitForSingleObject(handle, 0) };
        unsafe { CloseHandle(handle); }
        match state { 0 => Some(false), 258 => Some(true), _ => None }
    }
    #[cfg(not(windows))]
    { None }
}

/// Claude 的 sessions/<pid>.json 由 CLI 管理；登记存在还须确认进程未退出。
/// 未提供登记目录或读取失败时返回未知，避免把无法确认误判成关闭。
fn claude_sessions(root: &Path, alive: impl Fn(u32) -> Option<bool>) -> Option<std::collections::HashSet<String>> {
    let entries = std::fs::read_dir(root).ok()?;
    let mut ids = std::collections::HashSet::new();
    for entry in entries {
        let path = entry.ok()?.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") { continue; }
        let Some(pid) = path.file_stem().and_then(|s| s.to_str()).and_then(|s| s.parse::<u32>().ok()) else { continue; };
        if alive(pid) == Some(false) { continue; }
        let data = match std::fs::read(&path) {
            Ok(data) => data,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return None,
        };
        let record: serde_json::Value = serde_json::from_slice(&data).ok()?;
        let id = record.get("sessionId")?.as_str()?;
        if !id.is_empty() { ids.insert(id.to_string()); }
    }
    Some(ids)
}

pub fn live_usage(conn: &rusqlite::Connection, platform: &str, since: Option<i64>) -> rusqlite::Result<LiveUsage> {
    let mut usage = db::live_usage(conn, platform, since)?;
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from);
    if platform == "claude" {
        let root = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from)
            .or_else(|| home.as_ref().map(|h| h.join(".claude")));
        if let Some(ids) = root.and_then(|r| claude_sessions(&r.join("sessions"), process_alive)) {
            filter_with(&mut usage, |id| if ids.contains(id) { Presence::Held } else { Presence::Missing });
        }
    } else if platform == "codex" {
        let root = crate::session_titles::codex_home().map(|h| h.join("thread-writer-locks"));
        filter_with(&mut usage, |id| presence(root.as_deref(), id));
    }
    Ok(usage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn closed_claude_session_disappears_even_with_recent_tokens() {
        let conn = db::open(Path::new(":memory:")).unwrap();
        let mut usage = db::live_usage(&conn, "claude", None).unwrap();
        usage.today_tokens = Some(235_300);
        usage.active_sessions.push(db::ActiveSession {
            session_id: "closed".into(), title: "closed session".into(),
            delta_tokens: 39_100, last_seen_ms: chrono::Local::now().timestamp_millis(),
            started_at_ms: None, running_tokens: None, state: "recent".into(),
        });
        filter_with(&mut usage, |_| Presence::Missing);
        assert!(usage.active_sessions.is_empty(), "recent writes must not keep a closed Claude session visible");
        assert_eq!(usage.today_tokens, Some(235_300), "history must remain unchanged");
    }

    #[test]
    fn claude_registry_excludes_exited_processes_and_removed_sessions() {
        let root = std::env::temp_dir().join(format!("island-claude-registry-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        for (pid, id) in [(1, "open"), (2, "exited"), (3, "unknown")] {
            std::fs::write(root.join(format!("{pid}.json")), format!("{{\"sessionId\":\"{id}\"}}")).unwrap();
        }
        let ids = claude_sessions(&root, |pid| match pid { 1 => Some(true), 2 => Some(false), _ => None }).unwrap();
        assert!(ids.contains("open"));
        assert!(ids.contains("unknown"));
        assert!(!ids.contains("exited"));
        for pid in 1..=3 { std::fs::remove_file(root.join(format!("{pid}.json"))).unwrap(); }
        assert!(claude_sessions(&root, |_| Some(true)).unwrap().is_empty());
        std::fs::remove_dir(&root).unwrap();
        assert!(claude_sessions(&root, |_| Some(true)).is_none());
        #[cfg(windows)]
        assert_eq!(process_alive(std::process::id()), Some(true));
        assert_eq!(process_alive(0), Some(false));
    }

    #[test]
    fn orphan_logs_are_excluded_but_silent_owned_turn_is_kept() {
        let conn = db::open(Path::new(":memory:")).unwrap();
        for id in ["orphan", "waiting", "unreadable", "silent"] {
            db::set_session_activity(&conn, "codex", id, "running", 1).unwrap();
        }
        let mut usage = db::live_usage(&conn, "codex", None).unwrap();
        for session in &mut usage.active_sessions {
            session.started_at_ms = Some(1);
            session.running_tokens = Some(if session.session_id == "waiting" { 120 } else { 900 });
        }
        // Held（确认真活）、NotHeld（静默期：思考/等待/压缩）与 Unknown（无法确认）
        // 都保留；只有 Missing（锁文件已消失的孤儿）被剔除。
        filter_with(&mut usage, |id| match id {
            "waiting" => Presence::Held,
            "silent" => Presence::NotHeld,
            "orphan" => Presence::Missing,
            _ => Presence::Unknown,
        });
        assert_eq!(usage.active_sessions.len(), 3, "应保留 Held/NotHeld/Unknown，仅剔除 Missing 孤儿");
        let ids: Vec<&str> = usage.active_sessions.iter().map(|s| s.session_id.as_str()).collect();
        assert!(ids.contains(&"waiting"));
        assert!(ids.contains(&"silent"));
        assert!(ids.contains(&"unreadable"));
        assert!(!ids.contains(&"orphan"));
        // 全部孤儿剔除后 active_sessions 为空 → running_tokens 归 None。
        filter_with(&mut usage, |_| Presence::Missing);
        assert!(usage.active_sessions.is_empty());
        assert_eq!(usage.running_tokens, None);
        let history: i64 = conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0)).unwrap();
        assert_eq!(history, 4);
    }

    #[test]
    fn failed_entries_stay_but_running_sum_counts_only_running() {
        let conn = db::open(Path::new(":memory:")).unwrap();
        let now = chrono::Local::now().timestamp_millis();
        db::set_session_activity(&conn, "codex", "live", "running", now).unwrap();
        db::set_session_activity(&conn, "codex", "errored", "failed", now - 5_000).unwrap();
        let mut usage = db::live_usage(&conn, "codex", None).unwrap();
        assert_eq!(usage.active_sessions.len(), 2, "前置：窗口内的失败会话已在列表中");
        for session in &mut usage.active_sessions {
            session.started_at_ms = Some(1);
            session.running_tokens = Some(100);
        }
        filter_with(&mut usage, |_| Presence::Held);
        assert_eq!(usage.active_sessions.len(), 2, "失败会话不是孤儿，存活校验不得剔除");
        assert_eq!(usage.running_tokens, Some(100), "运行合计只含 running 会话，失败会话不计入");
    }

    #[test]
    fn completed_owned_thread_is_not_running_and_invalid_ids_are_not_paths() {
        let conn = db::open(Path::new(":memory:")).unwrap();
        db::set_session_activity(&conn, "codex", "finished", "done", 1).unwrap();
        let mut usage = db::live_usage(&conn, "codex", None).unwrap();
        filter_with(&mut usage, |_| Presence::Held);
        assert!(usage.active_sessions.is_empty());
        assert_eq!(presence(Some(Path::new(".")), "../not-a-session"), Presence::Unknown);
        assert_eq!(presence(None, "00000000-0000-0000-0000-000000000000"), Presence::Unknown);
    }

    #[test]
    #[ignore = "只读核对本机运行标记与实际写入锁；不输出会话标识或内容"]
    fn local_presence_read_only_smoke() {
        let path = PathBuf::from(std::env::var_os("APPDATA").unwrap()).join("dev.ningz.cc-usage/usage.db");
        let conn = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let raw = db::live_usage(&conn, "codex", None).unwrap();
        let verified = live_usage(&conn, "codex", None).unwrap();
        println!("日志运行标记={}；存活校验后={}；排除遗留或无法确认={}", raw.active_sessions.len(), verified.active_sessions.len(), raw.active_sessions.len()-verified.active_sessions.len());
        assert!(verified.active_sessions.len() <= raw.active_sessions.len());
    }

    #[cfg(windows)]
    #[test]
    fn windows_probe_distinguishes_live_lock_stale_file_and_missing_file() {
        use std::os::windows::io::AsRawHandle;
        #[link(name = "kernel32")]
        extern "system" {
            fn LockFile(handle: *mut std::ffi::c_void, low: u32, high: u32, len_low: u32, len_high: u32) -> i32;
        }
        let path = std::env::temp_dir().join(format!("island-presence-{}.lock", std::process::id()));
        let file = std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(&path).unwrap();
        assert_eq!(probe_lock(&path), Presence::NotHeld);
        assert_ne!(unsafe { LockFile(file.as_raw_handle(), 0, 0, u32::MAX, u32::MAX) }, 0);
        assert_eq!(probe_lock(&path), Presence::Held);
        assert_eq!(probe_lock(&path), Presence::Held);
        drop(file);
        assert_eq!(probe_lock(&path), Presence::NotHeld);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(probe_lock(&path), Presence::Missing);
    }
}
