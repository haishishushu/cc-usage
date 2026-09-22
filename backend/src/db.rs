//! SQLite 存储与统计查询
//!
//! 约束见需求文档 §7.5 / §7.7：
//! - 按查询范围读取，避免全部历史常驻内存
//! - 查询配合实际过滤和排序建立必要索引
//! - 数据保存绝对时间（UTC 毫秒），显示与自然周期计算使用系统时区
//! - 查询区间包含开始、不包含结束

use chrono::{DateTime, Datelike, Duration, Local, TimeZone};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// 一条请求级记录。字段缺失一律用 Option，不伪造为 0（§2.4）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestRecord {
    pub platform: String,
    pub source: String,
    /// 去重键：Claude 用 message.id，Codex 用 response_id
    pub dedup_key: String,
    pub session_id: Option<String>,
    /// UTC 毫秒
    pub ts: i64,
    pub model: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub cache_write_tokens: Option<i64>,
    /// 总 Token：优先采用来源提供且语义明确的总值，否则由采集器按该来源口径计算
    pub total_tokens: Option<i64>,
    /// 思考强度（low / medium / high 等），来源原样透传。
    /// Claude 在 assistant 行自带；Codex 只在 turn_context 上给出，由采集器按会话继承。
    /// 来源未提供时为 None —— 界面显示「—」，不猜测也不补默认档位。
    pub effort: Option<String>,
}

pub fn open(path: &std::path::Path) -> rusqlite::Result<Connection> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    init(&conn)?;
    Ok(conn)
}

fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS requests (
            id                 INTEGER PRIMARY KEY,
            platform           TEXT    NOT NULL,
            source             TEXT    NOT NULL,
            dedup_key          TEXT    NOT NULL,
            session_id         TEXT,
            ts                 INTEGER NOT NULL,
            model              TEXT,
            input_tokens       INTEGER NOT NULL DEFAULT 0,
            output_tokens      INTEGER NOT NULL DEFAULT 0,
            cache_read_tokens  INTEGER NOT NULL DEFAULT 0,
            cache_write_tokens INTEGER NOT NULL DEFAULT 0,
            total_tokens       INTEGER NOT NULL DEFAULT 0,
            input_known        INTEGER NOT NULL DEFAULT 1,
            output_known       INTEGER NOT NULL DEFAULT 1,
            cache_read_known   INTEGER NOT NULL DEFAULT 1,
            cache_write_known  INTEGER NOT NULL DEFAULT 1,
            total_known        INTEGER NOT NULL DEFAULT 1,
            effort             TEXT,
            duration_ms        INTEGER,
            first_token_ms     INTEGER,
            status_code        INTEGER,
            UNIQUE(source, dedup_key)
        );
        -- 日志按时间倒序分页、汇总按平台+区间过滤，两个索引对应这两类查询
        CREATE INDEX IF NOT EXISTS idx_requests_ts ON requests(ts DESC);
        CREATE INDEX IF NOT EXISTS idx_requests_platform_ts ON requests(platform, ts DESC);

        -- 请求级最终值之外，单独记录每次新增的正向差额；同一个流式请求
        -- 多次更新时统计表保留最终值，实时游标仍能看到后续差额。
        CREATE TABLE IF NOT EXISTS usage_events (
            id          INTEGER PRIMARY KEY,
            request_id  INTEGER NOT NULL,
            platform    TEXT    NOT NULL,
            session_id  TEXT,
            ts          INTEGER NOT NULL,
            delta_total INTEGER NOT NULL,
            FOREIGN KEY(request_id) REFERENCES requests(id)
        );
        CREATE INDEX IF NOT EXISTS idx_usage_events_platform_id ON usage_events(platform, id);
        CREATE INDEX IF NOT EXISTS idx_usage_events_session_ts ON usage_events(platform, session_id, ts);

        -- 会话标题：Claude Code 的 ai-title 行、Codex 的 session_index.jsonl
        CREATE TABLE IF NOT EXISTS sessions (
            platform   TEXT NOT NULL,
            session_id TEXT NOT NULL,
            title      TEXT,
            is_running INTEGER NOT NULL DEFAULT 0,
            activity_state TEXT NOT NULL DEFAULT 'done',
            activity_updated_ms INTEGER,
            PRIMARY KEY (platform, session_id)
        );

        -- 连接：只有官方订阅（auth）与 API Key（api）两种（§6.3）。
        -- 本地会话记录是「统计来源」而非连接，不入此表。
        --
        -- 凭证与展示分离：secret 只在查额度/余额时由后端读取，
        -- 任何返回给前端的结构都只带 masked，界面与日志永不显示原值。
        CREATE TABLE IF NOT EXISTS connections (
            id           TEXT PRIMARY KEY,
            platform     TEXT    NOT NULL,
            kind         TEXT    NOT NULL CHECK (kind IN ('auth','api')),
            name         TEXT    NOT NULL,
            label        TEXT    NOT NULL DEFAULT '',
            secret       TEXT,
            masked       TEXT    NOT NULL DEFAULT '',
            -- sub2api 等自建网关的部署地址；官方直连留空
            base_url     TEXT,
            status       TEXT    NOT NULL DEFAULT 'connected',
            last_sync_ms INTEGER,
            created_ms   INTEGER NOT NULL
        );

        -- 增量读取状态：记录每个文件已读到的字节偏移，避免定时全量扫描
        CREATE TABLE IF NOT EXISTS scan_state (
            path   TEXT PRIMARY KEY,
            offset INTEGER NOT NULL,
            mtime  INTEGER NOT NULL,
            parser_model TEXT,
            parser_session_id TEXT,
            parser_activity_state TEXT,
            parser_updated_at_ms INTEGER,
            parser_initialized INTEGER NOT NULL DEFAULT 0,
            parser_effort TEXT
        );
        "#,
    )?;

    crate::source_store::init(&conn)?;

    // 旧库先补“字段是否由来源提供”的标记，再执行依赖这些列的事件迁移。
    let _ = conn.execute("ALTER TABLE requests ADD COLUMN input_known INTEGER NOT NULL DEFAULT 1", []);
    let _ = conn.execute("ALTER TABLE requests ADD COLUMN output_known INTEGER NOT NULL DEFAULT 1", []);
    let _ = conn.execute("ALTER TABLE requests ADD COLUMN cache_read_known INTEGER NOT NULL DEFAULT 1", []);
    let _ = conn.execute("ALTER TABLE requests ADD COLUMN cache_write_known INTEGER NOT NULL DEFAULT 1", []);
    let _ = conn.execute("ALTER TABLE requests ADD COLUMN total_known INTEGER NOT NULL DEFAULT 1", []);
    // 思考强度是后加的列。历史行保持 NULL：那时的记录并未采集该值，
    // 补任何默认档位都是伪造，界面按「—」显示即可。
    let _ = conn.execute("ALTER TABLE requests ADD COLUMN effort TEXT", []);
    // 耗时、首字延迟与响应码只有「本应用在请求路径上」时才测得到（本地代理，默认关闭）。
    // 会话记录里没有这些值，所以历史行与未开代理时一律为 NULL，界面显示「—」。
    let _ = conn.execute("ALTER TABLE requests ADD COLUMN duration_ms INTEGER", []);
    let _ = conn.execute("ALTER TABLE requests ADD COLUMN first_token_ms INTEGER", []);
    let _ = conn.execute("ALTER TABLE requests ADD COLUMN status_code INTEGER", []);

    // 实时口径对齐终端：运行中/今日增量只算新鲜 Token；Codex 从 input 中扣除缓存字段。
    // 旧行为 NULL 时回退 delta_total，历史事件不重算。
    let _ = conn.execute("ALTER TABLE usage_events ADD COLUMN delta_fresh INTEGER", []);

    // 旧库首次升级时为已有请求建立一次基线事件。前端首次快照不会播放动画，
    // 但游标从此可统一切换到 usage_events。
    conn.execute(
        "INSERT INTO usage_events(request_id, platform, session_id, ts, delta_total, delta_fresh)
         SELECT r.id, r.platform, r.session_id, r.ts, r.total_tokens,
                r.input_tokens + r.output_tokens
         FROM requests r
         WHERE r.total_known = 1 AND r.total_tokens > 0
           AND NOT EXISTS (SELECT 1 FROM usage_events e WHERE e.request_id = r.id)",
        [],
    )?;

    // 迁移：早期版本没有 base_url 列。重复执行会报 duplicate column，忽略即可
    let _ = conn.execute("ALTER TABLE connections ADD COLUMN base_url TEXT", []);
    // 连接默认参数（手动添加时可选）：默认模型 / 思考强度 / 1M 上下文（画布 18）
    let _ = conn.execute("ALTER TABLE connections ADD COLUMN model TEXT", []);
    let _ = conn.execute("ALTER TABLE connections ADD COLUMN effort TEXT", []);
    let _ = conn.execute("ALTER TABLE connections ADD COLUMN context_1m INTEGER", []);
    // 会话生命周期：旧库升级后默认均为停止，避免把近期 Token 误报成正在运行。
    let _ = conn.execute("ALTER TABLE sessions ADD COLUMN is_running INTEGER NOT NULL DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE sessions ADD COLUMN activity_state TEXT NOT NULL DEFAULT 'done'", []);
    let _ = conn.execute("ALTER TABLE sessions ADD COLUMN activity_updated_ms INTEGER", []);
    let _ = conn.execute(
        "UPDATE sessions SET activity_state = 'running' WHERE is_running = 1 AND activity_state = 'done'",
        [],
    );
    let _ = conn.execute("ALTER TABLE scan_state ADD COLUMN parser_model TEXT", []);
    let _ = conn.execute("ALTER TABLE scan_state ADD COLUMN parser_session_id TEXT", []);
    let _ = conn.execute("ALTER TABLE scan_state ADD COLUMN parser_activity_state TEXT", []);
    let _ = conn.execute("ALTER TABLE scan_state ADD COLUMN parser_updated_at_ms INTEGER", []);
    // 思考强度只在 Codex 的 turn_context 上出现，必须随位点一起存，
    // 否则断点续扫后要等下一条 turn_context 才恢复，这中间的记录会平白缺值。
    let _ = conn.execute("ALTER TABLE scan_state ADD COLUMN parser_effort TEXT", []);
    let _ = conn.execute(
        "ALTER TABLE scan_state ADD COLUMN parser_initialized INTEGER NOT NULL DEFAULT 0",
        [],
    );
    // 新增本轮起点后只重建一次解析上下文，从源日志恢复真实开始时间。
    if conn.execute("ALTER TABLE sessions ADD COLUMN turn_started_ms INTEGER", []).is_ok() {
        conn.execute("UPDATE scan_state SET parser_initialized = 0", [])?;
    }
    Ok(())
}

#[cfg(test)]
pub fn get_offset(conn: &Connection, path: &str) -> i64 {
    get_scan_state(conn, path).map_or(0, |state| state.0)
}

#[derive(Debug, Serialize)]
pub struct CleanupPreview {
    pub cutoff_ms: i64,
    pub requests: i64,
    pub usage_events: i64,
    pub sessions: i64,
}

#[derive(Debug, Serialize)]
pub struct ImportSummary {
    pub requests_changed: usize,
    pub sessions_changed: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupSession {
    platform: String,
    session_id: String,
    title: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupMetric {
    source: String, dedup_key: String, input_semantics: String,
    #[serde(default)] native_outcome: Option<String>,
    credits: Option<f64>, original_credits: Option<f64>, billable: Option<bool>,
    context_ratio: Option<f64>, reasoning: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupFile {
    #[serde(default)]
    metrics: Vec<BackupMetric>,
    #[serde(default)]
    contexts: Vec<crate::source_store::ContextSnapshot>,
    schema_version: u32,
    exported_at_ms: i64,
    platform: Option<String>,
    requests: Vec<RequestRecord>,
    sessions: Vec<BackupSession>,
}

pub fn get_scan_state(conn: &Connection, path: &str) -> Option<(i64, i64)> {
    conn.query_row(
        "SELECT offset, mtime FROM scan_state WHERE path = ?1",
        params![path],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).ok()
}

#[derive(Default)]
pub struct ScanCheckpoint {
    pub offset: i64,
    pub mtime: i64,
    pub parser_model: Option<String>,
    pub parser_session_id: Option<String>,
    pub parser_activity_state: Option<String>,
    pub parser_updated_at_ms: Option<i64>,
    pub parser_initialized: bool,
    /// Codex 的思考强度（会话级，随文件顺序继承）
    pub parser_effort: Option<String>,
}

pub fn get_scan_checkpoint(conn: &Connection, path: &str) -> Option<ScanCheckpoint> {
    conn.query_row(
        "SELECT offset, mtime, parser_model, parser_session_id,
                parser_activity_state, parser_updated_at_ms, parser_initialized,
                parser_effort
         FROM scan_state WHERE path = ?1",
        params![path],
        |row| Ok(ScanCheckpoint {
            offset: row.get(0)?,
            mtime: row.get(1)?,
            parser_model: row.get(2)?,
            parser_session_id: row.get(3)?,
            parser_activity_state: row.get(4)?,
            parser_updated_at_ms: row.get(5)?,
            parser_initialized: row.get::<_, i32>(6)? != 0,
            parser_effort: row.get(7)?,
        }),
    ).ok()
}

pub fn set_scan_checkpoint(
    conn: &Connection,
    path: &str,
    checkpoint: &ScanCheckpoint,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO scan_state(
             path, offset, mtime, parser_model, parser_session_id,
             parser_activity_state, parser_updated_at_ms, parser_initialized,
             parser_effort
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
         ON CONFLICT(path) DO UPDATE SET
             offset = excluded.offset,
             mtime = excluded.mtime,
             parser_model = excluded.parser_model,
             parser_session_id = excluded.parser_session_id,
             parser_activity_state = excluded.parser_activity_state,
             parser_updated_at_ms = excluded.parser_updated_at_ms,
             parser_initialized = excluded.parser_initialized,
             parser_effort = excluded.parser_effort",
        params![
            path,
            checkpoint.offset,
            checkpoint.mtime,
            checkpoint.parser_model,
            checkpoint.parser_session_id,
            checkpoint.parser_activity_state,
            checkpoint.parser_updated_at_ms,
            checkpoint.parser_initialized as i32,
            checkpoint.parser_effort,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
pub fn set_offset(conn: &Connection, path: &str, offset: i64, mtime: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO scan_state(path, offset, mtime) VALUES(?1, ?2, ?3)
         ON CONFLICT(path) DO UPDATE SET offset = ?2, mtime = ?3",
        params![path, offset, mtime],
    )?;
    Ok(())
}

/// 实时统计使用的新鲜 Token 口径。
/// Claude 的 input_tokens 不含缓存，Codex 的 input_tokens 已包含 cached_input_tokens，
/// 所以只有 Codex 需要从输入量中扣除缓存字段。
fn fresh_tokens(platform: &str, input: i64, output: i64, cache_read: i64, cache_write: i64) -> i64 {
    let input = input.max(0);
    let output = output.max(0);
    let uncached_input = if platform == "codex" {
        input
            .saturating_sub(cache_read.max(0))
            .saturating_sub(cache_write.max(0))
    } else {
        input
    };
    uncached_input.saturating_add(output)
}

fn insert_records_into(conn: &Connection, recs: &[RequestRecord]) -> rusqlite::Result<usize> {
    fn merge(old: i64, known: bool, next: Option<i64>) -> (i64, bool) {
        match next {
            Some(value) => (if known { old.max(value) } else { value }, true),
            None => (old, known),
        }
    }
    let mut changed = 0usize;
    for r in recs {
        let previous = conn.query_row(
            "SELECT id, input_tokens, output_tokens, cache_read_tokens,
                    cache_write_tokens, total_tokens, input_known, output_known,
                    cache_read_known, cache_write_known, total_known
             FROM requests WHERE source = ?1 AND dedup_key = ?2",
            params![r.source, r.dedup_key],
            |row| Ok((
                row.get::<_, i64>(0)?,
                [row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?],
                [row.get::<_, i32>(6)? != 0, row.get::<_, i32>(7)? != 0,
                 row.get::<_, i32>(8)? != 0, row.get::<_, i32>(9)? != 0,
                 row.get::<_, i32>(10)? != 0],
            )),
        ).optional()?;
        let incoming = [r.input_tokens, r.output_tokens, r.cache_read_tokens, r.cache_write_tokens, r.total_tokens];
        let (request_id, delta_total, delta_fresh) = if let Some((id, old, known)) = previous {
            let next: [(i64, bool); 5] =
                std::array::from_fn(|index| merge(old[index], known[index], incoming[index]));
            let delta = if next[4].1 { next[4].0 - if known[4] { old[4] } else { 0 } } else { 0 };
            let old_fresh = fresh_tokens(
                &r.platform,
                if known[0] { old[0] } else { 0 },
                if known[1] { old[1] } else { 0 },
                if known[2] { old[2] } else { 0 },
                if known[3] { old[3] } else { 0 },
            );
            let next_fresh = fresh_tokens(&r.platform, next[0].0, next[1].0, next[2].0, next[3].0);
            let delta_fresh = (next_fresh - old_fresh).max(0);
            if next.iter().enumerate().any(|(index, value)| value.0 != old[index] || value.1 != known[index]) {
                conn.execute(
                    "UPDATE requests SET session_id = COALESCE(?2, session_id), ts = MAX(ts, ?3),
                        model = COALESCE(?4, model), input_tokens = ?5, output_tokens = ?6,
                        cache_read_tokens = ?7, cache_write_tokens = ?8, total_tokens = ?9,
                        input_known = ?10, output_known = ?11, cache_read_known = ?12,
                        cache_write_known = ?13, total_known = ?14
                     WHERE id = ?1",
                    params![id, r.session_id, r.ts, r.model,
                        next[0].0, next[1].0, next[2].0, next[3].0, next[4].0,
                        next[0].1 as i32, next[1].1 as i32, next[2].1 as i32,
                        next[3].1 as i32, next[4].1 as i32],
                )?;
                changed += 1;
            }
            (id, delta.max(0), delta_fresh)
        } else {
            conn.execute(
                "INSERT INTO requests
                 (platform, source, dedup_key, session_id, ts, model,
                  input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, total_tokens,
                  input_known, output_known, cache_read_known, cache_write_known, total_known, effort)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
                params![r.platform, r.source, r.dedup_key, r.session_id, r.ts, r.model,
                    r.input_tokens.unwrap_or(0), r.output_tokens.unwrap_or(0),
                    r.cache_read_tokens.unwrap_or(0), r.cache_write_tokens.unwrap_or(0),
                    r.total_tokens.unwrap_or(0), r.input_tokens.is_some() as i32,
                    r.output_tokens.is_some() as i32, r.cache_read_tokens.is_some() as i32,
                    r.cache_write_tokens.is_some() as i32, r.total_tokens.is_some() as i32,
                    r.effort],
            )?;
            changed += 1;
            (conn.last_insert_rowid(), r.total_tokens.unwrap_or(0).max(0),
                fresh_tokens(
                    &r.platform,
                    r.input_tokens.unwrap_or(0),
                    r.output_tokens.unwrap_or(0),
                    r.cache_read_tokens.unwrap_or(0),
                    r.cache_write_tokens.unwrap_or(0),
                ))
        };
        if delta_total > 0 || delta_fresh > 0 {
            conn.execute(
                "INSERT INTO usage_events(request_id, platform, session_id, ts, delta_total, delta_fresh)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![request_id, r.platform, r.session_id, r.ts, delta_total, delta_fresh],
            )?;
        }
    }
    Ok(changed)
}

pub(crate) fn insert_scanned_records(
    conn: &Connection,
    recs: &[RequestRecord],
) -> rusqlite::Result<usize> {
    insert_records_into(conn, recs)
}

/// 本地代理计时回填（proxy.rs）：按去重键关联本机请求记录。
/// 返回受影响行数；0 表示记录尚未由采集器入库，调用方应转入待关联队列稍后重试。
/// source 必须与采集器写入的 [`crate::collector::SOURCE_LOCAL`] 一致，硬编码字面量会静默匹配不到。
pub fn apply_proxy_timing(
    conn: &Connection,
    dedup_key: &str,
    first_token_ms: i64,
    duration_ms: i64,
    status_code: i64,
) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE requests SET duration_ms = ?3, first_token_ms = ?4, status_code = ?5
         WHERE source = ?1 AND dedup_key = ?2",
        params![crate::collector::SOURCE_LOCAL, dedup_key, duration_ms, first_token_ms, status_code],
    )
}

pub fn cleanup_preview(conn: &Connection, cutoff_ms: i64) -> rusqlite::Result<CleanupPreview> {
    let requests = conn.query_row(
        "SELECT COUNT(*) FROM requests WHERE ts < ?1",
        params![cutoff_ms],
        |row| row.get(0),
    )?;
    let usage_events = conn.query_row(
        "SELECT COUNT(*) FROM usage_events
         WHERE ts < ?1 OR request_id IN (SELECT id FROM requests WHERE ts < ?1)",
        params![cutoff_ms],
        |row| row.get(0),
    )?;
    let sessions = conn.query_row(
        "SELECT COUNT(*) FROM sessions s
         WHERE s.is_running = 0
           AND s.activity_updated_ms IS NOT NULL
           AND s.activity_updated_ms < ?1
           AND NOT EXISTS (
             SELECT 1 FROM requests r
             WHERE r.platform = s.platform AND r.session_id = s.session_id AND r.ts >= ?1
           )",
        params![cutoff_ms],
        |row| row.get(0),
    )?;
    Ok(CleanupPreview { cutoff_ms, requests, usage_events, sessions })
}

pub fn cleanup_history(conn: &mut Connection, cutoff_ms: i64) -> rusqlite::Result<CleanupPreview> {
    let preview = cleanup_preview(conn, cutoff_ms)?;
    let tx = conn.transaction()?;
    tx.execute("UPDATE collection_policy SET cutoff_ms=max(cutoff_ms,?1) WHERE singleton=1",[cutoff_ms])?;
    tx.execute(
        "DELETE FROM usage_events WHERE ts < ?1 OR request_id IN (SELECT id FROM requests WHERE ts < ?1)",
        params![cutoff_ms],
    )?;
    tx.execute("DELETE FROM native_context WHERE ts < ?1",[cutoff_ms])?;
    tx.execute("DELETE FROM request_metrics WHERE request_id IN (SELECT id FROM requests WHERE ts < ?1)", params![cutoff_ms])?;
    tx.execute("DELETE FROM requests WHERE ts < ?1", params![cutoff_ms])?;
    tx.execute(
        "DELETE FROM sessions
         WHERE is_running = 0
           AND activity_updated_ms IS NOT NULL
           AND activity_updated_ms < ?1
           AND NOT EXISTS (
             SELECT 1 FROM requests r
             WHERE r.platform = sessions.platform AND r.session_id = sessions.session_id
           )",
        params![cutoff_ms],
    )?;
    // scan_state 故意保留：清理后的历史日志不会在下一轮采集时重新导入。
    tx.commit()?;
    let _ = conn.execute_batch("PRAGMA optimize;");
    Ok(preview)
}

pub fn export_backup(conn: &Connection, platform: Option<&str>) -> Result<String, String> {
    let mut request_stmt = conn.prepare(
        "SELECT platform, source, dedup_key, session_id, ts, model,
                input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, total_tokens,
                input_known, output_known, cache_read_known, cache_write_known, total_known,
                effort
         FROM requests
         WHERE ?1 IS NULL OR platform = ?1
         ORDER BY ts ASC",
    ).map_err(|error| error.to_string())?;
    let requests = request_stmt.query_map(params![platform], |row| {
        let known = [
            row.get::<_, i32>(11)? != 0,
            row.get::<_, i32>(12)? != 0,
            row.get::<_, i32>(13)? != 0,
            row.get::<_, i32>(14)? != 0,
            row.get::<_, i32>(15)? != 0,
        ];
        let values = [row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?, row.get(10)?];
        Ok(RequestRecord {
            platform: row.get(0)?,
            source: row.get(1)?,
            dedup_key: row.get(2)?,
            session_id: row.get(3)?,
            ts: row.get(4)?,
            model: row.get(5)?,
            input_tokens: known[0].then_some(values[0]),
            output_tokens: known[1].then_some(values[1]),
            cache_read_tokens: known[2].then_some(values[2]),
            cache_write_tokens: known[3].then_some(values[3]),
            total_tokens: known[4].then_some(values[4]),
            effort: row.get(16)?,
        })
    }).map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>().map_err(|error| error.to_string())?;

    let mut session_stmt = conn.prepare(
        "SELECT platform, session_id, title FROM sessions
         WHERE title IS NOT NULL AND (?1 IS NULL OR platform = ?1)
         ORDER BY platform, session_id",
    ).map_err(|error| error.to_string())?;
    let sessions = session_stmt.query_map(params![platform], |row| Ok(BackupSession {
        platform: row.get(0)?,
        session_id: row.get(1)?,
        title: row.get(2)?,
    })).map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>().map_err(|error| error.to_string())?;

    let mut stmt=conn.prepare("SELECT r.source,r.dedup_key,r.input_semantics,m.credits,m.original_credits,m.billable,m.context_ratio,m.reasoning,r.native_outcome
        FROM requests r LEFT JOIN request_metrics m ON m.request_id=r.id WHERE ?1 IS NULL OR r.platform=?1").map_err(|e|e.to_string())?;
    let metrics=stmt.query_map(params![platform],|r|Ok(BackupMetric { source:r.get(0)?,dedup_key:r.get(1)?,input_semantics:r.get(2)?,
        credits:r.get(3)?,original_credits:r.get(4)?,billable:r.get(5)?,context_ratio:r.get(6)?,reasoning:r.get(7)?,native_outcome:r.get(8)? }))
        .map_err(|e|e.to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|e|e.to_string())?;
    serde_json::to_string_pretty(&BackupFile {
        metrics,
        contexts: crate::source_store::contexts(conn,platform).map_err(|e|e.to_string())?,
        schema_version: 2,
        exported_at_ms: chrono::Utc::now().timestamp_millis(),
        platform: platform.map(str::to_string),
        requests,
        sessions,
    }).map_err(|error| error.to_string())
}

pub fn import_backup(conn: &mut Connection, json: &str) -> Result<ImportSummary, String> {
    if json.len() > 100 * 1024 * 1024 {
        return Err("导入文件超过 100 MB 限制".into());
    }
    let backup: BackupFile = serde_json::from_str(json)
        .map_err(|error| format!("备份 JSON 格式无效：{error}"))?;
    if !matches!(backup.schema_version, 1 | 2) {
        return Err(format!("不支持的备份版本：{}", backup.schema_version));
    }
    if backup.requests.len() > 1_000_000 {
        return Err("导入记录超过 100 万条限制".into());
    }
    for record in &backup.requests {
        if !crate::platforms::known(&record.platform)
            || record.source.trim().is_empty()
            || record.dedup_key.trim().is_empty()
            || record.source.len() > 512
            || record.dedup_key.len() > 1_024
            || record.model.as_ref().is_some_and(|model| model.len() > 512)
            || record.ts <= 0
            || [record.input_tokens, record.output_tokens, record.cache_read_tokens,
                record.cache_write_tokens, record.total_tokens]
                .into_iter().flatten().any(|value| value < 0)
        {
            return Err("备份包含无效的平台、标识、时间或 Token 数值".into());
        }
    }
    for session in &backup.sessions {
        if !crate::platforms::known(&session.platform)
            || session.session_id.trim().is_empty()
            || session.session_id.len() > 512
            || session.title.len() > 4_096
        {
            return Err("备份包含无效的会话平台、标识或标题".into());
        }
    }
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let mut metrics=std::collections::HashMap::new();
    for m in &backup.metrics {
        if !matches!(m.input_semantics.as_str(),"includes_cache"|"excludes_cache"|"unknown")
            || [m.credits,m.original_credits,m.context_ratio].into_iter().flatten().any(|n| !n.is_finite() || n<0.0)
            || m.native_outcome.as_deref().is_some_and(|s|!matches!(s,"success"|"failed"|"unknown"))
            || m.context_ratio.is_some_and(|n|n>1.0) || m.reasoning.is_some_and(|n|n<0)
            || metrics.insert((m.source.as_str(),m.dedup_key.as_str()),m).is_some() {
            return Err("备份包含无效或重复的来源指标".into());
        }
    }
    let mut requests_changed=0;
    for r in &backup.requests {
        if crate::platforms::native(&r.platform) {
            let m=metrics.get(&(r.source.as_str(),r.dedup_key.as_str()));
            let input_semantics=match m.map(|m|m.input_semantics.as_str()) {
                Some("includes_cache")=>"includes_cache",Some("excludes_cache")=>"excludes_cache",_=>"unknown"
            };
            requests_changed+=crate::source_store::write(&tx,&[crate::source_store::Record { request:r.clone(),input_semantics,
                credits:m.and_then(|m|m.credits),original_credits:m.and_then(|m|m.original_credits),billable:m.and_then(|m|m.billable),
                context_ratio:m.and_then(|m|m.context_ratio),reasoning:m.and_then(|m|m.reasoning) }]).map_err(|e|e.to_string())?;
            if let Some(m)=m {tx.execute("UPDATE requests SET native_outcome=?1 WHERE source=?2 AND dedup_key=?3",params![m.native_outcome,r.source,r.dedup_key]).map_err(|e|e.to_string())?;}
        } else {requests_changed+=insert_records_into(&tx,std::slice::from_ref(r)).map_err(|e|e.to_string())?;}
    }
    for c in backup.contexts {
        if !crate::platforms::known(&c.platform) || c.source.len()>512 || c.session_id.len()>1024 || c.ts<=0
            || !c.ratio.is_finite() || !(0.0..=1.0).contains(&c.ratio) {return Err("备份包含无效上下文快照".into());}
        crate::source_store::context(&tx,&c).map_err(|e|e.to_string())?;
    }
    let mut sessions_changed = 0;
    for session in backup.sessions {
        if !session.title.trim().is_empty() {
            sessions_changed += tx.execute(
                "INSERT INTO sessions(platform, session_id, title) VALUES(?1,?2,?3)
                 ON CONFLICT(platform, session_id) DO UPDATE SET title = ?3
                 WHERE sessions.title IS NOT excluded.title",
                params![session.platform, session.session_id, session.title],
            ).map_err(|error| error.to_string())?;
        }
    }
    tx.commit().map_err(|error| error.to_string())?;
    Ok(ImportSummary { requests_changed, sessions_changed })
}

/// 批量写入；重复的 (source, dedup_key) 直接忽略 —— 去重在数据库层做。
#[cfg(test)]
pub fn insert_records(conn: &mut Connection, recs: &[RequestRecord]) -> rusqlite::Result<usize> {
    let tx = conn.transaction()?;
    let inserted = insert_records_into(&tx, recs)?;
    tx.commit()?;
    Ok(inserted)
}

/// 单个日志文件的一次扫描提交。请求、标题、生命周期与字节偏移必须同事务成功，
/// 否则回滚全部内容，下次从原偏移重读，避免“偏移已前进但记录未入库”。
#[cfg(test)]
pub fn commit_scanned_file(
    conn: &mut Connection,
    recs: &[RequestRecord],
    titles: &[(String, String, String)],
    activity: Option<(&str, &str, &str, i64)>,
    path: &str,
    offset: i64,
    mtime: i64,
) -> rusqlite::Result<usize> {
    let tx = conn.transaction()?;
    let inserted = insert_records_into(&tx, recs)?;
    for (platform, session_id, title) in titles {
        upsert_session_title(&tx, platform, session_id, title)?;
    }
    if let Some((platform, session_id, state, updated_at_ms)) = activity {
        set_session_activity(&tx, platform, session_id, state, updated_at_ms)?;
    }
    set_offset(&tx, path, offset, mtime)?;
    tx.commit()?;
    Ok(inserted)
}

pub fn upsert_session_title(
    conn: &Connection,
    platform: &str,
    session_id: &str,
    title: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO sessions(platform, session_id, title) VALUES(?1,?2,?3)
         ON CONFLICT(platform, session_id) DO UPDATE SET title = ?3
         WHERE sessions.title IS NOT excluded.title",
        params![platform, session_id, title],
    )?;
    Ok(())
}

pub fn set_session_activity(
    conn: &Connection,
    platform: &str,
    session_id: &str,
    state: &str,
    updated_at_ms: i64,
) -> rusqlite::Result<()> {
    let state = if matches!(state, "running" | "done" | "failed" | "unknown" | "waiting") { state } else { "unknown" };
    conn.execute(
        "INSERT INTO sessions(platform, session_id, is_running, activity_state, activity_updated_ms)
         VALUES(?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(platform, session_id) DO UPDATE SET
           is_running = excluded.is_running,
           activity_state = excluded.activity_state,
           activity_updated_ms = excluded.activity_updated_ms
         WHERE sessions.activity_updated_ms IS NULL
            OR excluded.activity_updated_ms >= sessions.activity_updated_ms",
        params![platform, session_id, (state == "running") as i32, state, updated_at_ms],
    )?;
    Ok(())
}

/* ------------------------------------------------------------------------ */
/*  实时增量与最近活跃会话（§2.1.1 / §2.1.2）                                */
/* ------------------------------------------------------------------------ */

#[derive(Debug, Serialize)]
pub struct ActiveSession {
    pub session_id: String,
    /// 来源提供的真实标题；缺失时显示未命名会话，不把 ID 当标题
    pub title: String,
    /// 本段提示内该会话新增的 Token
    pub delta_tokens: i64,
    pub last_seen_ms: i64,
    pub started_at_ms: Option<i64>,
    /// 当前会话本轮累计 Token；没有可信任务起点时未知。
    pub running_tokens: Option<i64>,
    /// running / done / failed；没有可信生命周期的来源使用 recent。
    pub state: String,
}

#[derive(Debug, Serialize)]
pub struct LiveUsage {
    /// 本次快照所属平台；前端切换平台后据此丢弃迟到事件。
    pub platform: String,
    /// 数据库游标，调用方下次带回来即可拿到「自那以后」的新增
    pub cursor: i64,
    /// 自 since 之后新增的 Token 合计
    pub delta_tokens: i64,
    /// 新增涉及的会话（用于展开态按会话展示明细）
    pub delta_sessions: Vec<ActiveSession>,
    /// Codex 为生命周期确认的运行会话；其他来源暂为近期活跃会话
    pub active_sessions: Vec<ActiveSession>,
    /// 正在执行的本轮真实累计值，可在页面重开后恢复；没有可信起点时未知。
    pub running_tokens: Option<i64>,
    /// 0 表示由生命周期驱动，不因静默过期；正数只用于“最近活跃”来源。
    pub active_window_seconds: i64,
    pub today_tokens: Option<i64>,
    /// 新增记录是否来自刚发生的实时写入；历史补采不得播放增量动画。
    pub realtime_delta: bool,
    /// 首次加载：调用方不得据此播放增量动画（§2.1.1）
    pub initial: bool,
}

/// 尚无可信生命周期来源的“最近活跃”窗口，不得将它显示成运行状态。
pub const ACTIVE_WINDOW_SECONDS: i64 = 90;

/// 「运行中 Token」合计只统计 running 会话；失败会话仅自带本轮最终值用于冻结展示。
/// 没有任何 running 会话时返回 None。
pub fn sum_running_tokens(sessions: &[ActiveSession]) -> Option<i64> {
    match sessions.iter()
        .filter(|session| session.state == "running")
        .map(|session| session.running_tokens)
        .collect::<Option<Vec<_>>>()
    {
        Some(values) if values.is_empty() => None,
        Some(values) => Some(values.iter().sum()),
        None => None,
    }
}

/// 本机今日 Token（新鲜口径）：Claude 取输入+输出，Codex 扣除缓存重读与缓存写入，
/// 与灵动岛「本机今日 Token」同源（§2.1.2）；托盘摘要的 API Key 无套餐视图也复用。
/// 区间内无记录是真实的 0；存在 total 未知的历史行则返回 None（界面显示「—」，不补零）。
pub fn today_fresh_tokens(conn: &Connection, platform: &str) -> rusqlite::Result<Option<i64>> {
    if crate::platforms::native(platform) {
        let (start,end)=period_range(conn,platform,"today");
        return conn.query_row("SELECT CASE WHEN COUNT(*)=0 THEN NULL WHEN MIN(total_known)=1 THEN SUM(total_tokens) END FROM requests WHERE platform=?1 AND ts>=?2 AND ts<?3",params![platform,start,end],|r|r.get(0));
    }
    let (today_start, today_end) = period_range(conn, platform, "today");
    let (tc, te) = platform_clause_n(platform, 3);
    // 灵动岛「本机今日 Token」与终端口径一致：新鲜 Token。
    // Claude 的缓存单列，Codex 的 input_tokens 已含缓存，因此 Codex 需要扣除缓存重读；
    // 主面板统计仍用 total_tokens（含缓存）并另行标注。
    let today_sql = format!(
        "SELECT CASE WHEN COUNT(*) = 0 THEN 0
                     WHEN MIN(total_known) = 1 THEN SUM(
                         CASE WHEN platform = 'codex'
                              THEN MAX(input_tokens - cache_read_tokens - cache_write_tokens, 0) + output_tokens
                              ELSE input_tokens + output_tokens
                         END)
                     ELSE NULL END
         FROM requests WHERE ts >= ?1 AND ts < ?2{EXCLUDE_DEGENERATE_SQL}{tc}"
    );
    if te.is_empty() {
        conn.query_row(&today_sql, params![today_start, today_end], |r| r.get(0))
    } else {
        conn.query_row(&today_sql, params![today_start, today_end, te[0]], |r| r.get(0))
    }
}

pub fn live_usage(
    conn: &Connection,
    platform: &str,
    since: Option<i64>,
) -> rusqlite::Result<LiveUsage> {
    let cursor: i64 = conn.query_row("SELECT value FROM event_sequence WHERE singleton=1", [], |r| r.get(0))?;
    // 连表查询里 requests 与 sessions 都有 platform 列，必须带别名限定，否则 SQLite 报歧义
    let (clause, extra) = if platform == "all" {
        (String::new(), vec![])
    } else {
        (" AND r.platform = ?2".to_string(), vec![platform.to_string()])
    };

    let read_sessions = |sql: &str, a: i64| -> rusqlite::Result<Vec<ActiveSession>> {
        let mut stmt = conn.prepare(sql)?;
        let map = |r: &rusqlite::Row| -> rusqlite::Result<ActiveSession> {
            let sid: Option<String> = r.get(0)?;
            let sid = sid.unwrap_or_default();
            let title: Option<String> = r.get(1)?;
            Ok(ActiveSession {
                title: title.filter(|t| !t.trim().is_empty()).unwrap_or_else(|| {
                    "未命名会话".to_string()
                }),
                session_id: sid,
                delta_tokens: r.get(2)?,
                last_seen_ms: r.get(3)?,
                started_at_ms: None,
                running_tokens: None,
                state: "recent".into(),
            })
        };
        let rows = if extra.is_empty() {
            stmt.query_map(params![a], map)?.collect::<Result<_, _>>()?
        } else {
            stmt.query_map(params![a, extra[0]], map)?.collect::<Result<_, _>>()?
        };
        Ok(rows)
    };

    let group_sql = |cond: &str| {
        format!(
            "SELECT r.session_id, s.title, SUM(r.total_tokens), MAX(r.ts)
             FROM requests r
             LEFT JOIN sessions s
               ON s.platform = r.platform AND s.session_id = r.session_id
             WHERE {cond}{clause}
             GROUP BY r.session_id ORDER BY MAX(r.ts) DESC"
        )
    };

    let delta_sessions = match since {
        Some(id) => {
            let delta_clause = if platform == "all" { "" } else { " AND e.platform = ?2" };
            let sql = format!(
                "SELECT e.session_id, s.title, SUM(COALESCE(e.delta_fresh, e.delta_total)), MAX(e.ts)
                 FROM usage_events e
                 LEFT JOIN sessions s
                   ON s.platform = e.platform AND s.session_id = e.session_id
                 WHERE e.id > ?1{delta_clause}
                 GROUP BY e.session_id ORDER BY MAX(e.ts) DESC"
            );
            let mut stmt = conn.prepare(&sql)?;
            let map = |row: &rusqlite::Row| -> rusqlite::Result<ActiveSession> {
                let title: Option<String> = row.get(1)?;
                Ok(ActiveSession {
                    session_id: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    title: title.filter(|value| !value.trim().is_empty())
                        .unwrap_or_else(|| "未命名会话".into()),
                    delta_tokens: row.get(2)?,
                    last_seen_ms: row.get(3)?,
                    started_at_ms: None,
                running_tokens: None,
                    state: "recent".into(),
                })
            };
            if platform == "all" {
                stmt.query_map(params![id], map)?.collect::<Result<_, _>>()?
            } else {
                stmt.query_map(params![id, platform], map)?.collect::<Result<_, _>>()?
            }
        }
        None => vec![],
    };
    let delta_tokens = delta_sessions.iter().map(|s| s.delta_tokens).sum();
    let realtime_delta = delta_sessions.iter().map(|session| session.last_seen_ms).max()
        .is_some_and(|last_seen| last_seen >= Local::now().timestamp_millis() - 30_000);

    // 思考、等待输入和压缩可以长时间没有新日志；静默不代表本轮结束。
    let lifecycle = matches!(platform, "codex" | "claude");
    let active_window_seconds = if lifecycle { 0 } else { ACTIVE_WINDOW_SECONDS };
    let active_from = Local::now().timestamp_millis() - active_window_seconds * 1_000;
    // 最终失败的会话短暂保留在列表里展示「失败」，超过窗口后自动消失。
    let failed_from = Local::now().timestamp_millis() - ACTIVE_WINDOW_SECONDS * 1_000;
    // Codex 必须由明确生命周期驱动；迁移后的旧记录默认停止，不能再用近期 Token 猜运行态。
    let mut active_sessions = if lifecycle {
        // 会话按首次出现时间降序——最新的会话在最前面（2026-09-19 鼠鼠定版）。
        // 首次时间取该会话请求里的最早时间戳：Token 更新只改活跃度、不改首次时间，
        // 列表因此不随增量刷新重排，只有新会话出现时才在最前插入。
        // 首次时间未知（无请求记录的残留行）排最后，再按活跃度降序兜底。
        let mut stmt = conn.prepare(
            "SELECT session_id, title, 0, activity_updated_ms, activity_state, turn_started_ms,
                    (SELECT MIN(r.ts) FROM requests r
                      WHERE r.platform = sessions.platform AND r.session_id = sessions.session_id)
                      AS first_seen_ms
             FROM sessions
             WHERE platform = ?1
               AND (activity_state = 'running'
                    OR (activity_state = 'failed' AND activity_updated_ms >= ?2))
             ORDER BY first_seen_ms DESC, activity_updated_ms DESC",
        )?;
        let rows = stmt.query_map(params![platform, failed_from], |r| {
            let title: Option<String> = r.get(1)?;
            Ok(ActiveSession {
                session_id: r.get(0)?,
                title: title.filter(|t| !t.trim().is_empty()).unwrap_or_else(|| "未命名会话".into()),
                delta_tokens: r.get(2)?,
                last_seen_ms: r.get(3)?,
                started_at_ms: r.get(5)?,
                running_tokens: None,
                state: r.get(4)?,
            })
        })?
        .collect::<Result<_, _>>()?;
        rows
    } else {
        read_sessions(&group_sql("r.ts >= ?1"), active_from)?
    };

    for session in &mut active_sessions {
        if lifecycle && session.started_at_ms.is_some() {
            // 实时口径对齐 Claude Code 终端：只算新鲜 Token（input+output），
            // 缓存重读不计——否则每轮 +百万级全是重复上下文（见 usage_events.delta_fresh）。
            session.running_tokens = Some(conn.query_row(
                "SELECT COALESCE(SUM(COALESCE(delta_fresh, delta_total)), 0) FROM usage_events
                 WHERE platform = ?3 AND session_id = ?1 AND ts >= ?2",
                params![session.session_id, session.started_at_ms, platform],
                |row| row.get::<_, i64>(0),
            )?);
        }
    }
    let running_tokens = sum_running_tokens(&active_sessions);

    let today_tokens = today_fresh_tokens(conn, platform)?;

    Ok(LiveUsage {
        platform: platform.to_string(),
        cursor,
        delta_tokens,
        delta_sessions,
        active_sessions,
        running_tokens,
        active_window_seconds,
        today_tokens,
        realtime_delta,
        initial: since.is_none(),
    })
}

/* ------------------------------------------------------------------------ */
/*  自然周期：使用系统时区；本周从周一 00:00 起，本月从月初起                 */
/* ------------------------------------------------------------------------ */

fn start_of_today_at(end: i64) -> DateTime<Local> {
    let now = Local.timestamp_millis_opt(end).single().unwrap_or_else(Local::now);
    local_midnight(now.date_naive()).unwrap_or(now)
}

fn local_midnight(date: chrono::NaiveDate) -> Option<DateTime<Local>> {
    let naive = date.and_hms_opt(0, 0, 0)?;
    Local.from_local_datetime(&naive).earliest()
}

fn start_of_week_at(end: i64) -> DateTime<Local> {
    let today = start_of_today_at(end);
    // 在本地日历日期上回退到周一，再重新解析当地午夜；不能从带时区时间戳
    // 直接减 24 小时，否则跨夏令时时可能落到 23:00 或 01:00。
    let monday = today.date_naive()
        - Duration::days(today.weekday().num_days_from_monday() as i64);
    local_midnight(monday).unwrap_or(today)
}

fn start_of_month_at(end: i64) -> DateTime<Local> {
    let today = start_of_today_at(end);
    chrono::NaiveDate::from_ymd_opt(today.year(), today.month(), 1)
        .and_then(local_midnight)
        .unwrap_or(today)
}

/// 返回 [start, end) 的 UTC 毫秒。`total` 的起点取最早记录，不能称为账号终生消耗
/// 自定义时间范围（§2.3）。两端都是本地时区的毫秒时间戳。
/// `end` 为 None 表示「结束时间跟随当前时刻」，每次查询取当时的现在。
#[derive(Debug, Clone, Copy, Default, serde::Deserialize)]
pub struct CustomRange {
    pub start: i64,
    pub end: Option<i64>,
}

/// 解析区间：period == "custom" 时用显式范围，否则按预设周期。
/// 自定义范围只筛选历史 Token 查询，不影响额度窗口（§2.3）。
pub fn resolve_range_at(
    conn: &Connection,
    platform: &str,
    period: &str,
    custom: Option<CustomRange>,
    query_end_ms: i64,
) -> (i64, i64) {
    match (period, custom) {
        ("custom", Some(r)) => (r.start, r.end.unwrap_or(query_end_ms)),
        _ => period_range_at(conn, platform, period, query_end_ms),
    }
}

pub fn period_range_at(conn: &Connection, platform: &str, period: &str, end: i64) -> (i64, i64) {
    let start = match period {
        "today" => start_of_today_at(end).timestamp_millis(),
        "week" => start_of_week_at(end).timestamp_millis(),
        "month" => start_of_month_at(end).timestamp_millis(),
        _ if platform == "all" => conn
            .query_row("SELECT MIN(ts) FROM requests", [], |r| r.get::<_, Option<i64>>(0))
            .ok().flatten().unwrap_or(end),
        _ => conn
            .query_row(
                "SELECT MIN(ts) FROM requests WHERE platform = ?1",
                params![platform],
                |r| r.get::<_, Option<i64>>(0),
            )
            .ok().flatten().unwrap_or(end),
    };
    (start, end)
}

pub fn period_range(conn: &Connection, platform: &str, period: &str) -> (i64, i64) {
    period_range_at(conn, platform, period, Local::now().timestamp_millis())
}

/// `idx` 是平台参数在该 SQL 中的占位序号
fn platform_clause_n(platform: &str, idx: usize) -> (String, Vec<String>) {
    if platform == "all" {
        (String::new(), vec![])
    } else {
        (format!(" AND platform = ?{idx}"), vec![platform.to_string()])
    }
}

/// 平台 + 模型过滤。两个维度都可缺省，占位序号由构造时给定的起点依次排开，
/// 调用方不再手工数 `?3` / `?4`——统计、趋势、费用和日志四类查询共用同一套过滤条件，
/// 序号一旦算错就是静默的错筛，而不是编译错误。
struct Filter {
    clause: String,
    args: Vec<rusqlite::types::Value>,
}

impl Filter {
    /// `first` 是第一个可选参数的占位序号；它前面的序号留给各查询自己的固定参数。
    fn new(platform: &str, model: Option<&str>, first: usize) -> Self {
        let mut clause = String::new();
        let mut args = Vec::new();
        if platform != "all" {
            clause.push_str(&format!(" AND platform = ?{}", first + args.len()));
            args.push(rusqlite::types::Value::Text(platform.to_string()));
        }
        // 模型名允许为空字符串（来源没给模型时记录为 NULL），"all" 作为「不筛选」的哨兵；
        // 真有模型叫 all 也不会误伤——前端传的是 None 而非字符串 "all"。
        if let Some(name) = model {
            clause.push_str(&format!(" AND model = ?{}", first + args.len()));
            args.push(rusqlite::types::Value::Text(name.to_string()));
        }
        Self { clause, args }
    }

    /// 把固定参数与过滤参数拼成一条完整的参数表。
    fn params(&self, fixed: &[i64]) -> Vec<rusqlite::types::Value> {
        let mut out: Vec<_> = fixed.iter().map(|v| rusqlite::types::Value::Integer(*v)).collect();
        out.extend(self.args.iter().cloned());
        out
    }
}

/// 区间内出现过的模型名，供筛选下拉使用。NULL（来源未提供模型）不进列表——
/// 它不是一个可供选择的模型，而是「这条记录没说」。
pub fn list_models_at(
    conn: &Connection,
    platform: &str,
    period: &str,
    custom: Option<CustomRange>,
    query_end_ms: i64,
) -> rusqlite::Result<Vec<String>> {
    let (start, end) = resolve_range_at(conn, platform, period, custom, query_end_ms);
    let filter = Filter::new(platform, None, 3);
    let sql = format!(
        "SELECT DISTINCT model FROM requests
         WHERE ts >= ?1 AND ts < ?2 AND model IS NOT NULL AND model <> ''{}
         ORDER BY model",
        filter.clause
    );
    let mut stmt = conn.prepare(&sql)?;
    let models = stmt
        .query_map(rusqlite::params_from_iter(filter.params(&[start, end])), |r| r.get(0))?
        .collect();
    models
}

#[derive(Debug, Serialize)]
pub struct PeriodTotals {
    pub today: Option<i64>,
    pub week: Option<i64>,
    pub month: Option<i64>,
    pub total: Option<i64>,
    /// 自定义区间的合计；未使用自定义时间时为 None
    pub custom: Option<i64>,
    /// 累计的采集起点（本地时间字符串），用于「采集起点 2026/03/02」这类说明
    pub collected_since: Option<String>,
}

pub fn token_totals_at(
    conn: &Connection,
    platform: &str,
    custom: Option<CustomRange>,
    model: Option<&str>,
    query_end_ms: i64,
) -> rusqlite::Result<PeriodTotals> {
    let sum_range = |start: i64, end: i64| -> rusqlite::Result<Option<i64>> {
        let filter = Filter::new(platform, model, 3);
        let sql = format!(
            "SELECT CASE WHEN COUNT(*) = 0 THEN {empty}
                         WHEN MIN(total_known) = 1 THEN SUM(total_tokens)
                         ELSE NULL END
             FROM requests WHERE ts >= ?1 AND ts < ?2{EXCLUDE_DEGENERATE_SQL}{}",
            filter.clause, empty=if crate::platforms::native(platform) { "NULL" } else { "0" }
        );
        conn.query_row(&sql, rusqlite::params_from_iter(filter.params(&[start, end])), |r| r.get(0))
    };

    let mut out = [Some(0i64); 4];
    for (i, period) in ["today", "week", "month", "total"].iter().enumerate() {
        let (start, end) = period_range_at(conn, platform, period, query_end_ms);
        out[i] = sum_range(start, end)?;
    }

    // 自定义区间与四个预设周期并列返回，界面切到「自定义时间」时用它
    let custom_total = match custom {
        Some(_) => {
            let (s, e) = resolve_range_at(conn, platform, "custom", custom, query_end_ms);
            sum_range(s, e)?
        }
        None => None,
    };
    // 采集起点跟随当前筛选：筛了模型就显示该模型最早的记录时间，
    // 否则「累计 X（采集起点 Y）」里的两个数会来自不同范围，自相矛盾。
    let since_filter = Filter::new(platform, model, 1);
    let since_sql = format!(
        "SELECT MIN(ts) FROM requests WHERE 1 = 1{}",
        since_filter.clause
    );
    let collected_since = conn.query_row(
        &since_sql,
        rusqlite::params_from_iter(since_filter.params(&[])),
        |r| r.get::<_, Option<i64>>(0),
    )?
        .map(|ms| {
            Local
                .timestamp_millis_opt(ms)
                .single()
                .map(|d| d.format("%Y/%m/%d").to_string())
                .unwrap_or_default()
        });
    Ok(PeriodTotals {
        today: out[0],
        week: out[1],
        month: out[2],
        total: out[3],
        custom: custom_total,
        collected_since,
    })
}

/// 区间内的 Token 分项。每一项都可能未知——来源没提供该字段时不能当成 0
/// （§DATA-09）：把「没说」画成 0 会让用户以为真的没有缓存命中。
#[derive(Debug, Serialize)]
pub struct UsageBreakdown {
    /// 新增输入：已扣除缓存重读，两个平台口径一致（见 `FRESH_INPUT_SQL`）
    pub fresh_input: Option<i64>,
    pub output: Option<i64>,
    pub cache_write: Option<i64>,
    pub cache_read: Option<i64>,
    /// 真实消耗，直接取 `total_tokens` 之和，与四张周期卡同源
    pub real_total: Option<i64>,
    /// 请求条数。本地记录可精确计数，不会未知
    pub requests: i64,
    /// 缓存命中率 0–1；任一分量未知则为 None，不画进度条
    pub cache_hit_rate: Option<f64>,
    /// 平均每次请求消耗；无请求或总量未知时为 None
    pub avg_per_request: Option<f64>,
}

/// 「新增输入」的跨平台归一化。
///
/// Claude 的 `input_tokens` 本就不含缓存（缓存读写各自单列），直接用；
/// Codex 的 `input_tokens` **已含** `cached_input_tokens`（见 collector.rs 对
/// `token_usage_record` 的注释），必须减掉，否则新增输入会把缓存重读算进去而虚高。
/// 减法夹到 0：来源偶发的 cached > input 不应产生负数。
const FRESH_INPUT_SQL: &str =
    "CASE input_semantics WHEN 'includes_cache' THEN max(input_tokens - cache_read_tokens, 0) WHEN 'excludes_cache' THEN input_tokens ELSE NULL END";

/// 同理，新增输入的「是否已知」在 Codex 上依赖两个字段：缺任何一个都算未知。
const FRESH_INPUT_KNOWN_SQL: &str =
    "CASE input_semantics WHEN 'includes_cache' THEN input_known * cache_read_known WHEN 'excludes_cache' THEN input_known ELSE 0 END";

/// 退化行排除（2026-09-19 鼠鼠定版）：已知 Token 字段全为 0 却缺任一缓存字段的
/// 记录——实测来自第三方网关的空响应（usage 只有 input/output 两个 0）——本身
/// 不携带可统计信息，却会让区间的合计、命中率与费用整体变「未知」。统计聚合
/// 一律跳过它；记录保留在库中，请求日志（request_log_at）特意不加本条件，照常显示。
/// 限定 claude：Codex 协议本就不上报缓存写入，缺字段在那边是常态而非退化。
const EXCLUDE_DEGENERATE_SQL: &str = " AND NOT (platform = 'claude' AND input_known = 1 AND input_tokens = 0 \
     AND output_known = 1 AND output_tokens = 0 \
     AND (cache_read_known = 0 OR cache_write_known = 0))";

pub fn usage_breakdown_at(
    conn: &Connection,
    platform: &str,
    period: &str,
    custom: Option<CustomRange>,
    model: Option<&str>,
    query_end_ms: i64,
) -> rusqlite::Result<UsageBreakdown> {
    let (start, end) = resolve_range_at(conn, platform, period, custom, query_end_ms);
    let filter = Filter::new(platform, model, 3);
    let sql = format!(
        "SELECT COUNT(*),
                MIN({FRESH_INPUT_KNOWN_SQL}), MIN(output_known), MIN(cache_write_known),
                MIN(cache_read_known), MIN(total_known),
                SUM({FRESH_INPUT_SQL}), SUM(output_tokens), SUM(cache_write_tokens),
                SUM(cache_read_tokens), SUM(total_tokens)
         FROM requests WHERE ts >= ?1 AND ts < ?2{EXCLUDE_DEGENERATE_SQL}{}",
        filter.clause
    );
    let row = conn.query_row(
        &sql,
        rusqlite::params_from_iter(filter.params(&[start, end])),
        |r| {
            let known = |index: usize| -> rusqlite::Result<bool> {
                Ok(r.get::<_, Option<i64>>(index)?.unwrap_or(0) != 0)
            };
            Ok((
                r.get::<_, i64>(0)?,
                (known(1)?, known(2)?, known(3)?, known(4)?, known(5)?),
                (
                    r.get::<_, Option<i64>>(6)?,
                    r.get::<_, Option<i64>>(7)?,
                    r.get::<_, Option<i64>>(8)?,
                    r.get::<_, Option<i64>>(9)?,
                    r.get::<_, Option<i64>>(10)?,
                ),
            ))
        },
    )?;
    let (requests, (fi_known, o_known, cw_known, cr_known, t_known), (fi, o, cw, cr, total)) = row;

    // 没有任何记录时四项都是真实的 0，不是未知：区间内确实没用过。
    let at = |known: bool, value: Option<i64>| -> Option<i64> {
        if requests == 0 {
            if crate::platforms::native(platform) { None } else { Some(0) }
        } else if known {
            value
        } else {
            None
        }
    };
    let fresh_input = at(fi_known, fi);
    let output = at(o_known, o);
    let cache_write = at(cw_known, cw);
    let cache_read = at(cr_known, cr);
    let real_total = at(t_known, total);

    // 命中率分母是「本可以命中的输入」：新增输入 + 缓存写入 + 缓存命中。
    // 任一分量未知则整个比率未知——用已知的部分凑一个分母会低估分母、高估命中率。
    let denominator_sql = format!("SELECT CASE WHEN MIN(CASE input_semantics
        WHEN 'includes_cache' THEN input_known * cache_read_known
        WHEN 'excludes_cache' THEN input_known * cache_read_known * cache_write_known ELSE 0 END)=1
        THEN SUM(CASE input_semantics WHEN 'includes_cache' THEN input_tokens
          WHEN 'excludes_cache' THEN input_tokens+cache_read_tokens+cache_write_tokens END) END
        FROM requests WHERE ts>=?1 AND ts<?2{EXCLUDE_DEGENERATE_SQL}{}", filter.clause);
    let denominator:Option<i64> = conn.query_row(&denominator_sql,
        rusqlite::params_from_iter(filter.params(&[start,end])), |r|r.get(0))?;
    let cache_hit_rate = cache_read.zip(denominator).and_then(|(r,d)| (d>0 && r<=d).then_some(r as f64/d as f64));
    let avg_per_request = match real_total {
        Some(t) if requests > 0 => Some(t as f64 / requests as f64),
        _ => None,
    };

    Ok(UsageBreakdown {
        fresh_input,
        output,
        cache_write,
        cache_read,
        real_total,
        requests,
        cache_hit_rate,
        avg_per_request,
    })
}

#[derive(Debug, Serialize)]
pub struct TrendPoint {
    pub label: String,
    /// 总量，保持原字段名与语义；界面切「总量」视图时用它
    pub tokens: Option<i64>,
    pub fresh_input: Option<i64>,
    pub output: Option<i64>,
    pub cache_write: Option<i64>,
    pub cache_read: Option<i64>,
    /// 该桶的估算费用；桶内存在无价目模型或缺字段时为 None，界面断线而不画 0
    pub cost: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct Trend {
    pub points: Vec<TrendPoint>,
    /// 实际分桶粒度，界面需明示（§7.5）
    pub bucket: String,
}

/// 今日按小时、周/月按日；长时间范围降低点数而非加载全部日志
#[cfg(test)]
pub fn trend(
    conn: &Connection,
    platform: &str,
    period: &str,
    custom: Option<CustomRange>,
) -> rusqlite::Result<Trend> {
    trend_at(conn, platform, period, custom, None, Local::now().timestamp_millis())
}

pub fn trend_at(
    conn: &Connection,
    platform: &str,
    period: &str,
    custom: Option<CustomRange>,
    model: Option<&str>,
    query_end_ms: i64,
) -> rusqlite::Result<Trend> {
    let (start, end) = resolve_range_at(conn, platform, period, custom, query_end_ms);
    const HOUR: i64 = 3_600_000;
    const DAY: i64 = 24 * HOUR;
    const MAX_POINTS: i64 = 180;
    let hourly = period == "today" || (period == "custom" && end - start <= 2 * DAY);
    let raw_days = ((end - start).max(1) + DAY - 1) / DAY;
    let days_per_bucket = if hourly { 0 } else { (raw_days + MAX_POINTS - 1) / MAX_POINTS };
    let bucket_ms = if hourly { HOUR } else { days_per_bucket.max(1) * DAY };
    let count = if period == "today" { 24 } else {
        (((end - start).max(1) + bucket_ms - 1) / bucket_ms).clamp(1, MAX_POINTS)
    } as usize;
    // 按「桶 × 平台 × 模型」聚合：分项展示只需桶，但计价必须知道是哪个平台的哪个模型，
    // 所以一次查询同时带回两者，前端要的五个系列和费用都从这一份结果里算出来。
    let filter = Filter::new(platform, model, 4);
    let sql = format!(
        "SELECT CAST((ts - ?1) / ?3 AS INTEGER) AS bucket_index, platform, model,
                SUM(total_tokens), SUM({FRESH_INPUT_SQL}), SUM(output_tokens),
                SUM(cache_write_tokens), SUM(cache_read_tokens), SUM(input_tokens),
                MIN(input_known), MIN(output_known), MIN(cache_write_known),
                MIN(cache_read_known), MIN(total_known), MIN({FRESH_INPUT_KNOWN_SQL})
         FROM requests WHERE ts >= ?1 AND ts < ?2{EXCLUDE_DEGENERATE_SQL}{}
         GROUP BY bucket_index, platform, model",
        filter.clause
    );
    let mut stmt = conn.prepare(&sql)?;
    #[derive(Default, Clone)]
    struct Bucket {
        tokens: i64,
        fresh_input: i64,
        output: i64,
        cache_write: i64,
        cache_read: i64,
        cost: f64,
        /// 桶内出现过无法计价的记录（缺字段或模型不在价目表），费用整桶作废
        cost_known: bool,
        known: [bool; 5],
        has_records: bool,
    }
    let mut buckets = vec![Bucket { cost_known: !crate::platforms::native(platform), known: [!crate::platforms::native(platform); 5], ..Default::default() }; count];
    let map = |row: &rusqlite::Row| -> rusqlite::Result<_> {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            [
                row.get::<_, i64>(3)?, row.get::<_, Option<i64>>(4)?.unwrap_or(0), row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?, row.get::<_, i64>(7)?, row.get::<_, i64>(8)?,
            ],
            (9..=13).all(|index| row.get::<_, i64>(index).unwrap_or(0) != 0),
            [13,14,10,11,12].map(|index| row.get::<_, i64>(index).unwrap_or(0) != 0),
        ))
    };
    let rows: Vec<_> = stmt
        .query_map(
            rusqlite::params_from_iter(filter.params(&[start, end, bucket_ms])),
            map,
        )?
        .collect::<Result<_, _>>()?;
    for (index, row_platform, row_model, sums, fields_known, known) in rows {
        if index < 0 || (index as usize) >= buckets.len() {
            continue;
        }
        let [total, fresh_input, output, cache_write, cache_read, raw_input] = sums;
        let bucket = &mut buckets[index as usize];
        if !bucket.has_records {bucket.known=known;bucket.cost_known=true;bucket.has_records=true;}
        else {for (value, incoming) in bucket.known.iter_mut().zip(known) { *value &= incoming; }}
        bucket.tokens += total;
        bucket.fresh_input += fresh_input;
        bucket.output += output;
        bucket.cache_write += cache_write;
        bucket.cache_read += cache_read;
        // 计价拿的是**原始** input_tokens：Codex 的缓存扣除在 pricing::estimate 内部完成
        // （见 QUOTA-13），这里先减一次就会重复扣减，费用被系统性低估。
        match fields_known
            .then(|| {
                crate::pricing::estimate(
                    Some(&row_platform), row_model.as_deref(), raw_input, output, cache_write, cache_read,
                )
            })
            .flatten()
        {
            Some(value) => bucket.cost += value,
            None => bucket.cost_known = false,
        }
    }
    let bucket = if hourly { "1 小时".to_string() } else { format!("{} 天", days_per_bucket.max(1)) };
    let start_year = Local.timestamp_millis_opt(start).single().map(|date| date.year());
    let end_year = Local.timestamp_millis_opt(end.saturating_sub(1)).single().map(|date| date.year());
    let crosses_year = start_year != end_year;
    Ok(Trend {
        points: buckets
            .into_iter()
            .enumerate()
            .map(|(index, bucket)| {
                let timestamp = start + index as i64 * bucket_ms;
                let label = Local.timestamp_millis_opt(timestamp).single().map(|date| {
                    if period == "today" { date.format("%H").to_string() }
                    else if hourly && crosses_year { date.format("%Y/%m/%d %H").to_string() }
                    else if hourly { date.format("%m/%d %H").to_string() }
                    else if days_per_bucket > 1 || crosses_year { date.format("%Y/%m/%d").to_string() }
                    else { date.format("%m/%d").to_string() }
                }).unwrap_or_default();
                TrendPoint {
                    label,
                    tokens: bucket.known[0].then_some(bucket.tokens),
                    fresh_input: bucket.known[1].then_some(bucket.fresh_input),
                    output: bucket.known[2].then_some(bucket.output),
                    cache_write: bucket.known[3].then_some(bucket.cache_write),
                    cache_read: bucket.known[4].then_some(bucket.cache_read),
                    cost: bucket.cost_known.then_some(bucket.cost),
                }
            })
            .collect(),
        bucket,
    })
}

#[derive(Debug, Serialize)]
pub struct LogRow {
    pub native_outcome: Option<String>,
    pub id: i64,
    pub time: String,
    pub date: String,
    pub model: Option<String>,
    pub input_semantics: String,
    pub input: Option<i64>,
    pub output: Option<i64>,
    pub cache_read: Option<i64>,
    pub cache_write: Option<i64>,
    pub total: Option<i64>,
    pub session_id: Option<String>,
    pub platform: String,
    /// 估算金额（美元）。本地记录不含账单，故必须标注「估算」；
    /// 模型不在价目表内时为 None，界面显示「—」而不是 0
    pub cost_estimate: Option<f64>,
    /// 思考强度，来源原样透传。加列之前的历史记录为 None，界面显示「—」
    pub effort: Option<String>,
    /// 请求总耗时（毫秒）。只有经本地代理转发的请求才有，否则 None
    pub duration_ms: Option<i64>,
    /// 首字延迟（毫秒），同上
    pub first_token_ms: Option<i64>,
    /// 上游真实 HTTP 响应码，同上。没有时前端按「成功」展示（能写入用量即响应已返回），不补造 200
    pub status_code: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct LogPage {
    pub rows: Vec<LogRow>,
    /// 总条数；本地来源可精确计数，因此不为 null
    pub total_count: i64,
    pub page: u32,
    pub page_size: u32,
    pub page_count: u32,
}

/// 区间内的估算费用合计（§2.4）。
/// `uncovered_tokens` 是价目表未覆盖模型的 Token 数——不为零时界面需说明
/// 「部分模型无价目」，否则用户会以为这就是全部花费。
#[derive(Debug, Serialize)]
pub struct CostEstimate {
    pub amount: f64,
    /// 恒为 true：本地记录推算出的都是估算值，不是账单
    pub estimated: bool,
    pub uncovered_tokens: i64,
    /// false 表示区间内至少一条记录缺少计价所需 Token 字段。
    pub complete: bool,
}

pub fn cost_estimate_at(
    conn: &Connection,
    platform: &str,
    period: &str,
    custom: Option<CustomRange>,
    model: Option<&str>,
    query_end_ms: i64,
) -> rusqlite::Result<CostEstimate> {
    let (start, end) = resolve_range_at(conn, platform, period, custom, query_end_ms);
    let filter = Filter::new(platform, model, 3);
    let sql = format!(
        "SELECT platform, model, SUM(input_tokens), SUM(output_tokens), SUM(cache_write_tokens),
                SUM(cache_read_tokens), SUM(total_tokens), MIN(input_known), MIN(output_known),
                MIN(cache_write_known), MIN(cache_read_known), MIN(total_known), MIN({FRESH_INPUT_KNOWN_SQL})
         FROM requests WHERE ts >= ?1 AND ts < ?2{EXCLUDE_DEGENERATE_SQL}{}
         GROUP BY platform, model",
        filter.clause
    );
    let mut stmt = conn.prepare(&sql)?;
    let map = |r: &rusqlite::Row| -> rusqlite::Result<(String, Option<String>, i64, i64, i64, i64, i64, bool)> {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?,
            (7..=11).all(|index| r.get::<_, i32>(index).unwrap_or(0) != 0)))
    };
    let rows: Vec<_> = stmt
        .query_map(rusqlite::params_from_iter(filter.params(&[start, end])), map)?
        .collect::<Result<_, _>>()?;

    let mut amount = 0.0;
    let mut uncovered = 0i64;
    let mut complete = true;
    for (row_platform, model, i, o, cw, cr, total, fields_known) in rows {
        if !fields_known {
            complete = false;
            continue;
        }
        match crate::pricing::estimate(Some(&row_platform), model.as_deref(), i, o, cw, cr) {
            Some(v) => amount += v,
            // 价目表没有该模型：计入未覆盖，不当成 0 元悄悄吞掉
            None => uncovered += total,
        }
    }
    Ok(CostEstimate {
        amount,
        estimated: true,
        uncovered_tokens: uncovered,
        complete,
    })
}

/// 默认每页 10 条（界面不显示该文案）；按时间从新到旧
pub const PAGE_SIZE: u32 = 10;

pub fn request_log_at(
    conn: &Connection,
    platform: &str,
    period: &str,
    page: u32,
    custom: Option<CustomRange>,
    model: Option<&str>,
    query_end_ms: i64,
) -> rusqlite::Result<LogPage> {
    let (start, end) = resolve_range_at(conn, platform, period, custom, query_end_ms);
    let filter = Filter::new(platform, model, 3);
    let clause = filter.clause.as_str();

    let count_sql =
        format!("SELECT COUNT(*) FROM requests WHERE ts >= ?1 AND ts < ?2{clause}");
    let total_count: i64 = conn.query_row(
        &count_sql,
        rusqlite::params_from_iter(filter.params(&[start, end])),
        |r| r.get(0),
    )?;

    let page = page.max(1);
    let offset = ((page - 1) * PAGE_SIZE) as i64;
    let sql = format!(
        "SELECT id, ts, model,
                CASE WHEN input_known = 1 THEN input_tokens END,
                CASE WHEN output_known = 1 THEN output_tokens END,
                CASE WHEN cache_read_known = 1 THEN cache_read_tokens END,
                CASE WHEN cache_write_known = 1 THEN cache_write_tokens END,
                CASE WHEN total_known = 1 THEN total_tokens END,
                session_id, platform, effort,
                duration_ms, first_token_ms, status_code, input_semantics, native_outcome
         FROM requests WHERE ts >= ?1 AND ts < ?2{clause}
         ORDER BY ts DESC LIMIT {PAGE_SIZE} OFFSET {offset}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let map = |r: &rusqlite::Row| -> rusqlite::Result<LogRow> {
        let ts: i64 = r.get(1)?;
        let dt = Local.timestamp_millis_opt(ts).single();
        let model: Option<String> = r.get(2)?;
        let input: Option<i64> = r.get(3)?;
        let output: Option<i64> = r.get(4)?;
        let cache_read: Option<i64> = r.get(5)?;
        let cache_write: Option<i64> = r.get(6)?;
        let row_platform: String = r.get(9)?;
        Ok(LogRow {
            id: r.get(0)?,
            time: dt.map(|d| d.format("%H:%M:%S").to_string()).unwrap_or_default(),
            date: dt.map(|d| d.format("%Y/%m/%d").to_string()).unwrap_or_default(),
            cost_estimate: input.zip(output).zip(cache_write).zip(cache_read)
                .and_then(|(((input, output), cache_write), cache_read)| crate::pricing::estimate(
                    Some(&row_platform), model.as_deref(), input, output, cache_write, cache_read,
                )),
            model,
            input_semantics: r.get(14)?,
            native_outcome:r.get(15)?,
            input,
            output,
            cache_read,
            cache_write,
            total: r.get(7)?,
            session_id: r.get(8)?,
            platform: row_platform,
            effort: r.get(10)?,
            duration_ms: r.get(11)?,
            first_token_ms: r.get(12)?,
            status_code: r.get(13)?,
        })
    };
    let rows: Vec<LogRow> = stmt
        .query_map(rusqlite::params_from_iter(filter.params(&[start, end])), map)?
        .collect::<Result<_, _>>()?;

    let page_count = ((total_count as f64) / PAGE_SIZE as f64).ceil() as u32;
    Ok(LogPage {
        rows,
        total_count,
        page,
        page_size: PAGE_SIZE,
        page_count: page_count.max(1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_record_insert_does_not_advance_scan_checkpoint_or_titles() {
        let mut conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        conn.execute_batch(
            "CREATE TRIGGER reject_request BEFORE INSERT ON requests BEGIN SELECT RAISE(ABORT, 'boom'); END;",
        ).unwrap();
        let record = RequestRecord {
            platform: "claude".into(),
            source: "local-session".into(),
            dedup_key: "request-1".into(),
            session_id: Some("session-1".into()),
            ts: 1,
            model: Some("claude-sonnet-4".into()),
            input_tokens: Some(1),
            output_tokens: Some(1),
            cache_read_tokens: Some(0),
            cache_write_tokens: Some(0),
            total_tokens: Some(2), effort: None,
        };

        let result = commit_scanned_file(
            &mut conn,
            &[record],
            &[("claude".into(), "session-1".into(), "标题".into())],
            None,
            "session.jsonl",
            42,
            7,
        );
        assert!(result.is_err());
        assert_eq!(get_offset(&conn, "session.jsonl"), 0);
        let titles: i64 = conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0)).unwrap();
        assert_eq!(titles, 0);
    }

    #[test]
    fn proxy_timing_updates_matching_local_request_and_reports_misses() {
        let mut conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        let record = |key: &str| RequestRecord {
            platform: "claude".into(),
            source: crate::collector::SOURCE_LOCAL.into(),
            dedup_key: key.into(),
            session_id: None, ts: 1, model: None, input_tokens: Some(1), output_tokens: Some(1),
            cache_read_tokens: Some(0), cache_write_tokens: Some(0), total_tokens: Some(2), effort: None,
        };
        insert_records(&mut conn, &[record("msg_known")]).unwrap();

        // 命中：三列写入，且只影响目标行
        let updated = super::apply_proxy_timing(&conn, "msg_known", 320, 4_500, 200).unwrap();
        assert_eq!(updated, 1);
        let (first, duration, status): (Option<i64>, Option<i64>, Option<i64>) = conn.query_row(
            "SELECT first_token_ms, duration_ms, status_code FROM requests WHERE dedup_key = 'msg_known'",
            [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).unwrap();
        assert_eq!((first, duration, status), (Some(320), Some(4_500), Some(200)));

        // 未命中（记录还没入库）：返回 0，调用方转入待关联队列
        let updated = super::apply_proxy_timing(&conn, "msg_missing", 10, 20, 200).unwrap();
        assert_eq!(updated, 0);
    }

    #[test]
    fn total_range_starts_at_the_selected_platform_first_record() {
        let mut conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        let record = |platform: &str, key: &str, ts: i64| RequestRecord {
            platform: platform.into(), source: "local".into(), dedup_key: key.into(),
            session_id: None, ts, model: None, input_tokens: Some(1), output_tokens: Some(0),
            cache_read_tokens: Some(0), cache_write_tokens: Some(0), total_tokens: Some(1), effort: None,
        };
        insert_records(&mut conn, &[record("claude", "c", 100), record("codex", "o", 200)]).unwrap();

        assert_eq!(period_range(&conn, "codex", "total").0, 200);
        assert_eq!(period_range(&conn, "claude", "total").0, 100);
    }

    #[test]
    fn ended_custom_daily_trend_stops_at_custom_end() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        let start = Local.with_ymd_and_hms(2026, 1, 10, 0, 0, 0).single().unwrap().timestamp_millis();
        let end = Local.with_ymd_and_hms(2026, 1, 13, 0, 0, 0).single().unwrap().timestamp_millis();
        let result = trend(&conn, "codex", "custom", Some(CustomRange { start, end: Some(end) })).unwrap();

        assert_eq!(result.points.len(), 3);
        assert_eq!(result.points.first().unwrap().label, "01/10");
        assert_eq!(result.points.last().unwrap().label, "01/12");
    }

    #[test]
    fn all_statistics_use_the_supplied_query_end_snapshot() {
        let mut conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        let record = |key: &str, ts: i64, total: i64| RequestRecord {
            platform: "codex".into(), source: "local".into(), dedup_key: key.into(),
            session_id: None, ts, model: None, input_tokens: Some(total), output_tokens: Some(0),
            cache_read_tokens: Some(0), cache_write_tokens: Some(0), total_tokens: Some(total), effort: None,
        };
        insert_records(&mut conn, &[record("before", 1_000, 3), record("after", 2_000, 30)]).unwrap();
        let range = Some(CustomRange { start: 0, end: None });
        let snapshot = 1_500;

        let totals = token_totals_at(&conn, "codex", range, None, snapshot).unwrap();
        let trend = trend_at(&conn, "codex", "custom", range, None, snapshot).unwrap();
        let log = request_log_at(&conn, "codex", "custom", 1, range, None, snapshot).unwrap();
        let cost = cost_estimate_at(&conn, "codex", "custom", range, None, snapshot).unwrap();

        assert_eq!(totals.custom, Some(3));
        assert_eq!(trend.points.iter().filter_map(|point| point.tokens).sum::<i64>(), 3);
        assert_eq!(log.total_count, 1);
        assert_eq!(log.rows[0].total, Some(3));
        assert_eq!(cost.uncovered_tokens, 3);
    }

    #[test]
    fn cumulative_request_updates_store_final_value_and_emit_only_the_difference() {
        let mut conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        let record = |total: i64| RequestRecord {
            platform: "codex".into(), source: "local".into(), dedup_key: "same-response".into(),
            session_id: Some("session-1".into()), ts: Local::now().timestamp_millis(),
            model: Some("unknown-test-model".into()), input_tokens: Some(total), output_tokens: Some(0),
            cache_read_tokens: Some(0), cache_write_tokens: Some(0), total_tokens: Some(total), effort: None,
        };
        assert_eq!(insert_records(&mut conn, &[record(10)]).unwrap(), 1);
        let baseline = live_usage(&conn, "codex", None).unwrap().cursor;
        assert_eq!(insert_records(&mut conn, &[record(15)]).unwrap(), 1);

        let update = live_usage(&conn, "codex", Some(baseline)).unwrap();
        assert_eq!(update.delta_tokens, 5);
        assert_eq!(update.delta_sessions[0].delta_tokens, 5);
        let stored: (i64, i64) = conn.query_row(
            "SELECT COUNT(*), SUM(total_tokens) FROM requests",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(stored, (1, 15));

        let cursor = update.cursor;
        assert_eq!(insert_records(&mut conn, &[record(15)]).unwrap(), 0);
        let duplicate = live_usage(&conn, "codex", Some(cursor)).unwrap();
        assert_eq!(duplicate.cursor, cursor);
        assert_eq!(duplicate.delta_tokens, 0);
    }

    #[test]
    fn calendar_ranges_keep_local_midnight_and_cross_year_labels_include_year() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        let end = Local.with_ymd_and_hms(2026, 1, 2, 12, 0, 0).single().unwrap().timestamp_millis();
        let (week_start, _) = period_range_at(&conn, "codex", "week", end);
        let week_start = Local.timestamp_millis_opt(week_start).single().unwrap();
        assert_eq!(week_start.weekday().num_days_from_monday(), 0);
        assert_eq!(week_start.format("%H:%M").to_string(), "00:00");

        let start = Local.with_ymd_and_hms(2025, 12, 30, 0, 0, 0).single().unwrap().timestamp_millis();
        let end = Local.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).single().unwrap().timestamp_millis();
        let trend = trend_at(
            &conn,
            "codex",
            "custom",
            Some(CustomRange { start, end: Some(end) }),
            None,
            end,
        ).unwrap();
        assert_eq!(trend.points[0].label, "2025/12/30");
        assert_eq!(trend.points[1].label, "2025/12/31");
        assert_eq!(trend.points[2].label, "2026/01/01");
    }

    #[test]
    fn recent_tokens_without_running_lifecycle_are_not_a_running_session() {
        let mut conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        upsert_session_title(&conn, "codex", "session-1", "已停止的会话").unwrap();
        insert_records(
            &mut conn,
            &[RequestRecord {
                platform: "codex".into(),
                source: "local".into(),
                dedup_key: "response-1".into(),
                session_id: Some("session-1".into()),
                ts: Local::now().timestamp_millis(),
                model: Some("gpt-5".into()),
                input_tokens: Some(10),
                output_tokens: Some(5),
                cache_read_tokens: Some(0),
                cache_write_tokens: Some(0),
                total_tokens: Some(15), effort: None,
            }],
        )
        .unwrap();

        let usage = live_usage(&conn, "codex", None).unwrap();
        assert!(usage.active_sessions.is_empty());
    }

    #[test]
    fn running_token_snapshot_restores_only_current_turn() {
        let mut conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        let now = Local::now().timestamp_millis();
        let record = |key: &str, session: &str, ts, total| RequestRecord {
            platform: "codex".into(), source: "local".into(), dedup_key: key.into(),
            session_id: Some(session.into()), ts, model: None,
            input_tokens: Some(total), output_tokens: Some(0), cache_read_tokens: Some(0),
            cache_write_tokens: Some(0), total_tokens: Some(total), effort: None,
        };
        insert_records(&mut conn, &[
            record("previous-turn", "active", now - 10_000, 900),
            record("current-turn", "active", now, 120),
            record("other-session", "other", now, 500),
        ]).unwrap();
        set_session_activity(&conn, "codex", "active", "running", now).unwrap();
        conn.execute("UPDATE sessions SET turn_started_ms = ?1 WHERE session_id = 'active'", [now - 1000]).unwrap();
        let initial = live_usage(&conn, "codex", None).unwrap();
        assert_eq!(initial.delta_tokens, 0);
        assert_eq!(initial.running_tokens, Some(120));
        assert_eq!(initial.active_sessions[0].running_tokens, Some(120));
        assert_eq!(live_usage(&conn, "codex", Some(initial.cursor)).unwrap().running_tokens, Some(120));
        set_session_activity(&conn, "codex", "other", "running", now).unwrap();
        conn.execute("UPDATE sessions SET turn_started_ms = ?1 WHERE session_id = 'other'", [now - 1000]).unwrap();
        let multiple = live_usage(&conn, "codex", None).unwrap();
        assert_eq!(multiple.running_tokens, Some(620));
        assert_eq!(multiple.active_sessions.iter().find(|s| s.session_id == "active").unwrap().running_tokens, Some(120));
        assert_eq!(multiple.active_sessions.iter().find(|s| s.session_id == "other").unwrap().running_tokens, Some(500));
        conn.execute("UPDATE sessions SET turn_started_ms = ?1 WHERE session_id = 'active'", [now + 1]).unwrap();
        let next = live_usage(&conn, "codex", None).unwrap();
        assert_eq!(next.running_tokens, Some(500));
        assert_eq!(next.active_sessions.iter().find(|s| s.session_id == "active").unwrap().running_tokens, Some(0));
    }

    /// 会话按首次出现时间降序：最新的会话在最前面（2026-09-19 鼠鼠定版）。
    /// Token 更新只改活跃度、不改首次时间，因此老会话持续出 Token 也不会
    /// 跳到新会话上面；列表只在真正出现新会话时才重排。
    #[test]
    fn sessions_order_by_first_seen_not_activity() {
        let mut conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        let now = Local::now().timestamp_millis();
        let record = |key: &str, session: &str, ts| RequestRecord {
            platform: "claude".into(), source: "local".into(), dedup_key: key.into(),
            session_id: Some(session.into()), ts, model: None,
            input_tokens: Some(1), output_tokens: Some(0), cache_read_tokens: Some(0),
            cache_write_tokens: Some(0), total_tokens: Some(1), effort: None,
        };
        // old 会话首次出现更早，但刚刚还在出 Token（活跃度最新）；
        // new 会话刚刚创建（首次时间最新），此后静默。
        insert_records(&mut conn, &[
            record("old-first", "old", now - 60_000),
            record("old-latest", "old", now),
            record("new-first", "new", now - 30_000),
        ]).unwrap();
        set_session_activity(&conn, "claude", "old", "running", now).unwrap();
        set_session_activity(&conn, "claude", "new", "running", now - 30_000).unwrap();

        let sessions: Vec<String> = live_usage(&conn, "claude", None).unwrap()
            .active_sessions.into_iter().map(|s| s.session_id).collect();
        assert_eq!(sessions, vec!["new".to_string(), "old".to_string()],
            "首次时间最新的会话排最前，活跃度不参与排序");
    }

    #[test]
    fn codex_live_usage_excludes_cached_input_tokens() {
        let mut conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        let now = Local::now().timestamp_millis();
        // 查询侧「今日」窗口的上界取的是查询时刻的系统时钟，虚拟机时钟偶发回跳
        // 会把刚插入的记录挤出窗口，导致 today 断言间歇性拿到 0。
        // 记录时间戳因此留出 60 秒余量；午夜前后运行时退化为零点后 1 毫秒，保证仍落今日窗口内。
        let ts = (now - 60_000).max(start_of_today_at(now).timestamp_millis() + 1);
        set_session_activity(&conn, "codex", "cached-turn", "running", now).unwrap();
        conn.execute(
            "UPDATE sessions SET turn_started_ms = ?1 WHERE platform = 'codex' AND session_id = 'cached-turn'",
            [ts - 1_000],
        ).unwrap();
        insert_records(&mut conn, &[RequestRecord {
            platform: "codex".into(),
            source: "local".into(),
            dedup_key: "cached-response".into(),
            session_id: Some("cached-turn".into()),
            ts,
            model: Some("gpt-5".into()),
            // Codex input_tokens 已包含 cached_input_tokens：100 = 20 新鲜 + 80 缓存。
            input_tokens: Some(100),
            output_tokens: Some(20),
            cache_read_tokens: Some(80),
            cache_write_tokens: Some(0),
            total_tokens: Some(120),
            effort: None,
        }]).unwrap();

        let live = live_usage(&conn, "codex", None).unwrap();
        assert_eq!(live.running_tokens, Some(40));
        assert_eq!(live.today_tokens, Some(40));
        // 主面板的总量继续使用来源 total_tokens，保留完整消耗记录。
        assert_eq!(token_totals_at(&conn, "codex", None, None, now + 1).unwrap().today, Some(120));

        // 同一 response 后续上报累计值时，只把新鲜 Token 的差额写入事件。
        let cursor = live.cursor;
        insert_records(&mut conn, &[RequestRecord {
            platform: "codex".into(),
            source: "local".into(),
            dedup_key: "cached-response".into(),
            session_id: Some("cached-turn".into()),
            ts,
            model: Some("gpt-5".into()),
            input_tokens: Some(120),
            output_tokens: Some(30),
            cache_read_tokens: Some(100),
            cache_write_tokens: Some(0),
            total_tokens: Some(150),
            effort: None,
        }]).unwrap();
        let updated = live_usage(&conn, "codex", Some(cursor)).unwrap();
        assert_eq!(updated.delta_tokens, 10);
        assert_eq!(updated.running_tokens, Some(50));
        assert_eq!(updated.today_tokens, Some(50));
    }

    #[test]
    fn failed_sessions_stay_visible_within_window_without_counting_as_running() {
        let mut conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        let now = Local::now().timestamp_millis();
        let record = |key: &str, session: &str, ts, total| RequestRecord {
            platform: "claude".into(), source: "local".into(), dedup_key: key.into(),
            session_id: Some(session.into()), ts, model: None,
            input_tokens: Some(total), output_tokens: Some(0), cache_read_tokens: Some(0),
            cache_write_tokens: Some(0), total_tokens: Some(total), effort: None,
        };
        // 一轮有真实 Token 的失败会话：请求报错后状态转为 failed
        insert_records(&mut conn, &[record("failed-turn", "errored", now - 30_000, 40)]).unwrap();
        set_session_activity(&conn, "claude", "errored", "running", now - 60_000).unwrap();
        conn.execute("UPDATE sessions SET turn_started_ms = ?1 WHERE session_id = 'errored'", [now - 60_000]).unwrap();
        set_session_activity(&conn, "claude", "errored", "failed", now - 30_000).unwrap();
        // 超出 90 秒窗口的旧失败会话不应再出现
        set_session_activity(&conn, "claude", "stale-failed", "failed", now - 120_000).unwrap();

        let usage = live_usage(&conn, "claude", None).unwrap();
        let states: Vec<_> = usage.active_sessions.iter()
            .map(|s| (s.session_id.as_str(), s.state.as_str())).collect();
        assert_eq!(states, vec![("errored", "failed")], "窗口内的失败会话保留 failed 状态，超窗失败不出现");
        assert_eq!(usage.active_sessions[0].running_tokens, Some(40), "失败会话展示本轮最终 Token");
        assert_eq!(usage.running_tokens, None, "没有 running 会话时运行合计为空，失败会话不计入");
        // 有 running 会话并存时，运行合计只统计 running
        insert_records(&mut conn, &[record("live-turn", "live", now, 120)]).unwrap();
        set_session_activity(&conn, "claude", "live", "running", now).unwrap();
        conn.execute("UPDATE sessions SET turn_started_ms = ?1 WHERE session_id = 'live'", [now - 1000]).unwrap();
        let usage = live_usage(&conn, "claude", None).unwrap();
        assert_eq!(usage.running_tokens, Some(120), "失败会话的最终 Token 不得计入运行合计");
        assert_eq!(usage.active_sessions.iter().find(|s| s.session_id == "errored").unwrap().running_tokens, Some(40));
    }

    #[test]
    fn twenty_year_trend_is_bounded_and_preserves_usage_totals() {
        let mut conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        let start = Local.with_ymd_and_hms(2006, 1, 1, 0, 0, 0).single().unwrap().timestamp_millis();
        let end = Local.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).single().unwrap().timestamp_millis();
        let records: Vec<_> = (0..20_000).map(|index| RequestRecord {
            platform: "codex".into(), source: "local".into(), dedup_key: format!("long-{index}"),
            session_id: None, ts: start + (end - start - 1) * index / 19_999, model: None,
            input_tokens: Some(2), output_tokens: Some(1), cache_read_tokens: Some(1),
            cache_write_tokens: Some(0), total_tokens: Some(3), effort: None,
        }).collect();
        insert_records(&mut conn, &records).unwrap();
        let range = Some(CustomRange { start, end: Some(end) });
        let trend = trend_at(&conn, "codex", "custom", range, None, end).unwrap();
        assert!(trend.points.len() <= 180);
        assert_eq!(trend.points.iter().filter_map(|point| point.tokens).sum::<i64>(), 60_000);
        let page = request_log_at(&conn, "codex", "custom", 1, range, None, end).unwrap();
        assert_eq!(page.total_count, 20_000);
        assert_eq!(page.rows.len(), PAGE_SIZE as usize);
    }

    #[test]
    fn fresh_running_lifecycle_is_returned_as_running() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        upsert_session_title(&conn, "codex", "session-1", "正在处理的会话").unwrap();
        conn.execute(
            "UPDATE sessions SET is_running = 1, activity_state = 'running', activity_updated_ms = ?1
             WHERE platform = 'codex' AND session_id = 'session-1'",
            params![Local::now().timestamp_millis()],
        )
        .unwrap();

        let usage = live_usage(&conn, "codex", None).unwrap();
        assert_eq!(usage.active_sessions.len(), 1);
        assert_eq!(usage.active_sessions[0].title, "正在处理的会话");
    }

    #[test]
    fn lifecycle_completion_is_removed_immediately() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        upsert_session_title(&conn, "codex", "session-1", "刚完成的会话").unwrap();
        let now = Local::now().timestamp_millis();

        set_session_activity(&conn, "codex", "session-1", "running", now).unwrap();
        assert_eq!(live_usage(&conn, "codex", None).unwrap().active_sessions.len(), 1);

        set_session_activity(&conn, "codex", "session-1", "done", now + 1).unwrap();
        let completed = live_usage(&conn, "codex", None).unwrap();
        assert!(completed.active_sessions.is_empty());

        conn.execute(
            "UPDATE sessions SET activity_updated_ms = ?1 WHERE platform = 'codex' AND session_id = 'session-1'",
            params![now - 2_000],
        ).unwrap();
        assert!(live_usage(&conn, "codex", None).unwrap().active_sessions.is_empty());
    }

    #[test]
    fn running_session_does_not_expire_while_context_may_be_compacting() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        upsert_session_title(&conn, "codex", "session-1", "正在自动压缩上下文").unwrap();
        let before_long_compaction = Local::now().timestamp_millis() - 60_000;
        set_session_activity(
            &conn,
            "codex",
            "session-1",
            "running",
            before_long_compaction,
        )
        .unwrap();

        let usage = live_usage(&conn, "codex", None).unwrap();
        assert_eq!(usage.active_sessions.len(), 1);
        assert_eq!(usage.active_sessions[0].title, "正在自动压缩上下文");
    }

    #[test]
    fn silent_unfinished_session_keeps_running_until_explicit_end() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        let now = Local::now().timestamp_millis();
        set_session_activity(&conn, "codex", "stale", "running", now - 301_000).unwrap();
        assert_eq!(live_usage(&conn, "codex", None).unwrap().active_sessions.len(), 1);
        set_session_activity(&conn, "codex", "stale", "running", now).unwrap();
        assert_eq!(live_usage(&conn, "codex", None).unwrap().active_sessions.len(), 1);
        set_session_activity(&conn, "codex", "stale", "failed", now).unwrap();
        let sessions = live_usage(&conn, "codex", None).unwrap().active_sessions;
        assert_eq!(sessions.len(), 1, "最终失败后在窗口内短暂展示失败状态");
        assert_eq!(sessions[0].state, "failed");
        conn.execute(
            "UPDATE sessions SET activity_updated_ms = ?1 WHERE session_id = 'stale'",
            [now - ACTIVE_WINDOW_SECONDS * 1_000 - 1],
        )
        .unwrap();
        assert!(
            live_usage(&conn, "codex", None).unwrap().active_sessions.is_empty(),
            "失败展示超过 90 秒窗口后消失"
        );
    }

    #[test]
    fn waiting_session_preserves_tokens_and_start_across_long_silence() {
        let mut conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        let started = Local::now().timestamp_millis() - 86_400_000;
        set_session_activity(&conn, "codex", "waiting", "running", started).unwrap();
        conn.execute("UPDATE sessions SET turn_started_ms = ?1 WHERE session_id = 'waiting'", [started]).unwrap();
        insert_records(&mut conn, &[RequestRecord {
            platform: "codex".into(), source: "local".into(), dedup_key: "before-wait".into(),
            session_id: Some("waiting".into()), ts: started + 1000, model: None,
            input_tokens: Some(100), output_tokens: Some(20), cache_read_tokens: Some(0),
            cache_write_tokens: Some(0), total_tokens: Some(120), effort: None,
        }]).unwrap();
        let initial = live_usage(&conn, "codex", None).unwrap();
        assert_eq!(initial.running_tokens, Some(120));
        assert_eq!(initial.active_sessions[0].started_at_ms, Some(started));
        let heartbeat = live_usage(&conn, "codex", Some(initial.cursor)).unwrap();
        assert_eq!(heartbeat.delta_tokens, 0);
        assert_eq!(heartbeat.running_tokens, Some(120));
        set_session_activity(&conn, "codex", "waiting", "done", Local::now().timestamp_millis()).unwrap();
        let ended = live_usage(&conn, "codex", Some(initial.cursor)).unwrap();
        assert!(ended.active_sessions.is_empty());
        assert_eq!(ended.running_tokens, None);
    }

    #[test]
    fn missing_token_fields_remain_unknown_while_real_zero_is_preserved() {
        let mut conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        let now = Local::now().timestamp_millis();
        let record = |key: &str, input: Option<i64>, total: Option<i64>| RequestRecord {
            platform: "claude".into(),
            source: "local".into(),
            dedup_key: key.into(),
            session_id: None,
            ts: now,
            model: Some("claude-sonnet-4".into()),
            input_tokens: input,
            output_tokens: Some(0),
            cache_read_tokens: Some(0),
            cache_write_tokens: Some(0),
            total_tokens: total, effort: None,
        };
        insert_records(&mut conn, &[
            record("missing", None, None),
            record("zero", Some(0), Some(0)),
        ]).unwrap();

        let page = request_log_at(&conn, "claude", "total", 1, None, None, now + 1).unwrap();
        let missing = page.rows.iter().find(|row| row.input.is_none()).unwrap();
        let zero = page.rows.iter().find(|row| row.input == Some(0)).unwrap();
        assert_eq!((missing.input, missing.total, missing.cost_estimate), (None, None, None));
        assert_eq!((zero.input, zero.total), (Some(0), Some(0)));
        assert_eq!(token_totals_at(&conn, "claude", None, None, now + 1).unwrap().total, None);
        assert!(!cost_estimate_at(&conn, "claude", "total", None, None, now + 1).unwrap().complete);
    }

    #[test]
    fn history_cleanup_deletes_old_data_but_keeps_scan_offsets() {
        let mut conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        let record = |key: &str, ts: i64| RequestRecord {
            platform: "codex".into(), source: "local".into(), dedup_key: key.into(),
            session_id: Some(key.into()), ts, model: None, input_tokens: Some(1),
            output_tokens: Some(1), cache_read_tokens: Some(0), cache_write_tokens: Some(0),
            total_tokens: Some(2), effort: None,
        };
        insert_records(&mut conn, &[record("old", 100), record("new", 200)]).unwrap();
        set_offset(&conn, "history.jsonl", 900, 12).unwrap();
        set_session_activity(&conn, "codex", "old", "done", 100).unwrap();

        let preview = cleanup_preview(&conn, 150).unwrap();
        assert_eq!((preview.requests, preview.usage_events, preview.sessions), (1, 1, 1));
        let deleted = cleanup_history(&mut conn, 150).unwrap();
        assert_eq!(deleted.requests, 1);
        assert_eq!(get_offset(&conn, "history.jsonl"), 900);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM requests", [], |row| row.get::<_, i64>(0)).unwrap(), 1);
    }

    #[test]
    fn backup_round_trip_is_deduplicated_and_excludes_credentials() {
        let mut source = Connection::open_in_memory().unwrap();
        init(&source).unwrap();
        insert_records(&mut source, &[RequestRecord {
            platform: "claude".into(), source: "local".into(), dedup_key: "request-1".into(),
            session_id: Some("session-1".into()), ts: 1_000, model: Some("claude-sonnet-4".into()),
            input_tokens: Some(10), output_tokens: None, cache_read_tokens: Some(2),
            cache_write_tokens: None, total_tokens: Some(12), effort: None,
        }]).unwrap();
        upsert_session_title(&source, "claude", "session-1", "会话标题").unwrap();
        source.execute(
            "INSERT INTO connections(id,platform,kind,name,secret,masked,created_ms) VALUES('c','claude','api','私有连接','secret-value','****',1)",
            [],
        ).unwrap();

        let json = export_backup(&source, Some("claude")).unwrap();
        assert!(!json.contains("secret-value"));
        assert!(!json.contains("私有连接"));

        let mut target = Connection::open_in_memory().unwrap();
        init(&target).unwrap();
        let first = import_backup(&mut target, &json).unwrap();
        let second = import_backup(&mut target, &json).unwrap();
        assert_eq!((first.requests_changed, first.sessions_changed), (1, 1));
        assert_eq!((second.requests_changed, second.sessions_changed), (0, 0));
        let row = request_log_at(&target, "claude", "total", 1, None, None, 2_000).unwrap().rows.remove(0);
        assert_eq!((row.input, row.output, row.total), (Some(10), None, Some(12)));
    }
}

#[cfg(test)]
#[path = "db_breakdown_tests.rs"]
mod db_breakdown_tests;
