//! Codex 标题索引独立于 Token 日志，改名也必须单独同步。
use std::{collections::HashMap, path::PathBuf};
use rusqlite::Connection;
use serde::Deserialize;

pub fn codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME").map(PathBuf::from).or_else(|| {
        std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))
            .map(|home| PathBuf::from(home).join(".codex"))
    })
}

#[derive(Deserialize)]
struct Entry { id: String, thread_name: String, updated_at: String }

fn parse_index(text: &str) -> HashMap<String, (i64, String)> {
    let mut titles = HashMap::new();
    for line in text.lines() {
        let Ok(entry) = serde_json::from_str::<Entry>(line) else { continue };
        let title = entry.thread_name.trim();
        if entry.id.is_empty() || title.is_empty() { continue; }
        let Ok(updated) = chrono::DateTime::parse_from_rfc3339(&entry.updated_at) else { continue };
        let updated = updated.timestamp_millis();
        let row = titles.entry(entry.id).or_insert_with(|| (updated, title.to_owned()));
        if updated >= row.0 { *row = (updated, title.to_owned()); }
    }
    titles
}

pub fn sync_codex(conn: &Connection, dir: &std::path::Path) -> Result<(), String> {
    let text = match std::fs::read_to_string(dir.join("session_index.jsonl")) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("读取 Codex 会话标题失败: {e}")),
    };
    for (id, (_, title)) in parse_index(&text) {
        crate::db::upsert_session_title(conn, "codex", &id, &title).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn titles_use_ids_for_matching_and_latest_timestamp_for_renames() {
        let rows = parse_index(concat!(
            "{\"id\":\"one\",\"thread_name\":\"整理前后端依赖文件目录\",\"updated_at\":\"2026-09-16T02:00:00Z\"}\n",
            "{\"id\":\"two\",\"thread_name\":\"另一会话\",\"updated_at\":\"2026-09-16T01:00:00Z\"}\n",
            "{\"id\":\"one\",\"thread_name\":\"旧标题\",\"updated_at\":\"2026-09-16T01:00:00Z\"}\n",
            "{\"id\":\"one\",\"thread_name\":\" \",\"updated_at\":\"2026-09-16T03:00:00Z\"}\n",
            "{partial"
        ));
        assert_eq!(rows["one"].1, "整理前后端依赖文件目录");
        assert_eq!(rows["two"].1, "另一会话");
        assert_eq!(rows.len(), 2);
    }
}
