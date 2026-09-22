//! 费用估算。
//!
//! 本地日志没有账单金额；这里只对已核验的精确模型 ID 估算。价格均为标准、
//! 非 Batch、每百万文本 Token 的美元价。未知型号、非日期后缀变体和无法判断
//! 缓存写入 TTL 的 Claude 请求返回 `None`，由界面显示“—”。
//!
//! 核验日期：2026-09-16。
//! OpenAI：https://developers.openai.com/api/docs/models/compare
//! Anthropic：https://platform.claude.com/docs/en/about-claude/pricing
//! Anthropic 生效日价目：https://www-cdn.anthropic.com/files/4zrzovbb/website/3684c2faafb97418665782cea0001f439f74b1d2.pdf

#[derive(Clone, Copy)]
struct Price {
    platform: &'static str,
    model: &'static str,
    input: f64,
    cached_input: f64,
    output: f64,
    /// `None` 表示来源日志不足以确定该类别的价格。
    cache_write: Option<f64>,
}

const PRICES: &[Price] = &[
    // Anthropic Claude API，标准 Global。缓存写入分 5m/1h，本地日志没有 TTL。
    Price { platform: "claude", model: "claude-opus-4-8", input: 5.0, cached_input: 0.5, output: 25.0, cache_write: None },
    Price { platform: "claude", model: "claude-opus-4-7", input: 5.0, cached_input: 0.5, output: 25.0, cache_write: None },
    Price { platform: "claude", model: "claude-opus-4-6", input: 5.0, cached_input: 0.5, output: 25.0, cache_write: None },
    Price { platform: "claude", model: "claude-opus-4-5", input: 5.0, cached_input: 0.5, output: 25.0, cache_write: None },
    Price { platform: "claude", model: "claude-sonnet-4-6", input: 3.0, cached_input: 0.3, output: 15.0, cache_write: None },
    Price { platform: "claude", model: "claude-sonnet-4-5", input: 3.0, cached_input: 0.3, output: 15.0, cache_write: None },
    Price { platform: "claude", model: "claude-sonnet-4", input: 3.0, cached_input: 0.3, output: 15.0, cache_write: None },
    Price { platform: "claude", model: "claude-haiku-4-5", input: 1.0, cached_input: 0.1, output: 5.0, cache_write: None },
    Price { platform: "claude", model: "claude-opus-4-1", input: 15.0, cached_input: 1.5, output: 75.0, cache_write: None },
    Price { platform: "claude", model: "claude-opus-4", input: 15.0, cached_input: 1.5, output: 75.0, cache_write: None },
    Price { platform: "claude", model: "claude-3-7-sonnet", input: 3.0, cached_input: 0.3, output: 15.0, cache_write: None },
    Price { platform: "claude", model: "claude-3-5-sonnet", input: 3.0, cached_input: 0.3, output: 15.0, cache_write: None },
    Price { platform: "claude", model: "claude-3-5-haiku", input: 0.8, cached_input: 0.08, output: 4.0, cache_write: None },
    // OpenAI API；cached_input 直接取官方列，不再用统一倍数推算。
    Price { platform: "codex", model: "gpt-6-astra", input: 10.0, cached_input: 1.0, output: 50.0, cache_write: Some(12.5) },
    Price { platform: "codex", model: "gpt-5.6-sol", input: 4.0, cached_input: 0.4, output: 20.0, cache_write: None },
    Price { platform: "codex", model: "gpt-5.6", input: 4.0, cached_input: 0.4, output: 20.0, cache_write: None },
    Price { platform: "codex", model: "gpt-5.6-terra", input: 2.0, cached_input: 0.2, output: 12.0, cache_write: None },
    Price { platform: "codex", model: "gpt-5.6-luna", input: 0.2, cached_input: 0.02, output: 1.2, cache_write: None },
    Price { platform: "codex", model: "gpt-5.3-codex", input: 1.75, cached_input: 0.175, output: 14.0, cache_write: None },
    Price { platform: "codex", model: "gpt-5.2-codex", input: 1.75, cached_input: 0.175, output: 14.0, cache_write: None },
    Price { platform: "codex", model: "gpt-5.2", input: 1.75, cached_input: 0.175, output: 14.0, cache_write: None },
    Price { platform: "codex", model: "gpt-5.1-codex-max", input: 1.25, cached_input: 0.125, output: 10.0, cache_write: None },
    Price { platform: "codex", model: "gpt-5.1-codex", input: 1.25, cached_input: 0.125, output: 10.0, cache_write: None },
    Price { platform: "codex", model: "gpt-5.1", input: 1.25, cached_input: 0.125, output: 10.0, cache_write: None },
    Price { platform: "codex", model: "gpt-5-codex", input: 1.25, cached_input: 0.125, output: 10.0, cache_write: None },
    Price { platform: "codex", model: "gpt-5", input: 1.25, cached_input: 0.125, output: 10.0, cache_write: None },
    Price { platform: "codex", model: "codex-mini-latest", input: 1.5, cached_input: 0.375, output: 6.0, cache_write: None },
    Price { platform: "codex", model: "gpt-4.1", input: 2.0, cached_input: 0.5, output: 8.0, cache_write: None },
    Price { platform: "codex", model: "gpt-4.1-mini", input: 0.4, cached_input: 0.1, output: 1.6, cache_write: None },
    Price { platform: "codex", model: "gpt-4.1-nano", input: 0.1, cached_input: 0.025, output: 0.4, cache_write: None },
    Price { platform: "codex", model: "gpt-4o", input: 2.5, cached_input: 1.25, output: 10.0, cache_write: None },
    Price { platform: "codex", model: "gpt-4o-mini", input: 0.15, cached_input: 0.075, output: 0.6, cache_write: None },
];

fn is_date_snapshot_suffix(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 11
        && bytes[0] == b'-'
        && bytes[1..].iter().enumerate().all(|(index, byte)| {
            if matches!(index, 4 | 7) { *byte == b'-' } else { byte.is_ascii_digit() }
        })
}

fn price_of(platform: Option<&str>, model: &str) -> Option<Price> {
    let model = model.to_ascii_lowercase();
    PRICES.iter().copied().find(|price| {
        platform.is_none_or(|value| value == price.platform)
            && (model == price.model
                || model.strip_prefix(price.model).is_some_and(is_date_snapshot_suffix))
    })
}

/// 单条请求的估算金额（美元）。模型或计价类别不能精确匹配时返回 None。
pub fn estimate(
    platform: Option<&str>,
    model: Option<&str>,
    input: i64,
    output: i64,
    cache_write: i64,
    cache_read: i64,
) -> Option<f64> {
    let price = price_of(platform, model?)?;
    let write_price = if cache_write == 0 { 0.0 } else { price.cache_write? };
    let ordinary_input = if price.platform == "codex" {
        input.saturating_sub(cache_read).saturating_sub(cache_write)
    } else {
        input
    };
    Some((ordinary_input as f64 * price.input
        + output as f64 * price.output
        + cache_write as f64 * write_price
        + cache_read as f64 * price.cached_input) / 1_000_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_input_including_cache_is_not_charged_twice() {
        let value = estimate(Some("codex"), Some("gpt-5"), 1_000, 500, 0, 800).unwrap();
        let expected = (200.0 * 1.25 + 800.0 * 0.125 + 500.0 * 10.0) / 1_000_000.0;
        assert!((value - expected).abs() < 1e-12);
    }

    #[test]
    fn exact_models_and_date_snapshots_match_but_future_variants_do_not() {
        assert!(price_of(Some("codex"), "gpt-5.2-codex").is_some());
        assert!(price_of(Some("codex"), "gpt-4.1-2025-04-14").is_some());
        assert!(price_of(Some("codex"), "gpt-5.2-codex-preview").is_none());
        assert!(price_of(Some("codex"), "gpt-5.20").is_none());
        assert!(price_of(Some("claude"), "claude-opus-4-9").is_none());
    }

    #[test]
    fn claude_cache_write_without_ttl_is_not_guessed() {
        assert!(estimate(Some("claude"), Some("claude-sonnet-4-5"), 1_000, 500, 100, 800).is_none());
        assert!(estimate(Some("claude"), Some("claude-sonnet-4-5"), 1_000, 500, 0, 800).is_some());
    }
}
