//! 性能基线：复制本机真实应用库到临时目录，按应用相同的调用路径计时。
//! 只输出耗时与计数，不输出会话内容；默认忽略，需显式执行：
//! `cargo test --lib perf_bench -- --ignored --nocapture`

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn copy_app_database() -> PathBuf {
    let app_data = std::env::var("APPDATA").expect("缺少 APPDATA，非 Windows 环境无法测量");
    let dir = Path::new(&app_data).join("dev.ningz.cc-usage");
    let workspace = std::env::temp_dir().join(format!("cc-usage-perf-{}", std::process::id()));
    std::fs::create_dir_all(&workspace).unwrap();
    for name in ["usage.db", "usage.db-wal", "usage.db-shm"] {
        let source = dir.join(name);
        if source.exists() {
            std::fs::copy(&source, workspace.join(name)).unwrap();
        }
    }
    workspace.join("usage.db")
}

/// 预热一次后取 `rounds` 次的中位数，避免首轮页缓存冷热差异。
fn median(rounds: usize, mut work: impl FnMut()) -> Duration {
    work();
    let mut samples: Vec<Duration> = (0..rounds).map(|_| {
        let started = Instant::now();
        work();
        started.elapsed()
    }).collect();
    samples.sort();
    samples[samples.len() / 2]
}

fn report(label: &str, value: Duration) {
    println!("{label:<44} {:>9.2} ms", value.as_secs_f64() * 1000.0);
}

#[test]
#[ignore = "复制本机真实应用库做性能基线；仅显式执行且不输出会话内容"]
fn perf_baseline() {
    let database = copy_app_database();
    let mut conn = crate::db::open(&database).unwrap();
    let requests: i64 = conn.query_row("SELECT COUNT(*) FROM requests", [], |r| r.get(0)).unwrap();
    let events: i64 = conn.query_row("SELECT COUNT(*) FROM usage_events", [], |r| r.get(0)).unwrap();
    let files: i64 = conn.query_row("SELECT COUNT(*) FROM scan_state", [], |r| r.get(0)).unwrap();
    println!("requests={requests} usage_events={events} scan_state={files}");

    // 先追平复制时刻之后的新增，再测稳态。
    let catch_up = Instant::now();
    crate::collector::scan(&mut conn);
    report("首轮追平扫描", catch_up.elapsed());

    report("心跳全量扫描（无新增）", median(5, || { crate::collector::scan(&mut conn); }));
    report("本机来源扫描（无新增）", median(5, || { crate::native_sources::scan(&mut conn); }));

    let home = crate::creds::home().unwrap();
    let mut claude_files = Vec::new();
    for entry in walk(&home.join(".claude").join("projects")) {
        if entry.extension().is_some_and(|ext| ext == "jsonl") { claude_files.push(entry); }
    }
    if let Some(path) = claude_files.first().cloned() {
        report("单文件写事件扫描（无新增）", median(5, || {
            crate::collector::scan_paths(&mut conn, vec![path.clone()]);
        }));
    }

    for platform in ["claude", "codex"] {
        report(&format!("live_usage {platform}"), median(9, || {
            crate::session_presence::live_usage(&conn, platform, None).unwrap();
        }));
    }
    report("live_usage 八个平台合计", median(5, || {
        for platform in ["claude", "codex", "gemini", "grok", "zcode", "trae", "qoder", "workbuddy"] {
            crate::session_presence::live_usage(&conn, platform, Some(0)).unwrap();
        }
    }));

    let now = chrono::Local::now().timestamp_millis();
    for platform in ["claude", "codex"] {
        for period in ["today", "total"] {
            report(&format!("总览一次刷新 {platform}/{period}"), median(5, || {
                crate::db::token_totals_at(&conn, platform, None, None, now).unwrap();
                crate::db::usage_breakdown_at(&conn, platform, period, None, None, now).unwrap();
                crate::db::trend_at(&conn, platform, period, None, None, now).unwrap();
                crate::db::request_log_at(&conn, platform, period, 1, None, None, now).unwrap();
                crate::db::list_models_at(&conn, platform, period, None, now).unwrap();
                crate::db::cost_estimate_at(&conn, platform, period, None, None, now).unwrap();
                let (start, end) = crate::db::period_range_at(&conn, platform, period, now);
                crate::source_store::metrics(&conn, platform, start, end, None).unwrap();
            }));
        }
        report(&format!("token_totals {platform}"), median(9, || {
            crate::db::token_totals_at(&conn, platform, None, None, now).unwrap();
        }));
        report(&format!("usage_breakdown {platform}/total"), median(9, || {
            crate::db::usage_breakdown_at(&conn, platform, "total", None, None, now).unwrap();
        }));
        report(&format!("usage_trend {platform}/total"), median(9, || {
            crate::db::trend_at(&conn, platform, "total", None, None, now).unwrap();
        }));
        report(&format!("cost_estimate {platform}/total"), median(9, || {
            crate::db::cost_estimate_at(&conn, platform, "total", None, None, now).unwrap();
        }));
        report(&format!("list_models {platform}/total"), median(9, || {
            crate::db::list_models_at(&conn, platform, "total", None, now).unwrap();
        }));
        report(&format!("source_metrics {platform}/total"), median(9, || {
            let (start, end) = crate::db::period_range_at(&conn, platform, "total", now);
            crate::source_store::metrics(&conn, platform, start, end, None).unwrap();
        }));
        report(&format!("request_log {platform}/total 第 50 页"), median(9, || {
            crate::db::request_log_at(&conn, platform, "total", 50, None, None, now).unwrap();
        }));
        report(&format!("today_fresh_tokens {platform}"), median(9, || {
            crate::db::today_fresh_tokens(&conn, platform).unwrap();
        }));
    }

    drop(conn);
    let _ = std::fs::remove_dir_all(database.parent().unwrap());
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() { out.extend(walk(&path)); } else { out.push(path); }
    }
    out
}

/// 冷启动回填：全新数据库从头采集本机全部会话日志（首次安装的场景）。
#[test]
#[ignore = "从头读取本机全部会话日志计时；仅显式执行且不输出会话内容"]
fn perf_cold_backfill() {
    let workspace = std::env::temp_dir().join(format!("cc-usage-cold-{}", std::process::id()));
    std::fs::create_dir_all(&workspace).unwrap();
    let mut conn = crate::db::open(&workspace.join("usage.db")).unwrap();
    let started = Instant::now();
    let result = crate::collector::scan(&mut conn);
    let elapsed = started.elapsed();
    let requests: i64 = conn.query_row("SELECT COUNT(*) FROM requests", [], |r| r.get(0)).unwrap();
    println!("files={} requests={requests} errors={}", result.files_scanned, result.errors.len());
    report("冷启动全量回填", elapsed);
    drop(conn);
    let _ = std::fs::remove_dir_all(workspace);
}
