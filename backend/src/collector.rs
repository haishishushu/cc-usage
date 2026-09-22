//! 本地会话记录采集
//!
//! 采集的是 CLI 的**会话日志**（用于统计 Token），不是凭证——
//! 凭证读取是「获取」按钮那条独立路径（§6.3），两者读的文件与用途不同。
//!
//! 约束（§7.7）：增量读取与去重，避免定时全量扫描、反复解析全部历史。

use crate::db::{self, RequestRecord};
use chrono::DateTime;
use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const SOURCE_LOCAL: &str = "local-session";

/// Claude Code 的 ai-title；Codex 标题从独立的 session_index.jsonl 同步。
struct TitleHit {
    platform: &'static str,
    session_id: String,
    title: String,
}

fn parse_title(line: &str, kind: &str) -> Option<TitleHit> {
    let v: Value = serde_json::from_str(line).ok()?;
    match kind {
        "claude" => {
            if v.get("type")?.as_str()? != "ai-title" {
                return None;
            }
            Some(TitleHit {
                platform: "claude",
                session_id: v.get("sessionId")?.as_str()?.to_string(),
                title: v.get("aiTitle")?.as_str()?.trim().to_string(),
            })
        }
        _ => None,
    }
}

#[derive(Debug, Serialize, Default)]
pub struct ScanResult {
    pub files_scanned: usize,
    pub records_inserted: usize,
    pub claude_found: bool,
    pub codex_found: bool,
    pub errors: Vec<String>,
    /// 出错文件所属的平台（claude / codex）。采集异常按平台定界：
    /// 一个文件的扫描失败只说明该平台的会话状态暂不可判定，
    /// 不应把其他平台的会话也打成「状态未知」。
    pub failed_sources: Vec<String>,
}

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn iso_to_millis(s: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp_millis())
}

fn n(v: &Value, key: &str) -> Option<i64> {
    v.get(key).and_then(Value::as_i64)
}

/// 递归收集某目录下的 .jsonl
fn collect_jsonl(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_jsonl(&p, out);
        } else if p.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            out.push(p);
        }
    }
}

/// 从上次偏移继续读；文件被截断或重写（当前大小 < 已读偏移）则整文件重扫
#[cfg(test)]
fn read_new_lines(conn: &Connection, path: &Path) -> std::io::Result<(Vec<String>, i64, i64)> {
    let key = path.to_string_lossy().to_string();
    let meta = std::fs::metadata(path)?;
    let size = meta.len() as i64;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let previous = db::get_scan_state(conn, &key);
    let mut offset = previous.map_or(0, |state| state.0);
    if offset > size {
        offset = 0; // 文件被重写，重新来过
    }
    if offset == size && previous.is_some_and(|(_, old_mtime)| old_mtime != mtime) {
        offset = 0; // 大小相同但修改时间变化：文件被等长替换
    }
    if offset == size {
        return Ok((vec![], offset, mtime));
    }

    let mut f = std::fs::File::open(path)?;
    f.seek(SeekFrom::Start(offset as u64))?;
    let mut reader = BufReader::new(f);
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    // 只消费到最后一个真实 LF；没有换行的尾部可能仍在写入，留给下次扫描。
    let complete_len = bytes.iter().rposition(|byte| *byte == b'\n').map_or(0, |i| i + 1);
    let mut lines = Vec::new();
    for raw in bytes[..complete_len].split(|byte| *byte == b'\n') {
        if raw.is_empty() { continue; }
        let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
        let line = std::str::from_utf8(raw)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        if !line.trim().is_empty() { lines.push(line.to_string()); }
    }
    Ok((lines, offset + complete_len as i64, mtime))
}

/// Claude Code：`~/.claude/projects/<project>/<session>.jsonl`
///
/// assistant 行结构（实测）：
///   timestamp, sessionId, message.id, message.model, message.usage{
///     input_tokens, output_tokens, cache_creation_input_tokens, cache_read_input_tokens }
///
/// 该来源**不提供** costUSD —— 对应界面上成本标「估算」。请求耗时 / 首字 / HTTP 码
/// 会话记录里没有：本地代理（proxy.rs）开启时由转发计时按去重键回填，未开启时为
/// NULL，界面显示「—」，不伪造。
fn parse_claude_line(line: &str) -> Option<RequestRecord> {
    let v: Value = serde_json::from_str(line).ok()?;
    if v.get("type")?.as_str()? != "assistant" {
        return None;
    }
    let msg = v.get("message")?;
    let usage = msg.get("usage")?;
    let dedup_key = msg.get("id")?.as_str()?.to_string();
    let ts = iso_to_millis(v.get("timestamp")?.as_str()?)?;

    let input = n(usage, "input_tokens");
    let output = n(usage, "output_tokens");
    let cache_write = n(usage, "cache_creation_input_tokens");
    let cache_read = n(usage, "cache_read_input_tokens");

    // Claude 的 input_tokens 不含缓存部分（缓存单列），因此总值需要相加，
    // 这不是「把缓存再次累加」。来源未提供显式 total，故由此口径计算。
    let total = input
        .zip(output)
        .zip(cache_write)
        .zip(cache_read)
        .map(|(((input, output), cache_write), cache_read)| input + output + cache_write + cache_read);

    Some(RequestRecord {
        platform: "claude".into(),
        source: SOURCE_LOCAL.into(),
        dedup_key,
        session_id: v.get("sessionId").and_then(Value::as_str).map(String::from),
        ts,
        model: msg.get("model").and_then(Value::as_str).map(String::from),
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: cache_read,
        cache_write_tokens: cache_write,
        total_tokens: total,
        // 思考强度就在同一条 assistant 行上，与 usage 同源，无需跨行继承
        effort: v.get("effort").and_then(Value::as_str).map(String::from),
    })
}

/// Codex：`~/.codex/sessions/<y>/<m>/<d>/rollout-*.jsonl`
///
/// `token_usage_record` 行的 payload：
///   - `usage`              每次响应的**增量**（我们要的）
///   - `turn_token_usage` / `thread_token_usage`  **累计值**，不能直接求和
///   - `response_id`        去重键
///
/// model 不在该行上，需沿用文件中最近一条 `turn_context` 的 payload.model。
/// 按路径逐层取字符串；任一层缺失或类型不符都返回 None。
fn nested_str<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in path {
        current = current.get(key)?;
    }
    current.as_str()
}

fn parse_codex_line(
    line: &str,
    current_model: &mut Option<String>,
    current_effort: &mut Option<String>,
) -> Option<RequestRecord> {
    let v: Value = serde_json::from_str(line).ok()?;
    let ty = v.get("type").and_then(Value::as_str)?;

    if ty == "turn_context" {
        let payload = v.get("payload");
        if let Some(m) = payload.and_then(|p| p.get("model")).and_then(Value::as_str) {
            *current_model = Some(m.to_string());
        }
        // 思考强度同样只在 turn_context 上，按会话继承到后续的用量行。
        // 实测存在三种嵌套位置，按由内到外的顺序回退；都没有就保持上一轮的值。
        if let Some(p) = payload {
            if let Some(e) = nested_str(p, &["thread_settings", "reasoning_effort"])
                .or_else(|| nested_str(p, &["thread_settings", "collaboration_mode", "settings", "reasoning_effort"]))
                .or_else(|| nested_str(p, &["collaboration_mode", "settings", "reasoning_effort"]))
            {
                *current_effort = Some(e.to_string());
            }
        }
        return None;
    }
    if ty != "token_usage_record" {
        return None;
    }

    let payload = v.get("payload")?;
    let dedup_key = payload.get("response_id")?.as_str()?.to_string();
    let ts = iso_to_millis(v.get("timestamp")?.as_str()?)?;
    // 取增量而非累计
    let usage = payload.get("usage")?;

    let input = n(usage, "input_tokens");
    let output = n(usage, "output_tokens");
    let cached = n(usage, "cached_input_tokens");
    let cache_write = n(usage, "cache_write_input_tokens");

    // Codex 提供语义明确的 total_tokens，且其 input_tokens 已含 cached 部分，
    // 因此直接采用来源总值，不再把缓存累加一次（§7.5）。
    let total = usage
        .get("total_tokens")
        .and_then(Value::as_i64)
        .or_else(|| input.zip(output).map(|(input, output)| input + output));

    Some(RequestRecord {
        platform: "codex".into(),
        source: SOURCE_LOCAL.into(),
        dedup_key,
        session_id: payload.get("session_id").and_then(Value::as_str).map(String::from),
        ts,
        model: current_model.clone(),
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: cached,
        cache_write_tokens: cache_write,
        total_tokens: total,
        effort: current_effort.clone(),
    })
}

#[derive(Default)]
struct SessionFileState {
    model: Option<String>,
    /// Codex 的思考强度只在 turn_context 上出现，随文件顺序继承到后续用量行
    effort: Option<String>,
    session_id: Option<String>,
    activity_state: Option<String>,
    updated_at_ms: Option<i64>,
    turn_started_ms: Option<i64>,
}

#[cfg(test)]
fn parse_codex_file_state<I, S>(lines: I) -> SessionFileState
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut state = SessionFileState::default();
    for line in lines {
        apply_codex_file_state(&mut state, line.as_ref());
    }
    state
}

fn apply_codex_file_state(state: &mut SessionFileState, line: &str) {
    let Ok(value) = serde_json::from_str::<Value>(line) else { return };
    let Some(kind) = value.get("type").and_then(Value::as_str) else { return };
    let payload = value.get("payload");

    if let Some(ts) = value.get("timestamp").and_then(Value::as_str).and_then(iso_to_millis) {
        state.updated_at_ms = Some(state.updated_at_ms.map_or(ts, |old| old.max(ts)));
    }
    if kind == "session_meta" {
        if let Some(session_id) = payload.and_then(|p| p.get("id")).and_then(Value::as_str) {
            state.session_id = Some(session_id.to_string());
        }
    }
    if kind == "turn_context" {
        if let Some(model) = payload.and_then(|p| p.get("model")).and_then(Value::as_str) {
            state.model = Some(model.to_string());
        }
    }
    if kind == "token_usage_record" && state.session_id.is_none() {
        state.session_id = payload
            .and_then(|p| p.get("session_id"))
            .and_then(Value::as_str)
            .map(String::from);
    }

    let event = payload.and_then(|p| p.get("type")).and_then(Value::as_str);
    match event {
        Some("task_started") => {
            state.activity_state = Some("running".into());
            state.turn_started_ms = value.get("timestamp").and_then(Value::as_str).and_then(iso_to_millis);
        },
        Some("task_complete") => state.activity_state = Some("done".into()),
        Some("turn_aborted") => state.activity_state = Some("failed".into()),
        Some("agent_message")
            if payload.and_then(|p| p.get("phase")).and_then(Value::as_str)
                == Some("final_answer") => state.activity_state = Some("done".into()),
        _ if kind == "response_item"
            && event == Some("message")
            && payload.and_then(|p| p.get("phase")).and_then(Value::as_str)
                == Some("final_answer") => state.activity_state = Some("done".into()),
        _ => {}
    }
}

fn apply_claude_file_state(state: &mut SessionFileState, line: &str) {
    let Ok(v) = serde_json::from_str::<Value>(line) else { return };
    if v.get("isSidechain").and_then(Value::as_bool) == Some(true) { return; }
    let Some(id) = v.get("sessionId").and_then(Value::as_str) else { return };
    let Some(ts) = v.get("timestamp").and_then(Value::as_str).and_then(iso_to_millis) else { return };
    if state.updated_at_ms.is_some_and(|old| ts < old) { return; }
    state.session_id = Some(id.into());
    state.updated_at_ms = Some(ts);
    let kind = v.get("type").and_then(Value::as_str);
    let content = v.get("message").and_then(|m| m.get("content"));
    let interrupted = |s: &str| matches!(s.trim(), "[Request interrupted by user]" | "[Request interrupted by user for tool use]");
    let interrupted = content.and_then(Value::as_str).is_some_and(interrupted)
        || content.and_then(Value::as_array).is_some_and(|parts| parts.iter().any(|p|
            p.get("type").and_then(Value::as_str) == Some("text")
                && p.get("text").and_then(Value::as_str).is_some_and(interrupted)));
    let tool_result = v.get("toolUseResult").is_some() || v.get("sourceToolAssistantUUID").is_some()
        || content.and_then(Value::as_array).is_some_and(|parts| parts.iter().any(|p| p.get("type").and_then(Value::as_str) == Some("tool_result")));
    match kind {
        Some("user") if !tool_result && v.get("isMeta").and_then(Value::as_bool) != Some(true) => {
            if interrupted {
                state.activity_state = Some("failed".into());
            } else if state.activity_state.as_deref() != Some("running") {
                state.activity_state = Some("running".into());
                state.turn_started_ms = Some(ts);
            }
        }
        Some("assistant") if v.get("message").and_then(|m| m.get("stop_reason")).and_then(Value::as_str) == Some("end_turn") => {
            state.activity_state = Some("done".into());
        }
        // 重试耗尽或不可重试时 CLI 写出的合成错误记录（isApiErrorMessage）是本轮
        // 最终失败的唯一终止信号；期间 source=request_retry 的 system 记录只是自动
        // 重试，不算结束。缺了这一臂，报错的会话会永远停留在 running 一直计时。
        Some("assistant") if v.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true) => {
            state.activity_state = Some("failed".into());
        }
        Some("system") if v.get("subtype").and_then(Value::as_str) == Some("turn_duration") => {
            state.activity_state = Some("done".into());
        }
        _ => {}
    }
}

fn scan_files<I>(conn: &mut Connection, files: I) -> ScanResult
where
    I: IntoIterator<Item = (PathBuf, &'static str)>,
{
    let mut result = ScanResult::default();
    for (path, kind) in files {
        result.files_scanned += 1;
        match scan_file(conn, &path, kind) {
            Ok(inserted) => result.records_inserted += inserted,
            Err(error) => {
                result.errors.push(format!(
                    "{}: {error}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
                result.failed_sources.push(kind.to_string());
            }
        }
    }

    result
}

const RECORD_BATCH_SIZE: usize = 256;

/// 单个文件以固定大小批次解析并在同一事务内提交记录、解析上下文和字节偏移。
/// 失败时整个文件回滚；重试仍从旧偏移开始，数据库去重继续兜底。
fn scan_file(conn: &mut Connection, path: &Path, kind: &str) -> Result<usize, String> {
    let key = path.to_string_lossy().to_string();
    let meta = std::fs::metadata(path).map_err(|error| error.to_string())?;
    let size = meta.len() as i64;
    let mtime = meta.modified().ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0);
    let saved = db::get_scan_checkpoint(conn, &key).unwrap_or_default();
    let rewritten = saved.offset > size
        || (saved.offset == size && saved.offset > 0 && saved.mtime != mtime);
    // 旧版本未保存对应平台的解析上下文。升级后首次扫描从头安全重建一次，
    // 随后都只处理新增字节；请求去重保证这次迁移重扫不会重复计数。
    let rebuild_lifecycle = matches!(kind, "codex" | "claude") && saved.offset > 0 && !saved.parser_initialized;
    let offset = if rewritten || rebuild_lifecycle { 0 } else { saved.offset };
    if offset == size {
        return Ok(0);
    }

    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    file.seek(SeekFrom::Start(offset as u64)).map_err(|error| error.to_string())?;
    let mut reader = BufReader::new(file.take((size - offset) as u64));
    let mut parser = if matches!(kind, "codex" | "claude") && offset > 0 {
        SessionFileState {
            model: saved.parser_model,
            // 与 model 同样随位点恢复：思考强度只在 turn_context 上出现，
            // 续扫时若从空开始，到下一条 turn_context 之间的记录会平白缺值。
            effort: saved.parser_effort,
            session_id: saved.parser_session_id,
            activity_state: saved.parser_activity_state,
            updated_at_ms: saved.parser_updated_at_ms,
            turn_started_ms: None,
        }
    } else {
        SessionFileState::default()
    };
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let mut batch = Vec::with_capacity(RECORD_BATCH_SIZE);
    let mut inserted = 0usize;
    let mut consumed = 0i64;
    let mut bytes = Vec::new();

    loop {
        bytes.clear();
        let read = reader.read_until(b'\n', &mut bytes).map_err(|error| error.to_string())?;
        if read == 0 || !bytes.ends_with(b"\n") {
            break;
        }
        consumed += read as i64;
        let raw = bytes.strip_suffix(b"\n").unwrap_or(&bytes);
        let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
        // 日志文件可能在 CLI 重写/追加的瞬间被观察到非 UTF-8 的中间行。
        // 该行无法构成 JSON，跳过它并继续消费后续完整行，避免一条坏行阻断整份日志。
        let line = match std::str::from_utf8(raw) {
            Ok(line) => line,
            Err(_) => continue,
        };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(title) = parse_title(line, kind) {
            if !title.title.is_empty() {
                db::upsert_session_title(&tx, title.platform, &title.session_id, &title.title)
                    .map_err(|error| error.to_string())?;
            }
        }
        if kind == "codex" {
            apply_codex_file_state(&mut parser, line);
        } else if kind == "claude" {
            apply_claude_file_state(&mut parser, line);
        }
        let record = match kind {
            "claude" => parse_claude_line(line),
            _ => parse_codex_line(line, &mut parser.model, &mut parser.effort),
        };
        if let Some(record) = record {
            batch.push(record);
            if batch.len() >= RECORD_BATCH_SIZE {
                inserted += db::insert_scanned_records(&tx, &batch)
                    .map_err(|error| error.to_string())?;
                batch.clear();
            }
        }
    }
    if !batch.is_empty() {
        inserted += db::insert_scanned_records(&tx, &batch).map_err(|error| error.to_string())?;
    }
    if matches!(kind, "codex" | "claude") {
        if let (Some(session_id), Some(state), Some(updated_at_ms)) = (
            parser.session_id.as_deref(),
            parser.activity_state.as_deref(),
            parser.updated_at_ms,
        ) {
            db::set_session_activity(&tx, kind, session_id, state, updated_at_ms)
                .map_err(|error| error.to_string())?;
            if let Some(started_at) = parser.turn_started_ms {
                tx.execute(
                    "UPDATE sessions SET turn_started_ms = ?1 WHERE platform = ?3 AND session_id = ?2",
                    rusqlite::params![started_at, session_id, kind],
                ).map_err(|error| error.to_string())?;
            }
        }
    }
    db::set_scan_checkpoint(&tx, &key, &db::ScanCheckpoint {
        offset: offset + consumed,
        mtime,
        parser_model: parser.model,
        parser_session_id: parser.session_id,
        parser_activity_state: parser.activity_state,
        parser_updated_at_ms: parser.updated_at_ms,
        parser_initialized: matches!(kind, "codex" | "claude"),
        parser_effort: parser.effort,
    }).map_err(|error| error.to_string())?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(inserted)
}

pub fn scan(conn: &mut Connection) -> ScanResult {
    let mut result = ScanResult::default();
    let Some(home) = home() else {
        result.errors.push("无法定位用户主目录".into());
        return result;
    };

    let claude_dir = home.join(".claude").join("projects");
    let codex_home = crate::session_titles::codex_home().unwrap_or_else(|| home.join(".codex"));
    if let Err(e) = crate::session_titles::sync_codex(conn, &codex_home) {
        result.errors.push(e);
        result.failed_sources.push("codex".into());
    }
    let codex_dir = codex_home.join("sessions");
    result.claude_found = claude_dir.is_dir();
    result.codex_found = codex_dir.is_dir();

    let mut files: Vec<(PathBuf, &'static str)> = Vec::new();
    if result.claude_found {
        let mut paths = Vec::new();
        collect_jsonl(&claude_dir, &mut paths);
        files.extend(paths.into_iter().map(|path| (path, "claude")));
    }
    if result.codex_found {
        let mut paths = Vec::new();
        collect_jsonl(&codex_dir, &mut paths);
        files.extend(paths.into_iter().map(|path| (path, "codex")));
    }

    let scanned = scan_files(conn, files);
    result.files_scanned = scanned.files_scanned;
    result.records_inserted = scanned.records_inserted;
    result.errors.extend(scanned.errors);
    result.failed_sources.extend(scanned.failed_sources);
    let native=crate::native_sources::scan(conn);
    result.files_scanned+=native.files_scanned;
    result.records_inserted+=native.records_inserted;
    result.errors.extend(native.errors);
    result.failed_sources.extend(native.failed_sources);
    result
}

/// notify 已经给出具体变化路径时，只处理这些文件；路径必须位于已知 CLI
/// 会话目录内，避免把任意外部文件送进解析器。周期心跳仍调用 `scan` 全量补漏。
pub fn scan_paths(conn: &mut Connection, changed: impl IntoIterator<Item = PathBuf>) -> ScanResult {
    let mut result = ScanResult::default();
    let Some(home) = home() else {
        result.errors.push("无法定位用户主目录".into());
        return result;
    };
    let claude_dir = home.join(".claude").join("projects");
    let codex_home = crate::session_titles::codex_home().unwrap_or_else(|| home.join(".codex"));
    let codex_dir = codex_home.join("sessions");
    let index = codex_home.join("session_index.jsonl");
    result.claude_found = claude_dir.is_dir();
    result.codex_found = codex_dir.is_dir();

    let mut unique = HashSet::new();
    let mut files = Vec::new();
    let mut sync_titles = false;
    for path in changed {
        if !unique.insert(path.clone()) {
            continue;
        }
        if path == index {
            sync_titles = true;
        } else if path.extension().and_then(|value| value.to_str()) == Some("jsonl")
            && path.starts_with(&claude_dir)
            && path.is_file()
        {
            files.push((path, "claude"));
        } else if path.extension().and_then(|value| value.to_str()) == Some("jsonl")
            && path.starts_with(&codex_dir)
            && path.is_file()
        {
            files.push((path, "codex"));
        }
    }
    if sync_titles {
        if let Err(error) = crate::session_titles::sync_codex(conn, &codex_home) {
            result.errors.push(error);
            // 标题索引属于 Codex 来源，失败同样只定界到该平台
            result.failed_sources.push("codex".into());
        }
    }
    let scanned = scan_files(conn, files);
    result.files_scanned = scanned.files_scanned;
    result.records_inserted = scanned.records_inserted;
    result.errors.extend(scanned.errors);
    result.failed_sources.extend(scanned.failed_sources);
    let native=crate::native_sources::scan(conn);
    result.files_scanned+=native.files_scanned;
    result.records_inserted+=native.records_inserted;
    result.errors.extend(native.errors);
    result.failed_sources.extend(native.failed_sources);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_jsonl(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "cc-usage-{name}-{}-{}.jsonl",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ))
    }

    #[test]
    fn incremental_reader_keeps_incomplete_tail_and_counts_crlf_bytes_exactly() {
        let path = temp_jsonl("partial-lines");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(b"first\r\nsecond\npartial").unwrap();
        drop(file);
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE scan_state(path TEXT PRIMARY KEY, offset INTEGER NOT NULL, mtime INTEGER NOT NULL);",
        ).unwrap();

        let (lines, offset, _) = read_new_lines(&conn, &path).unwrap();
        assert_eq!(lines, vec!["first", "second"]);
        assert_eq!(offset, b"first\r\nsecond\n".len() as i64);

        db::set_offset(&conn, &path.to_string_lossy(), offset, 0).unwrap();
        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"\n").unwrap();
        drop(file);
        let (lines, next_offset, _) = read_new_lines(&conn, &path).unwrap();
        assert_eq!(lines, vec!["partial"]);
        assert_eq!(next_offset, std::fs::metadata(&path).unwrap().len() as i64);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn equal_length_rewrite_is_rescanned_when_mtime_changes() {
        let path = temp_jsonl("equal-rewrite");
        std::fs::write(&path, b"first\n").unwrap();
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE scan_state(path TEXT PRIMARY KEY, offset INTEGER NOT NULL, mtime INTEGER NOT NULL);",
        ).unwrap();
        let (_, offset, mtime) = read_new_lines(&conn, &path).unwrap();
        db::set_offset(&conn, &path.to_string_lossy(), offset, mtime - 1).unwrap();
        std::fs::write(&path, b"other\n").unwrap();

        let (lines, next_offset, _) = read_new_lines(&conn, &path).unwrap();
        assert_eq!(lines, vec!["other"]);
        assert_eq!(next_offset, offset);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn codex_task_started_marks_session_running_and_tracks_latest_write() {
        let state = parse_codex_file_state([
            r#"{"timestamp":"2026-09-16T00:00:00Z","type":"session_meta","payload":{"id":"session-1"}}"#,
            r#"{"timestamp":"2026-09-16T00:00:01Z","type":"event_msg","payload":{"type":"task_started"}}"#,
            r#"{"timestamp":"2026-09-16T00:00:02Z","type":"token_usage_record","payload":{}}"#,
        ]);

        assert_eq!(state.session_id.as_deref(), Some("session-1"));
        assert_eq!(state.activity_state.as_deref(), Some("running"));
        assert_eq!(state.updated_at_ms, iso_to_millis("2026-09-16T00:00:02Z"));
        assert_eq!(state.turn_started_ms, iso_to_millis("2026-09-16T00:00:01Z"));
    }

    #[test]
    fn turn_start_survives_incremental_scan_and_resets_on_next_turn() {
        use std::io::Write;
        let path = temp_jsonl("turn-start");
        let mut conn = db::open(Path::new(":memory:")).unwrap();
        std::fs::write(&path, concat!(
            "{\"timestamp\":\"2026-09-16T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"elapsed\"}}\n",
            "{\"timestamp\":\"2026-09-16T00:00:01Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\"}}\n",
        )).unwrap();
        scan_file(&mut conn, &path, "codex").unwrap();
        let start = |conn: &Connection| conn.query_row(
            "SELECT turn_started_ms FROM sessions WHERE session_id = 'elapsed'",
            [], |row| row.get::<_, i64>(0),
        ).unwrap();
        assert_eq!(start(&conn), iso_to_millis("2026-09-16T00:00:01Z").unwrap());
        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "{{\"timestamp\":\"2026-09-16T00:01:00Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_complete\"}}}}").unwrap();
        scan_file(&mut conn, &path, "codex").unwrap();
        assert_eq!(start(&conn), iso_to_millis("2026-09-16T00:00:01Z").unwrap());
        writeln!(file, "{{\"timestamp\":\"2026-09-16T00:02:00Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_started\"}}}}").unwrap();
        scan_file(&mut conn, &path, "codex").unwrap();
        assert_eq!(start(&conn), iso_to_millis("2026-09-16T00:02:00Z").unwrap());
        drop(file);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn codex_new_turn_resets_elapsed_start() {
        let state = parse_codex_file_state([
            r#"{"timestamp":"2026-09-16T00:00:01Z","type":"event_msg","payload":{"type":"task_started"}}"#,
            r#"{"timestamp":"2026-09-16T00:00:02Z","type":"event_msg","payload":{"type":"task_complete"}}"#,
            r#"{"timestamp":"2026-09-16T00:01:00Z","type":"event_msg","payload":{"type":"task_started"}}"#,
        ]);
        assert_eq!(state.turn_started_ms, iso_to_millis("2026-09-16T00:01:00Z"));
    }

    #[test]
    fn claude_terminal_and_metadata_events_keep_lifecycle_consistent() {
        let start = r#"{"type":"user","sessionId":"s","timestamp":"2026-09-16T00:00:00Z","message":{"content":"prompt"}}"#;
        for end in [
            r#"{"type":"user","message":{"content":[{"type":"text","text":"[Request interrupted by user]"}]}}"#,
            r#"{"type":"system","subtype":"turn_duration"}"#,
            r#"{"type":"assistant","message":{"stop_reason":"end_turn"}}"#,
        ] {
            let mut state = SessionFileState::default();
            apply_claude_file_state(&mut state, start);
            let mut event: Value = serde_json::from_str(end).unwrap();
            event["sessionId"] = Value::String("s".into());
            event["timestamp"] = Value::String("2026-09-16T00:00:01Z".into());
            apply_claude_file_state(&mut state, &event.to_string());
            assert_ne!(state.activity_state.as_deref(), Some("running"));
            for extra in [
                r#"{"type":"user","isMeta":true,"message":{"content":"metadata"}}"#,
                r#"{"type":"user","isSidechain":true,"message":{"content":"side task"}}"#,
                r#"{"type":"user","message":{"content":[{"type":"tool_result"}]}}"#,
            ] {
                let mut event: Value = serde_json::from_str(extra).unwrap();
                event["sessionId"] = Value::String("s".into());
                event["timestamp"] = Value::String("2026-09-16T00:00:02Z".into());
                apply_claude_file_state(&mut state, &event.to_string());
                assert_ne!(state.activity_state.as_deref(), Some("running"));
            }
        }
    }

    #[test]
    fn claude_api_error_message_ends_the_turn_but_retries_do_not() {
        let start = r#"{"type":"user","sessionId":"s","timestamp":"2026-09-16T00:00:00Z","message":{"content":"prompt"}}"#;
        // 重试中的 api_error 记录：CLI 仍在自动重试，本轮没有结束
        for retry in [
            r#"{"type":"system","subtype":"api_error","level":"error","source":"request_retry","retryAttempt":1,"maxRetries":10}"#,
            r#"{"type":"system","subtype":"api_error","level":"error","source":"connection_retry","retryAttempt":2,"maxRetries":10}"#,
        ] {
            let mut state = SessionFileState::default();
            apply_claude_file_state(&mut state, start);
            let mut event: Value = serde_json::from_str(retry).unwrap();
            event["sessionId"] = Value::String("s".into());
            event["timestamp"] = Value::String("2026-09-16T00:00:01Z".into());
            apply_claude_file_state(&mut state, &event.to_string());
            assert_eq!(state.activity_state.as_deref(), Some("running"), "重试尚未放弃，本轮必须仍是运行中：{retry}");
        }
        // 最终失败：isApiErrorMessage 合成 assistant 记录（真实样本字段）→ failed
        let mut state = SessionFileState::default();
        apply_claude_file_state(&mut state, start);
        let mut error: Value = serde_json::from_str(
            r#"{"type":"assistant","message":{"model":"<synthetic>","role":"assistant","stop_reason":"stop_sequence","usage":{"input_tokens":0,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"API Error: 403 rejected"}]},"error":"authentication_failed","isApiErrorMessage":true,"apiErrorStatus":403}"#,
        ).unwrap();
        error["sessionId"] = Value::String("s".into());
        error["timestamp"] = Value::String("2026-09-16T00:01:00Z".into());
        apply_claude_file_state(&mut state, &error.to_string());
        assert_eq!(state.activity_state.as_deref(), Some("failed"), "最终失败的请求必须结束本轮");
        // 失败后用户再次发消息：开启新一轮 running，轮次起点刷新
        apply_claude_file_state(&mut state, r#"{"type":"user","sessionId":"s","timestamp":"2026-09-16T00:02:00Z","message":{"content":"again"}}"#);
        assert_eq!(state.activity_state.as_deref(), Some("running"));
        assert_eq!(state.turn_started_ms, iso_to_millis("2026-09-16T00:02:00Z"));
    }

    #[test]
    fn claude_lifecycle_preserves_waiting_tokens_and_ends_without_recent_window() {
        use std::io::Write;
        let path = temp_jsonl("claude-lifecycle");
        let mut conn = db::open(Path::new(":memory:")).unwrap();
        std::fs::write(&path, concat!(
            "{\"type\":\"user\",\"sessionId\":\"claude-turn\",\"timestamp\":\"2026-09-16T00:00:00Z\",\"message\":{\"role\":\"user\",\"content\":\"test prompt\"}}\n",
            "{\"type\":\"assistant\",\"sessionId\":\"claude-turn\",\"timestamp\":\"2026-09-16T00:00:02Z\",\"message\":{\"id\":\"response-test\",\"model\":\"claude-sonnet-4\",\"stop_reason\":\"tool_use\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5,\"cache_creation_input_tokens\":0,\"cache_read_input_tokens\":0}}}\n",
        )).unwrap();
        scan_file(&mut conn, &path, "claude").unwrap();
        let live = db::live_usage(&conn, "claude", None).unwrap();
        assert_eq!(live.active_sessions.len(), 1, "a silent unfinished turn must not expire after 90 seconds");
        assert_eq!(live.active_window_seconds, 0);
        assert_eq!(live.running_tokens, Some(15));
        let key = path.to_string_lossy().to_string();
        let mut old_checkpoint = db::get_scan_checkpoint(&conn, &key).unwrap();
        old_checkpoint.parser_initialized = false;
        db::set_scan_checkpoint(&conn, &key, &old_checkpoint).unwrap();
        assert_eq!(scan_file(&mut conn, &path, "claude").unwrap(), 0);
        assert_eq!(db::live_usage(&conn, "claude", None).unwrap().running_tokens, Some(15));
        let start = live.active_sessions[0].started_at_ms;
        assert_eq!(start, iso_to_millis("2026-09-16T00:00:00Z"));
        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "{{\"type\":\"user\",\"sessionId\":\"claude-turn\",\"timestamp\":\"2026-09-16T00:20:00Z\",\"message\":{{\"content\":[{{\"type\":\"tool_result\"}}]}}}}").unwrap();
        writeln!(file, "{{\"type\":\"system\",\"subtype\":\"compact_boundary\",\"sessionId\":\"claude-turn\",\"timestamp\":\"2026-09-16T00:20:01Z\"}}").unwrap();
        scan_file(&mut conn, &path, "claude").unwrap();
        assert_eq!(db::live_usage(&conn, "claude", None).unwrap().active_sessions[0].started_at_ms, start);
        writeln!(file, "{{\"type\":\"assistant\",\"sessionId\":\"claude-turn\",\"timestamp\":\"2026-09-16T00:20:02Z\",\"message\":{{\"stop_reason\":\"end_turn\"}}}}").unwrap();
        scan_file(&mut conn, &path, "claude").unwrap();
        assert!(db::live_usage(&conn, "claude", None).unwrap().active_sessions.is_empty());
        writeln!(file, "{{\"type\":\"user\",\"sessionId\":\"claude-turn\",\"timestamp\":\"2026-09-16T00:21:00Z\",\"message\":{{\"content\":\"next prompt\"}}}}").unwrap();
        scan_file(&mut conn, &path, "claude").unwrap();
        let next = db::live_usage(&conn, "claude", None).unwrap();
        assert_eq!(next.active_sessions[0].started_at_ms, iso_to_millis("2026-09-16T00:21:00Z"));
        assert_eq!(next.running_tokens, Some(0));
        drop(file); std::fs::remove_file(path).unwrap();
    }

    #[test]
        fn dbg_today_probe() {
            use chrono::Utc;
            let path = temp_jsonl("claude-dbg");
            let mut conn = db::open(Path::new(":memory:")).unwrap();
            let base = Utc::now().timestamp_millis();
            let iso = |offset_ms: i64| {
                chrono::DateTime::from_timestamp((base + offset_ms) / 1000, 0).unwrap()
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
            };
            std::fs::write(&path, format!(
                concat!(
                    "{{\"type\":\"user\",\"sessionId\":\"dbg\",\"timestamp\":\"{t0}\",\"message\":{{\"role\":\"user\",\"content\":\"p\"}}}}\n",
                    "{{\"type\":\"assistant\",\"sessionId\":\"dbg\",\"timestamp\":\"{t1}\",\"message\":{{\"id\":\"r1\",\"model\":\"m\",\"stop_reason\":\"tool_use\",\"usage\":{{\"input_tokens\":10,\"output_tokens\":5,\"cache_creation_input_tokens\":0,\"cache_read_input_tokens\":1000}}}}}}\n"
                ),
                t0 = iso(0), t1 = iso(2_000),
            )).unwrap();
            scan_file(&mut conn, &path, "claude").unwrap();
            let rows: Vec<(i64, i64, i64, i64, String)> = conn.prepare(
                "SELECT input_tokens, output_tokens, total_tokens, total_known, datetime(ts/1000,'unixepoch') FROM requests")
                .unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))).unwrap()
                .collect::<Result<_,_>>().unwrap();
            for row in &rows { println!("ROW {:?}", row); }
            let now_local = chrono::Local::now().timestamp_millis();
            println!("local_midnight_ms = {}", now_local - (now_local % 86_400_000));
            let range = db::period_range(&conn, "claude", "today");
            println!("today range = {} .. {}", range.0, range.1);
            let live = db::live_usage(&conn, "claude", None).unwrap();
            println!("today_tokens = {:?}", live.today_tokens);
            std::fs::remove_file(path).unwrap();
        }

    #[test]
    fn running_tokens_count_fresh_input_output_not_cache_rereads() {
        // 实时口径对齐 Claude Code 终端：缓存重读（cache_read/write）不计入运行中增量。
        // 同一轮里每次请求都重读全部缓存上下文，若计入会把 +34k 显示成 +1.5M。
        use chrono::Utc;
        let path = temp_jsonl("claude-fresh-tokens");
        let mut conn = db::open(Path::new(":memory:")).unwrap();
        // 时间戳取「当前时刻之前」，保证落在今日统计窗口内（窗口右边界是当下）
        let base = Utc::now().timestamp_millis() - 60_000;
        let iso = |offset_ms: i64| {
            chrono::DateTime::from_timestamp((base + offset_ms) / 1000, 0).unwrap()
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        };
        std::fs::write(&path, format!(
            concat!(
                "{{\"type\":\"user\",\"sessionId\":\"fresh-tokens\",\"timestamp\":\"{t0}\",\"message\":{{\"role\":\"user\",\"content\":\"test prompt\"}}}}\n",
                "{{\"type\":\"assistant\",\"sessionId\":\"fresh-tokens\",\"timestamp\":\"{t1}\",\"message\":{{\"id\":\"r1\",\"model\":\"claude-sonnet-4\",\"stop_reason\":\"tool_use\",\"usage\":{{\"input_tokens\":10,\"output_tokens\":5,\"cache_creation_input_tokens\":0,\"cache_read_input_tokens\":1000}}}}}}\n",
                "{{\"type\":\"assistant\",\"sessionId\":\"fresh-tokens\",\"timestamp\":\"{t2}\",\"message\":{{\"id\":\"r2\",\"model\":\"claude-sonnet-4\",\"stop_reason\":\"tool_use\",\"usage\":{{\"input_tokens\":20,\"output_tokens\":8,\"cache_creation_input_tokens\":30,\"cache_read_input_tokens\":2000}}}}}}\n"
            ),
            t0 = iso(0), t1 = iso(2_000), t2 = iso(4_000),
        )).unwrap();
        scan_file(&mut conn, &path, "claude").unwrap();
        let live = db::live_usage(&conn, "claude", None).unwrap();
        // 新鲜 Token = (10+5) + (20+8) = 43；缓存读 3000 与缓存写 30 不计
        assert_eq!(live.running_tokens, Some(43));
        // 灵动岛「本机今日 Token」同口径：新鲜 Token
        assert_eq!(live.today_tokens, Some(43));
        // 主面板统计口径不变：token_totals 仍含缓存（43 + 3000 + 30 = 3073）
        let totals = db::token_totals_at(&conn, "claude", None, None, chrono::Utc::now().timestamp_millis()).unwrap();
        assert_eq!(totals.today, Some(3073));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn codex_thinking_waiting_and_compacting_do_not_end_or_restart_turn() {
        let start = r#"{"timestamp":"2026-09-16T00:00:01Z","type":"event_msg","payload":{"type":"task_started"}}"#;
        for line in [
            r#"{"timestamp":"2026-09-16T00:10:01Z","type":"event_msg","payload":{"type":"agent_reasoning","text":"thinking"}}"#,
            r#"{"timestamp":"2026-09-16T00:10:01Z","type":"response_item","payload":{"type":"function_call","name":"request_user_input"}}"#,
            r#"{"timestamp":"2026-09-16T00:10:01Z","type":"event_msg","payload":{"type":"context_compacted"}}"#,
            r#"{"timestamp":"2026-09-16T00:10:01Z","type":"compacted","payload":{}}"#,
        ] {
            let state = parse_codex_file_state([start, line]);
            assert_eq!(state.activity_state.as_deref(), Some("running"));
            assert_eq!(state.turn_started_ms, iso_to_millis("2026-09-16T00:00:01Z"));
        }
    }

    #[test]
    fn codex_completion_abort_and_final_answer_all_stop_the_session() {
        for end in [
            r#"{"timestamp":"2026-09-16T00:00:02Z","type":"event_msg","payload":{"type":"task_complete"}}"#,
            r#"{"timestamp":"2026-09-16T00:00:02Z","type":"event_msg","payload":{"type":"turn_aborted"}}"#,
            r#"{"timestamp":"2026-09-16T00:00:02Z","type":"event_msg","payload":{"type":"agent_message","phase":"final_answer"}}"#,
        ] {
            let state = parse_codex_file_state([
                r#"{"timestamp":"2026-09-16T00:00:00Z","type":"session_meta","payload":{"id":"session-1"}}"#,
                r#"{"timestamp":"2026-09-16T00:00:01Z","type":"event_msg","payload":{"type":"task_started"}}"#,
                end,
            ]);
            let expected = if end.contains("turn_aborted") { "failed" } else { "done" };
            assert_eq!(state.activity_state.as_deref(), Some(expected), "结束事件状态错误：{end}");
        }
    }

    #[test]
    fn targeted_scan_only_reads_the_supplied_file() {
        let first = temp_jsonl("targeted-first");
        let second = temp_jsonl("targeted-second");
        std::fs::write(
            &first,
            b"{\"timestamp\":\"2026-09-16T00:00:00Z\",\"type\":\"assistant\",\"sessionId\":\"s1\",\"message\":{\"id\":\"m1\",\"usage\":{\"input_tokens\":1,\"output_tokens\":2}}}\n",
        )
        .unwrap();
        std::fs::write(
            &second,
            b"{\"timestamp\":\"2026-09-16T00:00:00Z\",\"type\":\"assistant\",\"sessionId\":\"s2\",\"message\":{\"id\":\"m2\",\"usage\":{\"input_tokens\":10,\"output_tokens\":20}}}\n",
        )
        .unwrap();
        let database = temp_jsonl("targeted-db").with_extension("db");
        let mut conn = crate::db::open(&database).unwrap();

        let result = scan_files(&mut conn, vec![(first.clone(), "claude")]);

        assert_eq!(result.files_scanned, 1);
        assert_eq!(result.records_inserted, 1);
        assert_eq!(crate::db::get_offset(&conn, &second.to_string_lossy()), 0);
        let stored: (i64, i64, i64) = conn
            .query_row("SELECT input_tokens, output_tokens, total_known FROM requests", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .unwrap();
        assert_eq!(stored, (1, 2, 0));
        let _ = std::fs::remove_file(first);
        let _ = std::fs::remove_file(second);
        drop(conn);
        let _ = std::fs::remove_file(database);
    }

    #[test]
    fn scan_skips_invalid_utf8_line_and_keeps_following_records() {
        let path = temp_jsonl("invalid-utf8-line");
        std::fs::write(
            &path,
            b"\xff\n{\"timestamp\":\"2026-09-16T00:00:00Z\",\"type\":\"assistant\",\"sessionId\":\"s1\",\"message\":{\"id\":\"m1\",\"usage\":{\"input_tokens\":1,\"output_tokens\":2}}}\n",
        )
        .unwrap();
        let mut conn = crate::db::open(Path::new(":memory:")).unwrap();

        let result = scan_files(&mut conn, vec![(path.clone(), "claude")]);

        assert!(result.errors.is_empty(), "坏行不应阻断整份日志：{:?}", result.errors);
        assert_eq!(result.records_inserted, 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn scan_failures_report_their_platform_so_other_sources_stay_trustworthy() {
        // 目录不存在 → open 失败 → 采集错误必须带上所属平台，供状态展示按平台定界
        let missing = temp_jsonl("missing-codex-dir");
        let mut conn = crate::db::open(Path::new(":memory:")).unwrap();

        let result = scan_files(&mut conn, vec![(missing.clone(), "codex")]);

        assert_eq!(result.errors.len(), 1, "读不到文件应报错：{:?}", result.errors);
        assert_eq!(result.failed_sources, vec!["codex".to_string()], "错误必须定界到出错平台");
        // 成功的文件不产生任何平台污染
        let ok_path = temp_jsonl("ok-claude");
        std::fs::write(
            &ok_path,
            "{\"timestamp\":\"2026-09-16T00:00:00Z\",\"type\":\"assistant\",\"sessionId\":\"s1\",\"message\":{\"id\":\"m-ok\",\"usage\":{\"input_tokens\":1,\"output_tokens\":2}}}\n",
        )
        .unwrap();
        let clean = scan_files(&mut conn, vec![(ok_path.clone(), "claude")]);
        assert!(clean.errors.is_empty());
        assert!(clean.failed_sources.is_empty(), "成功扫描不得报告失败平台");
        let _ = std::fs::remove_file(missing);
        let _ = std::fs::remove_file(ok_path);
    }

    #[test]
    fn codex_streaming_scan_batches_records_and_reuses_persisted_turn_context() {
        let path = temp_jsonl("codex-streaming");
        let database = temp_jsonl("codex-streaming-db").with_extension("db");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, r#"{{"timestamp":"2026-09-16T00:00:00Z","type":"session_meta","payload":{{"id":"session-stream"}}}}"#).unwrap();
        writeln!(file, r#"{{"timestamp":"2026-09-16T00:00:01Z","type":"turn_context","payload":{{"model":"gpt-5.6","thread_settings":{{"reasoning_effort":"high"}}}}}}"#).unwrap();
        for index in 0..(RECORD_BATCH_SIZE + 10) {
            writeln!(file, "{{\"timestamp\":\"2026-09-16T00:00:02Z\",\"type\":\"token_usage_record\",\"payload\":{{\"response_id\":\"response-{index}\",\"session_id\":\"session-stream\",\"usage\":{{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}}}}").unwrap();
        }
        drop(file);
        let mut conn = crate::db::open(&database).unwrap();

        let first = scan_files(&mut conn, vec![(path.clone(), "codex")]);
        assert_eq!(first.records_inserted, RECORD_BATCH_SIZE + 10);

        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, r#"{{"timestamp":"2026-09-16T00:00:03Z","type":"token_usage_record","payload":{{"response_id":"response-next","session_id":"session-stream","usage":{{"input_tokens":2,"output_tokens":3,"total_tokens":5}}}}}}"#).unwrap();
        drop(file);
        let second = scan_files(&mut conn, vec![(path.clone(), "codex")]);
        assert_eq!(second.records_inserted, 1);
        // model 与 effort 都只在首次扫描读到的 turn_context 上出现。续扫的这条记录
        // 要拿到它们，只能靠位点里持久化的解析上下文——否则会平白缺值。
        let (model, effort): (String, Option<String>) = conn.query_row(
            "SELECT model, effort FROM requests WHERE dedup_key = 'response-next'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(model, "gpt-5.6");
        assert_eq!(effort.as_deref(), Some("high"));

        let _ = std::fs::remove_file(path);
        drop(conn);
        let _ = std::fs::remove_file(database);
    }

    #[test]
    #[ignore = "读取本机 Claude/Codex 日志；仅显式执行且不输出会话内容"]
    fn real_local_logs_read_only_smoke() {
        let database = temp_jsonl("real-log-smoke").with_extension("db");
        let mut conn = crate::db::open(&database).unwrap();
        let result = scan(&mut conn);
        assert!(result.files_scanned > 0, "未发现可验证的本机会话日志");
        assert!(result.errors.is_empty(), "真实日志扫描错误：{:?}", result.errors);
        let records: i64 = conn.query_row("SELECT COUNT(*) FROM requests", [], |row| row.get(0)).unwrap();
        assert!(records > 0, "真实日志没有解析出任何请求记录");
        let claude: i64 = conn.query_row("SELECT COUNT(*) FROM requests WHERE platform = 'claude'", [], |row| row.get(0)).unwrap();
        let codex: i64 = conn.query_row("SELECT COUNT(*) FROM requests WHERE platform = 'codex'", [], |row| row.get(0)).unwrap();
        assert!(claude > 0, "Claude 真实日志没有解析出请求记录");
        assert!(codex > 0, "Codex 真实日志没有解析出请求记录");
        println!("validated_files={} parsed_records={records} claude_records={claude} codex_records={codex}", result.files_scanned);
        drop(conn);
        for path in [database.clone(), database.with_extension("db-wal"), database.with_extension("db-shm")] {
            let _ = std::fs::remove_file(path);
        }
    }

    /// 诊断工具：复制真实应用数据库（含各文件扫描位点与解析上下文）到临时目录，
    /// 用与应用完全相同的增量路径跑一次全量扫描并输出真实错误文本。
    /// 与 [`real_local_logs_read_only_smoke`] 的差异：那个从全新库从头扫，
    /// 复现不了「应用位点 + 增量读取」路径上的问题。只输出错误与计数，不输出会话内容。
    #[test]
    #[ignore = "复制应用真实库诊断采集错误；仅显式执行且不输出会话内容"]
    fn diagnose_app_scan_state_errors() {
        let Ok(app_data) = std::env::var("APPDATA") else {
            panic!("缺少 APPDATA，非 Windows 环境无法诊断");
        };
        let dir = std::path::Path::new(&app_data).join("dev.ningz.cc-usage");
        let workspace = temp_jsonl("app-db-diagnose");
        let database = workspace.with_extension("db");
        for (from, to) in [
            ("usage.db", "diagnosis.db"),
            ("usage.db-wal", "diagnosis.db-wal"),
            ("usage.db-shm", "diagnosis.db-shm"),
        ] {
            let source = dir.join(from);
            if source.exists() {
                std::fs::copy(&source, workspace.with_file_name(to))
                    .unwrap_or_else(|error| panic!("复制 {from} 失败：{error}"));
            }
        }
        let mut conn = crate::db::open(&database).unwrap();
        let result = scan(&mut conn);
        println!(
            "files={} inserted={} errors={}",
            result.files_scanned,
            result.records_inserted,
            result.errors.len()
        );
        for error in &result.errors {
            println!("ERROR: {error}");
        }
        assert!(result.errors.is_empty(), "应用库位点下仍存在采集错误（见上方 ERROR）");
        drop(conn);
        let _ = std::fs::remove_file(&database);
        for suffix in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(workspace.with_file_name(&format!("diagnosis.db{suffix}")));
        }
    }
}
