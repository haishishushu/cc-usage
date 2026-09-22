//! 原生应用白名单用量字段。只保存统计，不复制提示词、工具参数或凭证。
use crate::{db::RequestRecord, source_store::Record};
use serde_json::Value;

pub fn num(v: &Value, key: &str) -> Option<i64> {
    v.get(key)?.as_i64().filter(|n| *n >= 0)
}
pub fn amount(v: &Value, key: &str) -> Option<f64> {
    v.get(key)?.as_f64().filter(|n| n.is_finite() && *n >= 0.0)
}
pub fn text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key)?.as_str().filter(|s| !s.is_empty())
}
pub fn timestamp(v: &Value) -> Option<i64> {
    v.as_i64().filter(|n| *n > 0).or_else(|| {
        chrono::DateTime::parse_from_rfc3339(v.as_str()?)
            .ok()
            .map(|d| d.timestamp_millis())
    })
}

fn base(platform: &str, source: &str, session: &str, id: &str, ts: i64) -> Record {
    Record {
        request: RequestRecord {
            platform: platform.into(),
            source: source.into(),
            dedup_key: format!("{session}:{id}"),
            session_id: Some(format!("{source}:{session}")),
            ts,
            model: None,
            input_tokens: None,
            output_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            total_tokens: None,
            effort: None,
        },
        input_semantics: "unknown",
        credits: None,
        original_credits: None,
        billable: None,
        context_ratio: None,
        reasoning: None,
    }
}

pub fn workbuddy(v: &Value, source: &str, fallback_session: &str) -> Option<Record> {
    let pd = v.get("providerData")?;
    let u = pd.get("usage").unwrap_or(&Value::Null);
    let raw = pd.get("rawUsage").unwrap_or(&Value::Null);
    if !u.is_object() && !raw.is_object() {
        return None;
    }
    let id = text(v, "id").or_else(|| text(v, "messageId"))?;
    let session = text(v, "sessionId").unwrap_or(fallback_session);
    let mut item = base(
        "workbuddy",
        source,
        session,
        id,
        timestamp(v.get("timestamp")?)?,
    );
    item.request.model = text(pd, "requestModelId")
        .or_else(|| text(pd, "model"))
        .map(str::to_string);
    item.request.input_tokens = num(u, "inputTokens").or_else(|| num(raw, "prompt_tokens"));
    item.request.output_tokens = num(u, "outputTokens").or_else(|| num(raw, "completion_tokens"));
    item.request.total_tokens = num(u, "totalTokens").or_else(|| num(raw, "total_tokens"));
    let details = raw.get("prompt_tokens_details").unwrap_or(&Value::Null);
    item.request.cache_read_tokens =
        num(details, "cached_tokens").or_else(|| num(raw, "prompt_cache_hit_tokens"));
    if item.request.cache_read_tokens.is_none() {
        item.request.cache_read_tokens = u
            .get("inputTokensDetails")
            .and_then(Value::as_array)
            .and_then(|a| a.iter().find_map(|v| num(v, "cached_tokens")));
    }
    // prompt_tokens（含缓存）的原始形状已确认；不将其他供应商形状套进此口径。
    if raw.get("prompt_tokens").is_some() {
        item.input_semantics = "includes_cache";
    }
    item.request.cache_write_tokens =
        num(raw, "prompt_cache_write_tokens").or_else(|| num(raw, "cache_creation_input_tokens"));
    item.reasoning = raw
        .get("completion_tokens_details")
        .and_then(|v| num(v, "reasoning_tokens"))
        .or_else(|| num(raw, "completion_thinking_tokens"));
    item.credits = amount(raw, "credit");
    Some(item)
}

pub fn qoder(
    v: &Value,
    source: &str,
    fallback_session: &str,
    token_counts_available: Option<bool>,
) -> Option<Record> {
    if text(v, "type") != Some("assistant")
        || v.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true)
    {
        return None;
    }
    let msg = v.get("message")?;
    let u = msg.get("usage")?;
    let id = text(u, "request_id")
        .or_else(|| text(msg, "id"))
        .or_else(|| text(v, "uuid"))?;
    let session = text(v, "sessionId").unwrap_or(fallback_session);
    let mut item = base(
        "qoder",
        source,
        session,
        id,
        timestamp(v.get("timestamp")?)?,
    );
    item.request.model = text(msg, "model").map(str::to_string);
    // 可用性明确为 false 时零值是占位，非零历史实测值仍保留。
    let has_positive = [
        "input_tokens",
        "output_tokens",
        "cache_read_input_tokens",
        "cache_creation_input_tokens",
    ]
    .iter()
    .any(|key| num(u, key).is_some_and(|n| n > 0));
    let count = |key| {
        num(u, key).filter(|n| {
            *n > 0
                || token_counts_available == Some(true)
                || (token_counts_available.is_none() && has_positive)
        })
    };
    item.request.input_tokens = count("input_tokens");
    item.request.output_tokens = count("output_tokens");
    item.request.cache_read_tokens = count("cache_read_input_tokens");
    item.request.cache_write_tokens = count("cache_creation_input_tokens");
    item.request.total_tokens = count("total_tokens").or_else(|| {
        item.request
            .input_tokens
            .zip(item.request.output_tokens)
            .zip(item.request.cache_read_tokens)
            .zip(item.request.cache_write_tokens)
            .map(|(((i, o), r), w)| i + o + r + w)
    });
    item.input_semantics = "excludes_cache";
    item.credits = amount(u, "credits");
    item.original_credits = amount(u, "original_credits");
    item.billable = u.get("billable").and_then(Value::as_bool);
    item.context_ratio = amount(u, "context_usage_ratio").filter(|r| *r <= 1.0);
    Some(item)
}

pub fn gemini_message(v: &Value, source: &str, session: &str) -> Option<Record> {
    if text(v, "type") != Some("gemini") {
        return None;
    }
    let u = v.get("tokens")?;
    let mut item = base(
        "gemini",
        source,
        session,
        text(v, "id")?,
        timestamp(v.get("timestamp")?)?,
    );
    item.input_semantics = "includes_cache";
    item.request.model = text(v, "model").map(str::to_string);
    item.request.input_tokens = num(u, "input");
    item.request.output_tokens = num(u, "output");
    item.request.cache_read_tokens = num(u, "cached");
    item.request.total_tokens = num(u, "total");
    item.reasoning = num(u, "thoughts");
    Some(item)
}

/// 重建官方留存会话。整条消息更新替换；checkpoint / rewind 不当作额外请求。
pub fn gemini_records(lines: &[Value], source: &str) -> Vec<Record> {
    let mut session = String::new();
    let mut messages: Vec<Value> = Vec::new();
    for line in lines {
        if let Some(id) = text(line, "sessionId") {
            session = id.into();
        }
        if let Some(all) = line.get("messages").and_then(Value::as_array) {
            messages = all.clone();
            continue;
        }
        if let Some(set) = line.get("$set") {
            if let Some(id) = text(set, "sessionId") {
                session = id.into();
            }
            if let Some(all) = set.get("messages").and_then(Value::as_array) {
                messages = all.clone();
            }
            continue;
        }
        if let Some(target) = text(line, "$rewindTo") {
            let index = messages
                .iter()
                .position(|m| text(m, "id") == Some(target))
                .unwrap_or(0);
            messages.truncate(index);
            continue;
        }
        if let Some(id) = text(line, "id") {
            if let Some(index) = messages.iter().position(|m| text(m, "id") == Some(id)) {
                messages[index] = line.clone();
            } else {
                messages.push(line.clone());
            }
        }
    }
    if session.is_empty() {
        return vec![];
    }
    messages
        .iter()
        .filter_map(|m| gemini_message(m, source, &session))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn workbuddy_function_usage_uses_actual_cached_tokens_and_keeps_credits() {
        let v = json!({"id":"m","sessionId":"s","timestamp":1000,"type":"function_call","providerData":{
            "usage":{"inputTokens":1000,"outputTokens":20,"totalTokens":1020},
            "rawUsage":{"prompt_tokens":1000,"prompt_tokens_details":{"cached_tokens":800},"cache_read_input_tokens":0,"credit":0.65}}});
        let r = workbuddy(&v, "native:workbuddy:main", "s").unwrap();
        assert_eq!(r.request.cache_read_tokens, Some(800));
        assert_eq!(r.request.cache_write_tokens, None);
        assert_eq!(r.request.total_tokens, Some(1020));
        assert_eq!(r.credits, Some(0.65));
    }
    #[test]
    fn qoder_unavailable_zero_is_unknown_not_free_usage() {
        let v = json!({"type":"assistant","timestamp":1000,"message":{"id":"m","usage":{
            "input_tokens":0,"output_tokens":0,"cache_read_input_tokens":0,"cache_creation_input_tokens":0,"credits":0.65,"billable":false}}});
        let r = qoder(&v, "native:qoder:cn", "s", Some(false)).unwrap();
        assert_eq!(r.request.total_tokens, None);
        assert_eq!(r.credits, Some(0.65));
        assert_eq!(r.billable, Some(false));
        assert_eq!(
            qoder(&v, "native:qoder:cn", "s", Some(true))
                .unwrap()
                .request
                .total_tokens,
            Some(0)
        );
    }
    #[test]
    fn gemini_updates_rewind_and_total_do_not_double_count() {
        let mut m = json!({"type":"gemini","id":"m","timestamp":1000,"tokens":{"input":1000,"output":20,"cached":800,"thoughts":10,"total":1030}});
        let mut lines = vec![json!({"sessionId":"s"}), m.clone()];
        m["tokens"]["total"] = json!(1040);
        lines.push(m);
        let r = gemini_records(&lines, "native:gemini:cli");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].request.total_tokens, Some(1040));
        assert_eq!(r[0].request.cache_write_tokens, None);
        lines.push(json!({"$rewindTo":"m"}));
        assert!(gemini_records(&lines, "native:gemini:cli").is_empty());
    }
}
