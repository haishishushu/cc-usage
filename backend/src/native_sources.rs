//! 经核实的本机来源：只读第三方文件，记录写入本应用数据库。
use crate::{
    collector::ScanResult,
    native_parse as parse,
    source_store::{self, Record},
};
use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize)]
pub struct Source {
    pub id: String,
    pub platform: String,
    pub name: String,
    pub available: bool,
    pub capabilities: Vec<String>,
    pub reason: Option<String>,
    #[serde(skip)]
    pub path: PathBuf,
}

pub fn home() -> Option<PathBuf> {
    crate::creds::home()
}
fn app_data() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_default()
}

pub fn sources_at(home: &Path, app: &Path) -> Vec<Source> {
    [
        ("gemini", "cli", "Gemini CLI", home.join(".gemini/tmp")),
        ("grok", "cli", "Grok CLI", home.join(".grok/auth.json")),
        (
            "zcode",
            "main",
            "Zcode",
            home.join(".zcode/cli/db/db.sqlite"),
        ),
        ("trae", "ide", "Trae IDE", app.join("Trae")),
        (
            "qoder",
            "global",
            "Qoder 国际版",
            home.join(".qoder/projects"),
        ),
        (
            "qoder",
            "cn",
            "Qoder 国内版",
            home.join(".qoder-cn/projects"),
        ),
        (
            "workbuddy",
            "main",
            "WorkBuddy",
            home.join(".workbuddy/projects"),
        ),
        (
            "workbuddy",
            "ai",
            "WorkBuddy AI",
            home.join(".workbuddy-ai/projects"),
        ),
    ]
    .into_iter()
    .map(|(platform, instance, name, path)| {
        let implemented =
            crate::platforms::supports(platform, "local_sessions") || platform == "grok";
        let available = path.exists() && implemented;
        Source {
            id: format!("native:{platform}:{instance}"),
            platform: platform.into(),
            name: name.into(),
            available,
            capabilities: crate::platforms::get(platform)
                .map(|p| p.capabilities.clone())
                .unwrap_or_default(),
            reason: if !implemented {
                Some(crate::platforms::limitation(platform))
            } else if !available {
                Some(format!(
                    "未发现 {name} 的本机数据，请先在对应应用中登录并产生记录"
                ))
            } else {
                Some(crate::platforms::limitation(platform))
            },
            path,
        }
    })
    .collect()
}

pub fn sources(platform: &str) -> Vec<Source> {
    home()
        .map(|h| {
            sources_at(&h, &app_data())
                .into_iter()
                .filter(|s| s.platform == platform)
                .collect()
        })
        .unwrap_or_default()
}
pub fn monitor_id(id: &str) -> Option<&str> {
    id.strip_prefix("monitor:")
}

pub fn register(
    conn: &Connection,
    platform: &str,
    name: Option<&str>,
) -> Result<(usize, usize, Vec<String>), String> {
    if !crate::platforms::native(platform) {
        return Err("不是本机来源平台".into());
    }
    if let Some(name) = name {
        if name.trim().is_empty() || name.chars().count() > 80 {
            return Err("连接名称应为 1 到 80 个字符".into());
        }
    }
    let found: Vec<_> = sources(platform)
        .into_iter()
        .filter(|s| s.available)
        .collect();
    if found.is_empty() {
        return Err(sources(platform)
            .first()
            .and_then(|s| s.reason.clone())
            .unwrap_or_else(|| "未发现可用本机来源".into()));
    }
    register_sources(conn, found, name)
}
fn register_sources(
    conn: &Connection,
    found: Vec<Source>,
    name: Option<&str>,
) -> Result<(usize, usize, Vec<String>), String> {
    let mut added = 0;
    let mut existing = 0;
    let mut warnings = Vec::new();
    for s in found {
        if let Err(reason) = validate_source(&s) {
            warnings.push(format!("{}：{reason}", s.name));
            continue;
        }
        let id = format!("monitor:{}", s.id);
        let changed=conn.execute("INSERT OR IGNORE INTO connections(id,platform,kind,name,label,secret,masked,status,created_ms)
            VALUES(?1,?2,'auth',?3,'本机来源',NULL,'','paused',?4)",
            params![id,s.platform,name.map(str::trim).unwrap_or(&s.name),chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        if changed > 0 {
            added += 1;
        } else {
            existing += 1;
        }
    }
    if added + existing == 0 {
        return Err(warnings.join("；"));
    }
    Ok((added, existing, warnings))
}

pub fn check(id: &str, platform: &str) -> Result<Source, String> {
    let key = monitor_id(id).ok_or("该旧连接没有有效的本机来源标识，请重新获取")?;
    let source = sources(platform)
        .into_iter()
        .find(|s| s.id == key)
        .ok_or("本机来源标识无效")?;
    if !source.available {
        return Err(source.reason.unwrap_or_else(|| "本机来源不可用".into()));
    }
    validate_source(&source)?;
    Ok(source)
}

fn validate_source(source: &Source) -> Result<(), String> {
    if source.platform == "grok" {
        return crate::creds::local_grok_auth().map(|_| ());
    }
    if source.platform == "zcode" {
        readonly(&source.path)?.prepare("SELECT id,session_id,input_tokens,cache_read_input_tokens,cache_creation_input_tokens,raw_usage_json FROM model_usage LIMIT 0")
            .map_err(|_|"Zcode 数据库结构不受支持，尚不能启用采集".to_string())?;
        return Ok(());
    }
    let mut paths = Vec::new();
    files(&source.path, &mut paths)?;
    let mut malformed = 0;
    for path in paths {
        if source.platform == "gemini"
            && !path
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.starts_with("session-"))
        {
            continue;
        }
        if source.platform != "gemini" && path.extension().is_none_or(|s| s != "jsonl") {
            continue;
        }
        let lines = match json_lines(&path) {
            Ok(lines) => lines,
            Err(_) => {
                malformed += 1;
                continue;
            }
        };
        let valid = if source.platform == "gemini" {
            !parse::gemini_records(&lines, &source.id).is_empty()
        } else {
            lines.iter().any(|line| {
                if source.platform == "workbuddy" {
                    parse::workbuddy(line, &source.id, "probe").is_some()
                } else {
                    parse::qoder(line, &source.id, "probe", None).is_some()
                }
            })
        };
        if valid {
            return Ok(());
        }
    }
    Err(if malformed > 0 {
        "未找到可解析的用量记录，部分文件格式无效"
    } else {
        "目录已发现，但尚无可采集的用量记录；请先在对应应用完成一次会话"
    }
    .into())
}

fn readonly(path: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .and_then(|c| {
        c.busy_timeout(std::time::Duration::from_millis(200))?;
        Ok(c)
    })
    .map_err(|e| format!("只读数据库无法打开：{e}"))
}

fn files(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(path).map_err(|e| format!("无法读取来源目录：{e}"))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            continue;
        }
        let p = entry.path();
        if kind.is_dir() {
            files(&p, out)?;
        } else if p.extension().is_some_and(|s| s == "jsonl" || s == "json") {
            out.push(p);
        }
    }
    Ok(())
}
fn stamp(path: &Path) -> Result<(i64, i64), String> {
    let m = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let time = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    Ok((m.len() as i64, time))
}
fn save_stamp(conn: &Connection, key: &str, size: i64, time: i64) -> rusqlite::Result<usize> {
    conn.execute("INSERT INTO scan_state(path,offset,mtime) VALUES(?1,?2,?3) ON CONFLICT(path) DO UPDATE SET offset=?2,mtime=?3",params![key,size,time])
}

fn json_lines(path: &Path) -> Result<Vec<Value>, String> {
    if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 {
        return Err("会话文件超过 64 MiB 读取上限".into());
    }
    if path.extension().is_some_and(|s| s == "json") {
        if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 {
            return Err("会话快照超过 64 MiB 读取上限".into());
        }
        return serde_json::from_reader(std::fs::File::open(path).map_err(|e| e.to_string())?)
            .map(|v| vec![v])
            .map_err(|_| "会话 JSON 格式无效".into());
    }
    let mut reader = BufReader::new(std::fs::File::open(path).map_err(|e| e.to_string())?);
    let mut out = Vec::new();
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        let read = reader
            .read_until(b'\n', &mut bytes)
            .map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        if bytes.last() != Some(&b'\n') {
            break;
        } // 尚在追加的尾行留给下次
        if bytes.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        out.push(
            serde_json::from_slice(&bytes)
                .map_err(|_| "会话含无效 JSON 行，未推进读取位点".to_string())?,
        );
    }
    Ok(out)
}

fn qoder_availability(app: &Path, source: &str) -> HashMap<String, (bool, i64)> {
    let product = if source.ends_with(":cn") {
        "com.qodercn.app.stable"
    } else {
        "com.qoder.app.stable"
    };
    let Ok(db) = readonly(&app.join(product).join("main.sqlite")) else {
        return HashMap::new();
    };
    let Ok(mut q) =
        db.prepare("SELECT session_id,snapshot_json,updated_at FROM chat_session_context_usage")
    else {
        return HashMap::new();
    };
    let Ok(rows) = q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    }) else {
        return HashMap::new();
    };
    rows.flatten()
        .filter_map(|(id, json, ts)| {
            let v: Value = serde_json::from_str(&json).ok()?;
            Some((id, (v.get("tokenCountsAvailable")?.as_bool()?, ts)))
        })
        .collect()
}

fn scan_files(
    conn: &mut Connection,
    source: &Source,
    app: &Path,
) -> Result<(usize, Vec<String>), String> {
    let mut paths = Vec::new();
    files(&source.path, &mut paths)?;
    paths.sort();
    let availability = if source.platform == "qoder" {
        qoder_availability(app, &source.id)
    } else {
        HashMap::new()
    };
    let mut changed = 0;
    let mut errors = Vec::new();
    for path in paths {
        let result = (|| -> Result<usize, String> {
            if source.platform == "gemini" {
                if !path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.starts_with("session-"))
                {
                    return Ok(0);
                }
                if path.extension().is_some_and(|s| s == "json")
                    && path.with_extension("jsonl").exists()
                {
                    return Ok(0);
                }
            } else if !path.extension().is_some_and(|s| s == "jsonl") {
                return Ok(0);
            }
            let (size, time) = stamp(&path)?;
            let key = path.to_string_lossy().to_string();
            if crate::db::get_scan_state(conn, &key) == Some((size, time)) {
                return Ok(0);
            }
            let lines = json_lines(&path)?;
            let fallback = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown");
            let records = if source.platform == "gemini" {
                parse::gemini_records(&lines, &source.id)
            } else {
                let mut latest = HashMap::new();
                for line in &lines {
                    let r = if source.platform == "workbuddy" {
                        parse::workbuddy(line, &source.id, fallback)
                    } else {
                        let session = parse::text(line, "sessionId").unwrap_or(fallback);
                        let available = availability.get(session).and_then(|(b, t)| {
                            // 当前上下文快照只能解释其附近同一轮记录，不能覆盖旧历史。
                            let ts = line.get("timestamp").and_then(parse::timestamp)?;
                            ((ts - *t).abs() <= 60_000).then_some(*b)
                        });
                        parse::qoder(line, &source.id, fallback, available)
                    };
                    if let Some(r) = r {
                        latest.insert(r.request.dedup_key.clone(), r);
                    }
                }
                latest.into_values().collect()
            };
            let tx = conn.transaction().map_err(|e| e.to_string())?;
            let mut file_changed = source_store::write(&tx, &records).map_err(|e| e.to_string())?;
            for r in &records {
                if let Some(id) = &r.request.session_id {
                    crate::db::set_session_activity(
                        &tx,
                        &source.platform,
                        id,
                        "unknown",
                        r.request.ts,
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
            if source.platform == "gemini" {
                // 原文件是留存快照；官方 rewind 后移除的消息也从该快照统计中移除。
                if let Some(sid) = lines.iter().rev().find_map(|v| {
                    parse::text(v, "sessionId")
                        .or_else(|| v.get("$set").and_then(|v| parse::text(v, "sessionId")))
                }) {
                    let id = format!("{}:{sid}", source.id);
                    let keys: HashSet<_> = records
                        .iter()
                        .map(|r| r.request.dedup_key.as_str())
                        .collect();
                    let old: Vec<(i64, String)> = {
                        let mut q=tx.prepare("SELECT id,dedup_key FROM requests WHERE source=?1 AND session_id=?2").map_err(|e|e.to_string())?;
                        let rows = q
                            .query_map(params![source.id, id], |r| Ok((r.get(0)?, r.get(1)?)))
                            .map_err(|e| e.to_string())?;
                        rows.collect::<rusqlite::Result<_>>()
                            .map_err(|e| e.to_string())?
                    };
                    for (id, key) in old {
                        if !keys.contains(key.as_str()) {
                            tx.execute("DELETE FROM request_metrics WHERE request_id=?1", [id])
                                .map_err(|e| e.to_string())?;
                            tx.execute("DELETE FROM usage_events WHERE request_id=?1", [id])
                                .map_err(|e| e.to_string())?;
                            file_changed += tx
                                .execute("DELETE FROM requests WHERE id=?1", [id])
                                .map_err(|e| e.to_string())?;
                        }
                    }
                }
            }
            save_stamp(&tx, &key, size, time).map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok(file_changed)
        })();
        match result {
            Ok(n) => changed += n,
            Err(error) => errors.push(format!(
                "{}：{error}",
                path.file_name().unwrap_or_default().to_string_lossy()
            )),
        }
    }
    Ok((changed, errors))
}

fn zcode(conn: &mut Connection, source: &Source) -> Result<usize, String> {
    let key = source.path.to_string_lossy().to_string();
    let wal = PathBuf::from(format!("{key}-wal"));
    let db_stamp = stamp(&source.path)?;
    let wal_stamp = if wal.exists() { stamp(&wal)? } else { (0, 0) };
    if crate::db::get_scan_state(conn, &key) == Some(db_stamp)
        && crate::db::get_scan_state(conn, &format!("{key}-wal")) == Some(wal_stamp)
    {
        return Ok(0);
    }
    let input = readonly(&source.path)?;
    let mut stmt = input
        .prepare(
            "SELECT u.id,u.session_id,u.model_id,u.status,u.started_at,u.completed_at,
        u.input_tokens,u.output_tokens,u.cache_read_input_tokens,u.cache_creation_input_tokens,
        u.provider_total_tokens,u.computed_total_tokens,u.reasoning_tokens,u.raw_usage_json,s.title
        FROM model_usage u LEFT JOIN session s ON s.id=u.session_id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let session: String = row.get(1)?;
            let status: String = row.get(3)?;
            let ts = row.get::<_, Option<i64>>(5)?.unwrap_or(row.get(4)?);
            let raw: Option<String> = row.get(13)?;
            let raw: Value = raw
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(Value::Null);
            let known = raw.is_object() && raw.as_object().is_some_and(|v| !v.is_empty());
            let number = |i| -> rusqlite::Result<Option<i64>> {
                Ok(row
                    .get::<_, Option<i64>>(i)?
                    .filter(|n| *n >= 0 && (known || *n > 0)))
            };
            let input = number(6)?;
            let output = number(7)?;
            let cache_read = number(8)?;
            let cache_write = number(9)?;
            let provider_total = row.get::<_, Option<i64>>(10)?;
            let total = provider_total.or(if known { row.get(11)? } else { None });
            let semantics = if raw.get("prompt_tokens").is_some()
                || (raw.get("inputTokens").is_some()
                    && input
                        .zip(output)
                        .zip(total)
                        .is_some_and(|((i, o), t)| i + o == t))
            {
                "includes_cache"
            } else if raw.get("cache_creation_input_tokens").is_some()
                && input
                    .zip(output)
                    .zip(cache_read)
                    .zip(cache_write)
                    .zip(total)
                    .is_some_and(|((((i, o), r), w), t)| i + o + r + w == t)
            {
                "excludes_cache"
            } else {
                "unknown"
            };
            Ok((
                Record {
                    request: crate::db::RequestRecord {
                        platform: "zcode".into(),
                        source: source.id.clone(),
                        dedup_key: id,
                        session_id: Some(format!("{}:{session}", source.id)),
                        ts,
                        model: row.get(2)?,
                        input_tokens: input,
                        output_tokens: output,
                        cache_read_tokens: cache_read,
                        cache_write_tokens: cache_write,
                        total_tokens: total,
                        effort: None,
                    },
                    input_semantics: semantics,
                    credits: None,
                    original_credits: None,
                    billable: None,
                    context_ratio: None,
                    reasoning: number(12)?,
                },
                status,
                row.get::<_, Option<String>>(14)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let mut changed = 0;
    for (r, status, title) in rows {
        changed += source_store::write(&tx, &[r.clone()]).map_err(|e| e.to_string())?;
        tx.execute("UPDATE requests SET native_outcome=?1 WHERE source=?2 AND dedup_key=?3 AND native_outcome IS NOT ?1",
            params![match status.as_str() {"completed"=>"success","error"|"cancelled"=>"failed",_=>"unknown"},source.id,r.request.dedup_key]).map_err(|e|e.to_string())?;
        if let Some(id) = &r.request.session_id {
            if let Some(title) = title {
                crate::db::upsert_session_title(&tx, "zcode", id, &title)
                    .map_err(|e| e.to_string())?;
            }
            let state = match status.as_str() {
                "completed" => "done",
                "error" | "cancelled" => "failed",
                _ => "unknown",
            };
            crate::db::set_session_activity(&tx, "zcode", id, state, r.request.ts)
                .map_err(|e| e.to_string())?;
        }
    }
    save_stamp(&tx, &key, db_stamp.0, db_stamp.1).map_err(|e| e.to_string())?;
    save_stamp(&tx, &format!("{key}-wal"), wal_stamp.0, wal_stamp.1).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(changed)
}

fn session_metadata(conn: &mut Connection, source: &Source, app: &Path) -> Result<usize, String> {
    let path = if source.platform == "workbuddy" {
        source
            .path
            .parent()
            .unwrap_or(&source.path)
            .join("workbuddy.db")
    } else if source.platform == "qoder" {
        app.join(if source.id.ends_with(":cn") {
            "com.qodercn.app.stable"
        } else {
            "com.qoder.app.stable"
        })
        .join("main.sqlite")
    } else {
        return Ok(0);
    };
    if !path.exists() {
        return Ok(0);
    }
    let input = readonly(&path)?;
    let sql = if source.platform == "workbuddy" {
        "SELECT s.id,COALESCE(NULLIF(s.custom_title,''),s.title),s.model,u.used,u.size,u.updated_at,NULL
         FROM sessions s LEFT JOIN session_usage u ON u.session_id=s.id WHERE s.deleted_at IS NULL"
    } else {
        "SELECT s.session_id,s.title,s.model,NULL,NULL,u.updated_at,u.snapshot_json
         FROM chat_sessions s LEFT JOIN chat_session_context_usage u ON u.session_id=s.session_id WHERE s.deleted_at IS NULL"
    };
    let mut q = input.prepare(sql).map_err(|e| e.to_string())?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, Option<i64>>(5)?,
                r.get::<_, Option<String>>(6)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let mut changed = 0;
    for (id, title, model, used, size, ts, snapshot) in rows {
        let session = format!("{}:{id}", source.id);
        if let Some(title) = title.filter(|s| !s.trim().is_empty()) {
            crate::db::upsert_session_title(&tx, &source.platform, &session, &title)
                .map_err(|e| e.to_string())?;
        }
        let ratio = used
            .zip(size)
            .filter(|(u, s)| *u >= 0 && *s > 0 && *u <= *s)
            .map(|(u, s)| u as f64 / s as f64)
            .or_else(|| {
                snapshot
                    .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                    .and_then(|v| parse::amount(&v, "percentage"))
            });
        if let Some((ratio, ts)) = ratio.zip(ts) {
            changed += source_store::context(
                &tx,
                &source_store::ContextSnapshot {
                    platform: source.platform.clone(),
                    source: source.id.clone(),
                    session_id: session,
                    model,
                    ratio,
                    ts,
                },
            )
            .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(changed)
}

pub fn scan_at(conn: &mut Connection, home: &Path, app: &Path) -> ScanResult {
    let mut result = ScanResult::default();
    for source in sources_at(home, app) {
        if !source.available || !crate::platforms::supports(&source.platform, "local_sessions") {
            continue;
        }
        // 显式暂停的本机连接停止该来源采集；尚未添加连接时保持本地统计自动发现。
        let paused: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM connections WHERE id=?1 AND status='paused')",
                [format!("monitor:{}", source.id)],
                |r| r.get(0),
            )
            .unwrap_or(false);
        if paused {
            continue;
        }
        let read = if source.platform == "zcode" {
            zcode(conn, &source).map(|n| (n, Vec::new()))
        } else {
            scan_files(conn, &source, app)
        };
        result.files_scanned += 1;
        match read {
            Ok((n, errors)) => {
                result.records_inserted += n;
                if !errors.is_empty() {
                    result.failed_sources.push(source.platform.clone());
                    result
                        .errors
                        .push(format!("{}：{}", source.name, errors.join("；")));
                }
            }
            Err(reason) => {
                result.failed_sources.push(source.platform.clone());
                result.errors.push(format!("{}：{reason}", source.name));
            }
        }
        match session_metadata(conn, &source, app) {
            Ok(n) => result.records_inserted += n,
            Err(reason) => {
                result.failed_sources.push(source.platform.clone());
                result
                    .errors
                    .push(format!("{} 会话元数据：{reason}", source.name));
            }
        }
    }
    result
}
pub fn scan(conn: &mut Connection) -> ScanResult {
    home()
        .map(|h| scan_at(conn, &h, &app_data()))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "ai-usage-native-{label}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
    #[test]
    fn one_unusable_instance_does_not_block_registration_of_another() {
        let root = fixture_dir("register");
        let empty = root.join(".qoder/projects/p");
        let valid = root.join(".qoder-cn/projects/p");
        std::fs::create_dir_all(&empty).unwrap();
        std::fs::create_dir_all(&valid).unwrap();
        std::fs::write(valid.join("s.jsonl"),format!("{}\n",serde_json::json!({"type":"assistant","timestamp":1000,"sessionId":"s","message":{"id":"m","usage":{"credits":1.0}}}))).unwrap();
        let conn = crate::db::open(Path::new(":memory:")).unwrap();
        let sources = sources_at(&root, &root)
            .into_iter()
            .filter(|s| s.platform == "qoder" && s.available)
            .collect();
        let (added, existing, warnings) = register_sources(&conn, sources, None).unwrap();
        assert_eq!((added, existing, warnings.len()), (1, 0, 1));
        assert_eq!(
            crate::connections::list(&conn).unwrap()[0].id,
            "monitor:native:qoder:cn"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
    #[test]
    fn gemini_checkpoint_rewind_and_new_events_keep_monotonic_cursor() {
        let root = fixture_dir("rewind");
        let folder = root.join(".gemini/tmp/project/chats");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("session-test.jsonl");
        let message = |id: &str| serde_json::json!({"id":id,"type":"gemini","timestamp":1000,"tokens":{"input":100,"output":5,"cached":80,"total":105}});
        let save = |messages: Vec<Value>| {
            std::fs::write(
                &path,
                format!(
                    "{}\n",
                    serde_json::json!({"$set":{"sessionId":"s","messages":messages}})
                ),
            )
            .unwrap()
        };
        let mut conn = crate::db::open(Path::new(":memory:")).unwrap();
        save(vec![message("a"), message("b")]);
        assert!(scan_at(&mut conn, &root, &root).errors.is_empty());
        let first = crate::db::live_usage(&conn, "gemini", None).unwrap().cursor;
        save(vec![message("a")]);
        assert!(scan_at(&mut conn, &root, &root).errors.is_empty());
        let second = crate::db::live_usage(&conn, "gemini", Some(first))
            .unwrap()
            .cursor;
        assert!(second > first);
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM requests", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        save(vec![message("a"), message("new-long-id")]);
        scan_at(&mut conn, &root, &root);
        let next = crate::db::live_usage(&conn, "gemini", Some(second)).unwrap();
        assert!(next.cursor > second);
        assert_eq!(next.delta_tokens, 25);
        std::fs::remove_dir_all(&root).unwrap();
    }
    #[test]
    fn damaged_and_partial_files_do_not_block_other_sessions_or_advance_bad_checkpoint() {
        let root = fixture_dir("isolation");
        let folder = root.join(".workbuddy/projects/p");
        std::fs::create_dir_all(&folder).unwrap();
        let bad = folder.join("a-bad.jsonl");
        std::fs::write(&bad, "{bad}\n").unwrap();
        let good = folder.join("b-good.jsonl");
        let line = serde_json::json!({"id":"m","sessionId":"s","timestamp":1000,"providerData":{"usage":{"inputTokens":10,"outputTokens":5,"totalTokens":15},"rawUsage":{"prompt_tokens":10,"prompt_tokens_details":{"cached_tokens":0},"credit":0.5}}});
        std::fs::write(&good, format!("{line}\n{{\"partial\":")).unwrap();
        let mut conn = crate::db::open(Path::new(":memory:")).unwrap();
        let result = scan_at(&mut conn, &root, &root);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.records_inserted, 1);
        assert_eq!(
            crate::db::get_scan_state(&conn, &bad.to_string_lossy()),
            None
        );
        assert_eq!(scan_at(&mut conn, &root, &root).records_inserted, 0);
        crate::db::cleanup_history(&mut conn, 2000).unwrap();
        std::fs::write(&good, format!("{line}\n\n")).unwrap();
        scan_at(&mut conn, &root, &root);
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM requests", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
    #[test]
    #[ignore = "显式只读本机来源，应用写入仅在内存数据库"]
    fn read_only_native_smoke() {
        let mut conn = crate::db::open(Path::new(":memory:")).unwrap();
        let result = scan_at(&mut conn, &home().unwrap(), &app_data());
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        let mut q=conn.prepare("SELECT platform,COUNT(*),SUM(total_tokens),SUM(cache_read_tokens),SUM(cache_write_tokens),SUM(total_known) FROM requests GROUP BY platform").unwrap();
        let counts: Vec<_> = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        eprintln!("仅输出统计汇总：{counts:?}");
        drop(q);
        assert_eq!(
            scan_at(&mut conn, &home().unwrap(), &app_data()).records_inserted,
            0
        );
        for (platform, _, _, _, _, _) in &counts {
            crate::db::usage_breakdown_at(
                &conn,
                platform,
                "total",
                None,
                None,
                chrono::Utc::now().timestamp_millis(),
            )
            .unwrap();
            crate::db::trend_at(
                &conn,
                platform,
                "total",
                None,
                None,
                chrono::Utc::now().timestamp_millis(),
            )
            .unwrap();
        }
        let json = crate::db::export_backup(&conn, None).unwrap();
        let mut restored = crate::db::open(Path::new(":memory:")).unwrap();
        crate::db::import_backup(&mut restored, &json).unwrap();
        for (platform, _, _, _, _, _) in counts {
            assert_eq!(
                source_store::metrics(&conn, &platform, 0, i64::MAX, None)
                    .unwrap()
                    .credits,
                source_store::metrics(&restored, &platform, 0, i64::MAX, None)
                    .unwrap()
                    .credits
            );
        }
    }
    #[test]
    fn missing_sources_are_not_created_or_reported_connected() {
        let path = std::env::temp_dir().join(format!("missing-native-{}", std::process::id()));
        let list = sources_at(&path, &path);
        assert_eq!(list.len(), 8);
        assert!(list.iter().all(|s| !s.available));
        assert!(!path.exists());
        assert!(readonly(&path.join("missing.db")).is_err());
        assert!(!path.exists());
    }
}
