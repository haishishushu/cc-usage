//! 模型 → 思考强度档位（画布 18 后续，鼠鼠需求：档位按模型对应、可获取可持久化）。
//!
//! 官方 API 不暴露模型支持的档位，且档位本身随版本演进（2026-09 查证：
//! Claude 已有 low/medium/high/xhigh/max，OpenAI 还有 none/minimal），
//! 因此档位做成**数据**而不是代码：
//!   - `effort_levels` 表：档位字典（全局），启动内置已知档位，获取到新档位追加
//!   - `effort_model_rules` 表：模型匹配规则 → 档位（内置初始规则来自官方文档查证）
//!   - 「获取强度」按钮：拉取鼠鼠仓库的 `effort-map.json` 远程表，合并进两张表，
//!     官方加新档位时鼠鼠改远程文件、用户点一下按钮即可，无需发版
//!
//! 远程表格式（与上面两表对应，字段缺失时保留现状）：
//! ```json
//! {
//!   "updated": "2026-09-21",
//!   "levels": ["off", "none", "minimal", "low", "medium", "high", "xhigh", "max"],
//!   "rules": [{ "match": "claude-opus-4*", "efforts": ["low", "medium", "high", "xhigh", "max"] }]
//! }
//! ```
//! `match` 支持 `*` 结尾的前缀通配，大小写不敏感；按顺序先匹配先用。

use rusqlite::{params, Connection as Sql};

/// 档位展示顺序：关闭/无思考在前，思考越深越靠后；未知新档位追加在尾部。
pub const LEVEL_ORDER: &[&str] = &["off", "none", "minimal", "low", "medium", "high", "xhigh", "max"];

/// 内置初始规则（2026-09 官方文档查证；后续以远程表为准）。
const SEED_RULES: &[(&str, &[&str])] = &[
    ("claude-opus-4*", &["low", "medium", "high", "xhigh", "max"]),
    ("claude-sonnet-4*", &["low", "medium", "high"]),
    ("claude-haiku*", &["off"]),
    ("gpt-5*", &["none", "minimal", "low", "medium", "high", "xhigh", "max"]),
    ("codex-*", &["low", "medium", "high"]),
    ("grok-*", &["low", "medium", "high"]),
];

/// 建表 + 内置数据 seed（幂等：INSERT OR IGNORE，历史数据不动）。
pub fn seed(conn: &Sql) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS effort_levels (
             name TEXT PRIMARY KEY,
             ord  INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS effort_model_rules (
             pattern TEXT PRIMARY KEY,
             efforts TEXT NOT NULL
         );",
    )?;
    for (index, name) in LEVEL_ORDER.iter().enumerate() {
        conn.execute(
            "INSERT OR IGNORE INTO effort_levels(name, ord) VALUES (?1, ?2)",
            params![name, (index + 1) * 10],
        )?;
    }
    for (pattern, efforts) in SEED_RULES {
        let json = serde_json::to_string(efforts).expect("内置规则序列化不会失败");
        conn.execute(
            "INSERT OR IGNORE INTO effort_model_rules(pattern, efforts) VALUES (?1, ?2)",
            params![pattern, json],
        )?;
    }
    Ok(())
}

/// 模型名 → 支持的档位（按 ord 排序）。无规则命中时回落全部已知档位，
/// 不让手动输入的私有模型名卡死在空下拉。
pub fn efforts_for_model(conn: &Sql, model: &str) -> Vec<String> {
    let model = model.trim().to_ascii_lowercase();
    let mut stmt = match conn.prepare("SELECT pattern, efforts FROM effort_model_rules") {
        Ok(stmt) => stmt,
        Err(_) => return all_levels(conn),
    };
    let rules: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map(|rows| rows.collect::<Result<_, _>>().unwrap_or_default())
        .unwrap_or_default();
    for (pattern, efforts) in &rules {
        if matches(pattern, &model) {
            if let Ok(list) = serde_json::from_str::<Vec<String>>(efforts) {
                if !list.is_empty() {
                    return list;
                }
            }
        }
    }
    all_levels(conn)
}

fn all_levels(conn: &Sql) -> Vec<String> {
    let mut stmt = match conn.prepare("SELECT name FROM effort_levels ORDER BY ord") {
        Ok(stmt) => stmt,
        Err(_) => return default_efforts(),
    };
    stmt.query_map([], |r| r.get(0))
        .map(|rows| rows.collect::<Result<Vec<_>, _>>().unwrap_or_default())
        .unwrap_or_default()
}

fn default_efforts() -> Vec<String> {
    LEVEL_ORDER.iter().map(|s| s.to_string()).collect()
}

fn matches(pattern: &str, model: &str) -> bool {
    let pattern = pattern.trim().to_ascii_lowercase();
    match pattern.strip_suffix('*') {
        Some(prefix) => model.starts_with(prefix),
        None => model == pattern,
    }
}

/* ─────────────── 官方接口获取（画布 18，鼠鼠需求的主路径） ─────────────── */

/// Anthropic Models API 的模型条目：id + 能力元数据。
#[derive(Debug, serde::Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<ModelEntry>,
}

#[derive(Debug, serde::Deserialize)]
struct ModelEntry {
    id: String,
    #[serde(default)]
    capabilities: Option<ModelCapabilities>,
}

#[derive(Debug, serde::Deserialize)]
struct ModelCapabilities {
    #[serde(default)]
    effort: Option<EffortCapability>,
}

#[derive(Debug, serde::Deserialize)]
struct EffortCapability {
    #[serde(default)]
    supported: bool,
    #[serde(default)]
    low: Option<LevelFlag>,
    #[serde(default)]
    medium: Option<LevelFlag>,
    #[serde(default)]
    high: Option<LevelFlag>,
    #[serde(default)]
    xhigh: Option<LevelFlag>,
    #[serde(default)]
    max: Option<LevelFlag>,
}

#[derive(Debug, serde::Deserialize)]
struct LevelFlag {
    #[serde(default)]
    supported: bool,
}

/// 「获取强度」主路径：直接调官方 `/v1/models`（用连接的地址与 Key 鉴权），
/// 解析每个模型的 `capabilities.effort`，返回 (模型 id, 支持的档位) 列表。
/// 网关不透传 capabilities 时该模型被跳过——不编造档位。
pub fn fetch_model_efforts(
    platform: &str,
    base_url: &str,
    secret: &str,
) -> Result<Vec<(String, Vec<String>)>, String> {
    let url = crate::connections::models_endpoint(base_url, None)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = crate::connections::apply_auth(client.get(&url), platform, secret)
        .send()
        .map_err(|e| format!("无法连接网关：{e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("获取强度失败：HTTP {status}，请检查 Key 与地址"));
    }
    let body: ModelsResponse = resp.json().map_err(|e| format!("响应解析失败：{e}"))?;

    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    for entry in body.data {
        let Some(effort) = entry.capabilities.and_then(|c| c.effort) else { continue };
        let mut levels: Vec<String> = Vec::new();
        for (name, flag) in [
            ("low", effort.low),
            ("medium", effort.medium),
            ("high", effort.high),
            ("xhigh", effort.xhigh),
            ("max", effort.max),
        ] {
            if flag.map(|f| f.supported).unwrap_or(false) {
                levels.push(name.to_string());
            }
        }
        // 官方支持 thinking 但未标注分档时，至少保留通用三档
        if levels.is_empty() && effort.supported {
            levels = ["low".to_string(), "medium".to_string(), "high".to_string()].to_vec();
        }
        if !levels.is_empty() {
            entries.push((entry.id, levels));
        }
    }
    if entries.is_empty() {
        return Err("网关未返回思考强度元数据（capabilities.effort），无法自动获取；请改用手动对照表".into());
    }
    Ok(entries)
}

/// 官方获取结果落库：档位补进 effort_levels（新档位追加），模型规则逐条 upsert
/// （精确模型 id 匹配；不删除其他来源的规则——官方拉取只增不删）。
pub fn upsert_model_levels(conn: &Sql, entries: &[(String, Vec<String>)]) -> Result<(), String> {
    for (model, efforts) in entries {
        for name in efforts {
            let exists: Option<i64> = conn
                .query_row("SELECT ord FROM effort_levels WHERE name = ?1", params![name], |r| r.get(0))
                .ok();
            if exists.is_none() {
                let max_ord: i64 = conn
                    .query_row("SELECT COALESCE(MAX(ord), 0) FROM effort_levels", [], |r| r.get(0))
                    .map_err(|e| e.to_string())?;
                conn.execute(
                    "INSERT INTO effort_levels(name, ord) VALUES (?1, ?2)",
                    params![name, max_ord + 10],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        let json = serde_json::to_string(efforts).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO effort_model_rules(pattern, efforts) VALUES (?1, ?2)",
            params![model, json],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        seed(&conn).unwrap();
        conn
    }

    #[test]
    fn seed_provides_known_levels_and_rules() {
        let conn = db();
        assert!(efforts_for_model(&conn, "claude-opus-4-8").contains(&"xhigh".to_string()));
        assert_eq!(efforts_for_model(&conn, "claude-haiku-4-5"), vec!["off".to_string()]);
        assert!(efforts_for_model(&conn, "grok-4.6").contains(&"high".to_string()));
    }

    #[test]
    fn unknown_model_falls_back_to_all_levels() {
        let conn = db();
        let all = efforts_for_model(&conn, "my-private-gateway-model");
        assert!(all.contains(&"low".to_string()) && all.contains(&"xhigh".to_string()));
    }

    #[test]
    fn matches_is_case_insensitive_with_trailing_wildcard() {
        assert!(matches("GPT-5*", "gpt-5.2-turbo"));
        assert!(matches("claude-opus-4-1", "claude-opus-4-1"));
        assert!(!matches("grok-*", "claude-sonnet-4-5"));
    }


}
