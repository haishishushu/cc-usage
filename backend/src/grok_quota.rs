//! Grok（SuperGrok）积分额度查询（cc-switch 对齐，2026-09 源）
//!
//! 端点：POST https://grok.com/grok_api_v2.GrokBuildBilling/GetGrokCreditsConfig
//! gRPC-Web + protobuf（无 .proto 定义，字段启发式扫描）；凭证来自本机 grok CLI
//! （`~/.grok/auth.json`，见 [`crate::creds::local_grok_auth`]）。
//!
//! 纪律：这是**查询语义的 POST**（空 gRPC 帧，不发任何数据）；15 秒超时、禁止重定向。
//! gRPC 状态 16/7（凭据类）→ Unauthorized；4/14 等瞬时 → Failed（文案注明可重试）。
//! 与 cc-switch 的差异：token 已过期时不做「带过期 token 再试一次」的时钟偏差
//! 容错，直接报过期（岛内凭证读取不返回半可用状态，简化且行为可预期）。

use crate::coding_plan::{WCREDITS, W30D, W7D};
use crate::quota::{self, QuotaState, QuotaWindow};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

const GROK_BILLING_ENDPOINT: &str = "https://grok.com/grok_api_v2.GrokBuildBilling/GetGrokCreditsConfig";

/// 查询本机 Grok 订阅额度（Tauri 命令入口的纯函数半区）。
pub(crate) fn local_grok_quota() -> QuotaState {
    match crate::creds::local_grok_auth() {
        Ok(token) => query_grok(&token),
        Err(reason) => QuotaState::Unauthorized { reason },
    }
}

fn query_grok(access_token: &str) -> QuotaState {
    let c = match quota::client() { Ok(c) => c, Err(e) => return QuotaState::Failed { reason: e } };
    // 空 gRPC-web 帧：1 字节 flags + 4 字节大端长度 0
    let resp = match c.post(GROK_BILLING_ENDPOINT)
        .header("Authorization", format!("Bearer {access_token}"))
        .header("Origin", "https://grok.com")
        .header("Referer", "https://grok.com/?_s=usage")
        .header("Accept", "*/*")
        .header("Content-Type", "application/grpc-web+proto")
        .header("x-grpc-web", "1")
        .header("x-user-agent", "connect-es/2.1.1")
        .body(vec![0u8; 5])
        .timeout(std::time::Duration::from_secs(15))
        .send()
    {
        Ok(r) => r,
        Err(e) if e.is_timeout() => return QuotaState::Failed { reason: "Grok 计费查询超时（15 秒）".into() },
        Err(_) => return QuotaState::Failed { reason: "Grok 计费查询失败，请检查网络后刷新".into() },
    };

    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return QuotaState::Unauthorized {
            reason: format!("Grok 凭证被拒（HTTP {status}），请重新 grok login"),
        };
    }

    // gRPC 错误可能在 HTTP 头里携带（trailers-only 响应），先于响应体检查
    let header_status = resp.headers().get("grpc-status")
        .and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<i64>().ok());
    let header_message = resp.headers().get("grpc-message")
        .and_then(|v| v.to_str().ok()).map(percent_decode).unwrap_or_default();

    if !status.is_success() {
        // HTTP 408 与 grpc-status 4 同为服务端超时，按瞬时处理
        let raw = resp.text().unwrap_or_default();
        let head: String = raw.chars().take(400).collect();
        return QuotaState::Failed { reason: format!("Grok 计费接口错误（HTTP {status}）: {head}") };
    }

    if let Some(code) = header_status {
        if code != 0 {
            return grpc_status_failure(code, &header_message);
        }
    }

    let raw = match resp.bytes() {
        Ok(b) => b,
        Err(e) => return QuotaState::Failed { reason: format!("读取 Grok 计费响应失败: {e}") },
    };

    let trailers = grpc_web_trailer_fields(&raw);
    if let Some(code) = trailers.get("grpc-status").and_then(|v| v.parse::<i64>().ok()) {
        if code != 0 {
            let message = trailers.get("grpc-message").map(String::as_str).unwrap_or("");
            return grpc_status_failure(code, message);
        }
    }

    let now_secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
    let snapshot = match parse_billing_payload(&raw, now_secs) {
        Ok(s) => s,
        Err(e) => return QuotaState::Failed { reason: format!("Grok 计费响应解析失败: {e}") },
    };

    let (key, name) = tier_window(snapshot.resets_at, now_secs);
    QuotaState::Ok {
        windows: vec![QuotaWindow {
            key: key.into(),
            window_name: name.into(),
            used_percent: Some(snapshot.used_percent.clamp(0.0, 100.0)),
            amount_text: None,
            resets_at: snapshot.resets_at
                .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
                .map(|dt| dt.to_rfc3339()),
        }],
        plan: None,
    }
}

/// 将非 0 的 gRPC 状态映射为岛内五态。瞬时状态（超时/不可用）在 reason 中注明可重试；
/// 凭据类失败归 Unauthorized，其余确定性失败归 Failed。
fn grpc_status_failure(status: i64, message: &str) -> QuotaState {
    if is_grpc_auth_failure(status, message) {
        return QuotaState::Unauthorized {
            reason: format!("Grok 凭证被拒（grpc-status {status}），请重新 grok login"),
        };
    }
    if status == 9 && matches!(message.trim().to_lowercase().as_str(), "no personal team" | "no personal team.") {
        return QuotaState::Failed { reason: "xAI 尚未提供团队主体的用量接口（no personal team）".into() };
    }
    if matches!(status, 4 | 14)
        || (status == 1 && ["timeout", "deadline", "expired"].iter().any(|k| message.to_lowercase().contains(k)))
    {
        return QuotaState::Failed { reason: format!("Grok 计费服务暂时不可用（grpc-status {status}），可稍后重试: {message}") };
    }
    QuotaState::Failed { reason: format!("Grok 计费 RPC 失败（grpc-status {status}）: {message}") }
}

/// 认证类失败（token 无效/过期）的 gRPC 状态判定（移植自 CodexBar 同名启发式）
fn is_grpc_auth_failure(status: i64, message: &str) -> bool {
    if status == 16 { return true; }
    if status != 7 { return false; }
    let lower = message.to_lowercase();
    // 服务端文案连字符与空格两种形态都出现过，都认
    lower.contains("bad-credentials") || lower.contains("bad credentials")
        || lower.contains("unauthenticated")
        || (lower.contains("oauth2") && lower.contains("could not be validated"))
        || (lower.contains("access token")
            && (lower.contains("invalid") || lower.contains("expired") || lower.contains("could not be validated")))
}

/// 按重置时间距今的天数推断窗口（CodexBar `primaryLabel` 的阈值）：
/// 4–12 天 → 周窗口，20–45 天 → 月窗口，其余 → 通用 credit 额度
fn tier_window(resets_at: Option<i64>, now_secs: i64) -> (&'static str, &'static str) {
    if let Some(ts) = resets_at {
        let days = ((ts - now_secs) as f64 / 86_400.0).round() as i64;
        if (4..=12).contains(&days) { return W7D; }
        if (20..=45).contains(&days) { return W30D; }
    }
    WCREDITS
}

/* ───────────── gRPC-web 帧与 protobuf 启发式解析 ───────────── */

/// 解析出的账单快照
struct GrokBillingSnapshot {
    used_percent: f64,
    /// Unix 秒
    resets_at: Option<i64>,
}

/// protobuf 扫描收集到的字段（路径 = 从根到该字段的 field number 链）
#[derive(Default)]
struct ProtobufScan {
    /// (path, float 值, 出现顺序)
    fixed32_fields: Vec<(Vec<u64>, f32, usize)>,
    /// (path, varint 值)
    varint_fields: Vec<(Vec<u64>, u64)>,
}

fn read_varint(bytes: &[u8], index: &mut usize) -> Option<u64> {
    let mut value: u64 = 0;
    let mut shift: u32 = 0;
    while *index < bytes.len() && shift < 64 {
        let byte = bytes[*index];
        *index += 1;
        value |= u64::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
    }
    None
}

/// 递归扫描 protobuf 消息，收集 varint 与 fixed32 字段。
///
/// 无 .proto 定义，length-delimited 字段一律当嵌套消息试扫（深度 ≤4）；
/// 无法解析的字节从字段起点 +1 重新同步。返回下一个 fixed32 序号。
fn scan_protobuf(bytes: &[u8], depth: usize, path: &[u64], order: usize, scan: &mut ProtobufScan) -> usize {
    let mut index = 0;
    let mut next_order = order;

    while index < bytes.len() {
        let field_start = index;
        let key = match read_varint(bytes, &mut index) {
            Some(k) if k != 0 => k,
            _ => {
                index = field_start + 1;
                continue;
            }
        };
        let field_number = key >> 3;
        let wire_type = key & 0x07;
        let mut field_path = path.to_vec();
        field_path.push(field_number);

        match wire_type {
            0 => match read_varint(bytes, &mut index) {
                Some(value) => scan.varint_fields.push((field_path, value)),
                None => index = field_start + 1,
            },
            1 => {
                if index + 8 > bytes.len() {
                    return next_order;
                }
                index += 8;
            }
            2 => {
                let length = match read_varint(bytes, &mut index) {
                    Some(l) if l <= (bytes.len() - index) as u64 => l as usize,
                    _ => {
                        index = field_start + 1;
                        continue;
                    }
                };
                let end = index + length;
                if depth < 4 {
                    next_order = scan_protobuf(&bytes[index..end], depth + 1, &field_path, next_order, scan);
                }
                index = end;
            }
            5 => {
                if index + 4 > bytes.len() {
                    return next_order;
                }
                let bits = u32::from_le_bytes([bytes[index], bytes[index + 1], bytes[index + 2], bytes[index + 3]]);
                scan.fixed32_fields.push((field_path, f32::from_bits(bits), next_order));
                next_order += 1;
                index += 4;
            }
            _ => index = field_start + 1,
        }
    }

    next_order
}

/// 拆出 gRPC-web data 帧（flags 高位 0x80 的 trailer 帧跳过）。
/// 任一帧长度非法时返回空——调用方再按裸 protobuf 兜底。
fn grpc_web_data_frames(data: &[u8]) -> Vec<&[u8]> {
    let mut frames = Vec::new();
    let mut index = 0;
    while index < data.len() {
        if index + 5 > data.len() {
            return Vec::new();
        }
        let flags = data[index];
        let length = u32::from_be_bytes([data[index + 1], data[index + 2], data[index + 3], data[index + 4]]) as usize;
        let start = index + 5;
        let end = start + length;
        if end > data.len() {
            return Vec::new();
        }
        if flags & 0x80 == 0 {
            frames.push(&data[start..end]);
        }
        index = end;
    }
    frames
}

/// 响应体没有帧头时，看首字节是否像合法 protobuf tag（某些成功请求直接返回裸 protobuf）
fn looks_like_protobuf_payload(data: &[u8]) -> bool {
    match data.first() {
        Some(&first) => {
            let field_number = first >> 3;
            let wire_type = first & 0x07;
            field_number > 0 && matches!(wire_type, 0 | 1 | 2 | 5)
        }
        None => false,
    }
}

/// 从 trailer 帧（flags & 0x80）解析 `grpc-status` / `grpc-message` 等字段
fn grpc_web_trailer_fields(data: &[u8]) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    let mut index = 0;
    while index + 5 <= data.len() {
        let flags = data[index];
        let length = u32::from_be_bytes([data[index + 1], data[index + 2], data[index + 3], data[index + 4]]) as usize;
        let start = index + 5;
        let end = start + length;
        if end > data.len() {
            break;
        }
        if flags & 0x80 != 0 {
            if let Ok(text) = std::str::from_utf8(&data[start..end]) {
                for line in text.lines().filter(|l| !l.is_empty()) {
                    if let Some((key, value)) = line.split_once(':') {
                        fields.insert(key.trim().to_lowercase(), percent_decode(value.trim()));
                    }
                }
            }
        }
        index = end;
    }
    fields
}

/// gRPC message 使用 percent-encoding；解码失败的序列原样保留
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            // 只切字节切片再校验 UTF-8：对 &str 按字节切片会在多字节字符
            // 边界内 panic（trailer 内容由服务端控制，可含任意 UTF-8）
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(b) = u8::from_str_radix(hex, 16) {
                    out.push(b);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 从响应体提取已用百分比与重置时间（CodexBar `parseGRPCWebResponse` 的移植）。
///
/// 启发式：
/// - 百分比：wire-type 5 (float) 中路径末段为 1、值域 [0,100] 的字段，取路径最浅、出现最早的；
/// - 重置时间：varint 中值落在合理 Unix 秒区间且晚于当前时刻的字段，优先精确路径 [1,5,1]，
///   否则取最近的未来时间；
/// - 零用量特判：proto3 会省略值为 0 的 percent 字段，此时若存在重置时间和用量周期
///   标记（路径 [1,6,*] 或 [1,8,1]=1/2），按 0% 处理。
fn parse_billing_payload(data: &[u8], now_secs: i64) -> Result<GrokBillingSnapshot, String> {
    let mut payloads = grpc_web_data_frames(data);
    if payloads.is_empty() && looks_like_protobuf_payload(data) {
        payloads = vec![data];
    }
    if payloads.is_empty() {
        return Err("响应中没有 protobuf 载荷".into());
    }

    let mut scan = ProtobufScan::default();
    for payload in payloads {
        // fixed32 序号在每个顶层 data 帧内独立从 0 计数
        scan_protobuf(payload, 0, &[], 0, &mut scan);
    }

    let parsed_percent = scan
        .fixed32_fields
        .iter()
        .filter(|(path, value, _)| path.last() == Some(&1) && value.is_finite() && *value >= 0.0 && *value <= 100.0)
        .min_by_key(|(path, _, order)| (path.len(), *order))
        .map(|(_, value, _)| f64::from(*value));

    let reset_candidates: Vec<(&[u64], i64)> = scan
        .varint_fields
        .iter()
        .filter(|(_, value)| (1_700_000_000..=2_100_000_000).contains(value))
        .map(|(path, value)| (path.as_slice(), *value as i64))
        .filter(|(_, ts)| *ts > now_secs)
        .collect();
    let reset = reset_candidates
        .iter()
        .filter(|(path, _)| *path == [1, 5, 1])
        .map(|(_, ts)| *ts)
        .min()
        .or_else(|| reset_candidates.iter().map(|(_, ts)| *ts).min());

    let has_usage_period = scan
        .varint_fields
        .iter()
        .any(|(path, value)| path.starts_with(&[1, 6]) || (path.as_slice() == [1, 8, 1] && (*value == 1 || *value == 2)));
    let no_usage_yet = parsed_percent.is_none()
        && scan.fixed32_fields.is_empty()
        && reset.is_some()
        && has_usage_period;

    let used_percent = match parsed_percent.or(if no_usage_yet { Some(0.0) } else { None }) {
        Some(p) => p,
        None => return Err("无法在 Grok 计费响应中定位用量百分比".into()),
    };

    Ok(GrokBillingSnapshot { used_percent, resets_at: reset })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── protobuf 构造辅助（移植自 cc-switch subscription_grok tests）──

    fn varint(mut value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                break;
            }
            out.push(byte | 0x80);
        }
        out
    }

    fn field_varint(number: u64, value: u64) -> Vec<u8> {
        let mut out = varint(number << 3);
        out.extend(varint(value));
        out
    }

    fn field_float(number: u64, value: f32) -> Vec<u8> {
        let mut out = varint((number << 3) | 5);
        out.extend(value.to_bits().to_le_bytes());
        out
    }

    fn field_message(number: u64, payload: &[u8]) -> Vec<u8> {
        let mut out = varint((number << 3) | 2);
        out.extend(varint(payload.len() as u64));
        out.extend(payload);
        out
    }

    fn grpc_web_frame(flags: u8, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![flags];
        out.extend((payload.len() as u32).to_be_bytes());
        out.extend(payload);
        out
    }

    const NOW: i64 = 1_750_000_000;

    #[test]
    fn parses_percent_and_reset_from_framed_payload() {
        // message { 1: { 1: 37.5f, 5: { 1: reset_ts } } }
        let reset_ts = (NOW + 30 * 86_400) as u64;
        let inner = [field_float(1, 37.5), field_message(5, &field_varint(1, reset_ts))].concat();
        let payload = field_message(1, &inner);
        let data = grpc_web_frame(0, &payload);

        let snapshot = parse_billing_payload(&data, NOW).expect("parse ok");
        assert_eq!(snapshot.used_percent, 37.5);
        assert_eq!(snapshot.resets_at, Some(reset_ts as i64));
    }

    #[test]
    fn parses_bare_protobuf_without_frame_header() {
        let payload = field_message(1, &field_float(1, 12.0));
        let snapshot = parse_billing_payload(&payload, NOW).expect("parse ok");
        assert_eq!(snapshot.used_percent, 12.0);
        assert_eq!(snapshot.resets_at, None);
    }

    #[test]
    fn prefers_shallowest_percent_candidate() {
        // 深层 [1,2,1]=99.0 不应盖过浅层 [1,1]=25.0
        let inner = [field_message(2, &field_float(1, 99.0)), field_float(1, 25.0)].concat();
        let payload = field_message(1, &inner);
        let data = grpc_web_frame(0, &payload);

        let snapshot = parse_billing_payload(&data, NOW).expect("parse ok");
        assert_eq!(snapshot.used_percent, 25.0);
    }

    #[test]
    fn zero_usage_period_without_percent_field_reads_as_zero() {
        // proto3 省略 0 值 percent：仅有 [1,5,1] 重置时间 + [1,6,1] 周期标记
        let reset_ts = (NOW + 7 * 86_400) as u64;
        let inner = [field_message(5, &field_varint(1, reset_ts)), field_message(6, &field_varint(1, 3))].concat();
        let payload = field_message(1, &inner);
        let data = grpc_web_frame(0, &payload);

        let snapshot = parse_billing_payload(&data, NOW).expect("parse ok");
        assert_eq!(snapshot.used_percent, 0.0);
        assert_eq!(snapshot.resets_at, Some(reset_ts as i64));
    }

    #[test]
    fn reset_distance_maps_to_window_tier() {
        assert_eq!(tier_window(Some(NOW + 7 * 86_400), NOW), W7D);
        assert_eq!(tier_window(Some(NOW + 30 * 86_400), NOW), W30D);
        assert_eq!(tier_window(Some(NOW + 86_400), NOW), WCREDITS);
        assert_eq!(tier_window(None, NOW), WCREDITS);
    }

    #[test]
    fn grpc_auth_and_transient_statuses_are_distinguished() {
        assert!(matches!(grpc_status_failure(16, "unauthenticated"), QuotaState::Unauthorized { .. }));
        assert!(matches!(grpc_status_failure(7, "Bad credentials"), QuotaState::Unauthorized { .. }));
        assert!(matches!(grpc_status_failure(4, "deadline exceeded"), QuotaState::Failed { reason } if reason.contains("重试")));
        assert!(matches!(grpc_status_failure(9, "no personal team"), QuotaState::Failed { .. }));
        assert!(matches!(grpc_status_failure(2, "boom"), QuotaState::Failed { .. }));
    }
}
