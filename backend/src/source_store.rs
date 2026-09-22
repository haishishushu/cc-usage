//! 原生来源的完整记录写入。积分和上下文占用不进入 Token 总量。
use crate::db::RequestRecord;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Record {
    pub request: RequestRecord,
    pub input_semantics: &'static str,
    pub credits: Option<f64>,
    pub original_credits: Option<f64>,
    pub billable: Option<bool>,
    pub context_ratio: Option<f64>,
    pub reasoning: Option<i64>,
}

pub fn init(conn: &Connection) -> rusqlite::Result<()> {
    let _ = conn.execute("ALTER TABLE requests ADD COLUMN native_outcome TEXT", []);
    if conn
        .execute(
            "ALTER TABLE requests ADD COLUMN input_semantics TEXT NOT NULL DEFAULT 'unknown'",
            [],
        )
        .is_ok()
    {
        conn.execute("UPDATE requests SET input_semantics=CASE platform WHEN 'claude' THEN 'excludes_cache' WHEN 'codex' THEN 'includes_cache' ELSE 'unknown' END", [])?;
    }
    conn.execute_batch("CREATE TABLE IF NOT EXISTS request_metrics(
        request_id INTEGER PRIMARY KEY REFERENCES requests(id),
        credits REAL, original_credits REAL, billable INTEGER, context_ratio REAL, reasoning INTEGER
    );
    CREATE TABLE IF NOT EXISTS native_context(platform TEXT NOT NULL,source TEXT NOT NULL,session_id TEXT NOT NULL,
      model TEXT,ratio REAL NOT NULL,ts INTEGER NOT NULL,PRIMARY KEY(source,session_id));
    CREATE TABLE IF NOT EXISTS collection_policy(singleton INTEGER PRIMARY KEY CHECK(singleton=1), cutoff_ms INTEGER NOT NULL);
    INSERT OR IGNORE INTO collection_policy VALUES(1,0);
    CREATE TABLE IF NOT EXISTS event_sequence(singleton INTEGER PRIMARY KEY CHECK(singleton=1), value INTEGER NOT NULL);
    INSERT OR IGNORE INTO event_sequence SELECT 1,COALESCE(MAX(id),0) FROM usage_events;
    CREATE TRIGGER IF NOT EXISTS usage_event_sequence_insert AFTER INSERT ON usage_events
    BEGIN
      UPDATE usage_events SET id=max(NEW.id,(SELECT value+1 FROM event_sequence WHERE singleton=1)) WHERE id=NEW.id;
      UPDATE event_sequence SET value=(SELECT MAX(id) FROM usage_events) WHERE singleton=1;
    END;
    CREATE TRIGGER IF NOT EXISTS usage_event_sequence_delete AFTER DELETE ON usage_events
    BEGIN UPDATE event_sequence SET value=value+1 WHERE singleton=1; END;
    CREATE TRIGGER IF NOT EXISTS legacy_input_semantics AFTER INSERT ON requests
    WHEN NEW.input_semantics='unknown' AND NEW.platform IN ('claude','codex')
    BEGIN UPDATE requests SET input_semantics=CASE NEW.platform WHEN 'claude' THEN 'excludes_cache' ELSE 'includes_cache' END WHERE id=NEW.id; END;
    ")
}

pub fn write(conn: &Connection, records: &[Record]) -> rusqlite::Result<usize> {
    let mut changed = 0;
    let cutoff: i64 = conn.query_row(
        "SELECT cutoff_ms FROM collection_policy WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    for item in records {
        let r = &item.request;
        if r.ts < cutoff {
            continue;
        }
        let old: Option<(i64, Option<i64>, Option<i64>)> = conn.query_row(
            "SELECT id, CASE WHEN total_known=1 THEN total_tokens END,
             CASE WHEN input_known=1 AND output_known=1 THEN
               CASE input_semantics WHEN 'includes_cache' THEN CASE WHEN cache_read_known=1 THEN max(input_tokens-cache_read_tokens,0)+output_tokens END
                 WHEN 'excludes_cache' THEN input_tokens+output_tokens END END
             FROM requests WHERE source=?1 AND dedup_key=?2",
            params![r.source, r.dedup_key], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
        ).optional()?;
        let incoming = [
            r.input_tokens,
            r.output_tokens,
            r.cache_read_tokens,
            r.cache_write_tokens,
            r.total_tokens,
        ];
        let count = conn.execute("INSERT INTO requests(platform,source,dedup_key,session_id,ts,model,
            input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,total_tokens,
            input_known,output_known,cache_read_known,cache_write_known,total_known,effort,input_semantics)
            VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)
            ON CONFLICT(source,dedup_key) DO UPDATE SET
            session_id=excluded.session_id,ts=excluded.ts,model=excluded.model,
            input_tokens=excluded.input_tokens,output_tokens=excluded.output_tokens,
            cache_read_tokens=excluded.cache_read_tokens,cache_write_tokens=excluded.cache_write_tokens,
            total_tokens=excluded.total_tokens,input_known=excluded.input_known,output_known=excluded.output_known,
            cache_read_known=excluded.cache_read_known,cache_write_known=excluded.cache_write_known,
            total_known=excluded.total_known,effort=excluded.effort,input_semantics=excluded.input_semantics
            WHERE (session_id,ts,model,input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,total_tokens,
              input_known,output_known,cache_read_known,cache_write_known,total_known,effort,input_semantics)
            IS NOT (excluded.session_id,excluded.ts,excluded.model,excluded.input_tokens,excluded.output_tokens,
              excluded.cache_read_tokens,excluded.cache_write_tokens,excluded.total_tokens,excluded.input_known,
              excluded.output_known,excluded.cache_read_known,excluded.cache_write_known,excluded.total_known,
              excluded.effort,excluded.input_semantics)", params![r.platform,r.source,r.dedup_key,r.session_id,r.ts,r.model,
              incoming[0].unwrap_or(0),incoming[1].unwrap_or(0),incoming[2].unwrap_or(0),incoming[3].unwrap_or(0),incoming[4].unwrap_or(0),
              incoming[0].is_some(),incoming[1].is_some(),incoming[2].is_some(),incoming[3].is_some(),incoming[4].is_some(),r.effort,item.input_semantics])?;
        let id = match old {
            Some((id, _, _)) => id,
            None => conn.last_insert_rowid(),
        };
        let metric_count = conn.execute("INSERT INTO request_metrics(request_id,credits,original_credits,billable,context_ratio,reasoning)
            VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(request_id) DO UPDATE SET credits=excluded.credits,
            original_credits=excluded.original_credits,billable=excluded.billable,context_ratio=excluded.context_ratio,reasoning=excluded.reasoning
            WHERE (credits,original_credits,billable,context_ratio,reasoning) IS NOT
              (excluded.credits,excluded.original_credits,excluded.billable,excluded.context_ratio,excluded.reasoning)",
            params![id,item.credits,item.original_credits,item.billable,item.context_ratio,item.reasoning])?;
        if count > 0 || metric_count > 0 {
            changed += 1;
        }
        let fresh = match item.input_semantics {
            "includes_cache" => r
                .input_tokens
                .zip(r.cache_read_tokens)
                .zip(r.output_tokens)
                .map(|((i, c), o)| (i - c).max(0) + o),
            "excludes_cache" => r.input_tokens.zip(r.output_tokens).map(|(i, o)| i + o),
            _ => None,
        };
        let delta = r.total_tokens.unwrap_or(0) - old.and_then(|o| o.1).unwrap_or(0);
        let delta_fresh = fresh.unwrap_or(0) - old.and_then(|o| o.2).unwrap_or(0);
        if delta != 0 || delta_fresh != 0 {
            conn.execute("INSERT INTO usage_events(request_id,platform,session_id,ts,delta_total,delta_fresh) VALUES(?1,?2,?3,?4,?5,?6)",
                params![id,r.platform,r.session_id,r.ts,delta,delta_fresh])?;
        }
    }
    Ok(changed)
}

#[derive(Serialize)]
pub struct Metrics {
    pub credits: Option<f64>,
    pub credit_requests: i64,
    pub context_ratio: Option<f64>,
    pub context_at_ms: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContextSnapshot {
    pub platform: String,
    pub source: String,
    pub session_id: String,
    pub model: Option<String>,
    pub ratio: f64,
    pub ts: i64,
}

pub fn context(conn: &Connection, value: &ContextSnapshot) -> rusqlite::Result<usize> {
    if !value.ratio.is_finite() || !(0.0..=1.0).contains(&value.ratio) || value.ts <= 0 {
        return Ok(0);
    }
    let cutoff: i64 = conn.query_row(
        "SELECT cutoff_ms FROM collection_policy WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    if value.ts < cutoff {
        return Ok(0);
    }
    conn.execute("INSERT INTO native_context(platform,source,session_id,model,ratio,ts) VALUES(?1,?2,?3,?4,?5,?6)
        ON CONFLICT(source,session_id) DO UPDATE SET model=excluded.model,ratio=excluded.ratio,ts=excluded.ts
        WHERE excluded.ts>=native_context.ts AND (model,ratio,ts) IS NOT (excluded.model,excluded.ratio,excluded.ts)",
        params![value.platform,value.source,value.session_id,value.model,value.ratio,value.ts])
}

pub fn contexts(
    conn: &Connection,
    platform: Option<&str>,
) -> rusqlite::Result<Vec<ContextSnapshot>> {
    let mut q=conn.prepare("SELECT platform,source,session_id,model,ratio,ts FROM native_context WHERE ?1 IS NULL OR platform=?1")?;
    let result = q
        .query_map([platform], |r| {
            Ok(ContextSnapshot {
                platform: r.get(0)?,
                source: r.get(1)?,
                session_id: r.get(2)?,
                model: r.get(3)?,
                ratio: r.get(4)?,
                ts: r.get(5)?,
            })
        })?
        .collect();
    result
}

pub fn metrics(
    conn: &Connection,
    platform: &str,
    start: i64,
    end: i64,
    model: Option<&str>,
) -> rusqlite::Result<Metrics> {
    let (credits,credit_requests) = conn.query_row("SELECT SUM(m.credits),COUNT(m.credits) FROM request_metrics m
        JOIN requests r ON r.id=m.request_id WHERE r.platform=?1 AND r.ts>=?2 AND r.ts<?3 AND (?4 IS NULL OR r.model=?4)",
        params![platform,start,end,model], |r| Ok((r.get(0)?,r.get(1)?)))?;
    let context: Option<(f64,i64)> = conn.query_row("SELECT ratio,ts FROM (
        SELECT m.context_ratio AS ratio,r.ts AS ts FROM request_metrics m
        JOIN requests r ON r.id=m.request_id WHERE r.platform=?1 AND r.ts>=?2 AND r.ts<?3 AND m.context_ratio IS NOT NULL
        AND (?4 IS NULL OR r.model=?4)
        UNION ALL SELECT ratio,ts FROM native_context WHERE platform=?1 AND ts>=?2 AND ts<?3 AND (?4 IS NULL OR model=?4)
        ) ORDER BY ts DESC LIMIT 1",
        params![platform,start,end,model], |r| Ok((r.get(0)?,r.get(1)?))).optional()?;
    Ok(Metrics {
        credits,
        credit_requests,
        context_ratio: context.map(|c| c.0),
        context_at_ms: context.map(|c| c.1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(source: &str) -> Record {
        Record {
            request: RequestRecord {
                platform: "workbuddy".into(),
                source: source.into(),
                dedup_key: "r".into(),
                session_id: Some("s".into()),
                ts: 1000,
                model: None,
                input_tokens: Some(1000),
                output_tokens: Some(20),
                cache_read_tokens: Some(800),
                cache_write_tokens: None,
                total_tokens: Some(1020),
                effort: None,
            },
            input_semantics: "includes_cache",
            credits: Some(0.65),
            original_credits: None,
            billable: None,
            context_ratio: None,
            reasoning: None,
        }
    }
    #[test]
    fn snapshots_are_idempotent_and_credits_are_separate() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        let mut record = sample("native:workbuddy:main");
        assert_eq!(write(&conn, &[record.clone()]).unwrap(), 1);
        assert_eq!(write(&conn, &[record.clone()]).unwrap(), 0);
        record.request.total_tokens = Some(1010);
        write(&conn, &[record]).unwrap();
        assert_eq!(
            conn.query_row("SELECT SUM(delta_total) FROM usage_events", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1010
        );
        assert_eq!(
            metrics(&conn, "workbuddy", 0, 2000, None).unwrap().credits,
            Some(0.65)
        );
        write(&conn, &[sample("native:workbuddy:ai")]).unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM requests", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
    }
    #[test]
    fn missing_cache_creation_survives_trend_and_backup_without_becoming_zero() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        write(&conn, &[sample("native:workbuddy:main")]).unwrap();
        let breakdown =
            crate::db::usage_breakdown_at(&conn, "workbuddy", "total", None, None, 2000).unwrap();
        assert_eq!(breakdown.cache_write, None);
        assert_eq!(breakdown.cache_hit_rate, Some(0.8));
        let trend = crate::db::trend_at(&conn, "workbuddy", "total", None, None, 2000).unwrap();
        assert_eq!(trend.points[0].cache_write, None);
        assert_eq!(trend.points[0].tokens, Some(1020));
        let json = crate::db::export_backup(&conn, None).unwrap();
        let mut target = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        assert_eq!(
            crate::db::import_backup(&mut target, &json)
                .unwrap()
                .requests_changed,
            1
        );
        assert_eq!(
            crate::db::import_backup(&mut target, &json)
                .unwrap()
                .requests_changed,
            0
        );
        assert_eq!(
            crate::db::usage_breakdown_at(&target, "workbuddy", "total", None, None, 2000)
                .unwrap()
                .cache_write,
            None
        );
        assert_eq!(
            metrics(&target, "workbuddy", 0, 2000, None)
                .unwrap()
                .credits,
            Some(0.65)
        );
    }
}
