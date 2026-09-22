//! 分项统计、模型筛选与按桶计价的回归测试。
//!
//! 单独成文件是因为这批断言围绕同一组「口径」展开——跨平台归一化、未知与真实零的
//! 区分、费用不重复扣减——放在一起比散进 db.rs 那个已经很长的测试模块更好读。

use super::*;

/// 构造一条记录；四个 Token 字段用 Option 表达「来源是否提供了这个字段」
#[allow(clippy::too_many_arguments)]
fn rec(
    platform: &str,
    key: &str,
    ts: i64,
    model: Option<&str>,
    input: Option<i64>,
    output: Option<i64>,
    cache_write: Option<i64>,
    cache_read: Option<i64>,
    total: Option<i64>,
) -> RequestRecord {
    RequestRecord {
        platform: platform.into(),
        source: "local".into(),
        dedup_key: key.into(),
        session_id: None,
        ts,
        model: model.map(String::from),
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: cache_read,
        cache_write_tokens: cache_write,
        total_tokens: total,
        effort: None,
    }
}

fn seeded(records: &[RequestRecord]) -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    init(&conn).unwrap();
    insert_records(&mut conn, records).unwrap();
    conn
}

/// Codex 的 input_tokens 含缓存重读，必须扣掉才是「新增输入」；Claude 的不含，直接用。
/// 两条记录放在一起，验证按行判平台而不是一刀切。
#[test]
fn fresh_input_subtracts_cached_only_for_codex() {
    let conn = seeded(&[
        rec("codex", "c1", 1_000, Some("gpt-5"), Some(1_000), Some(200), Some(0), Some(800), Some(1_200)),
        rec("claude", "a1", 1_000, Some("claude-sonnet-4"), Some(1_000), Some(200), Some(0), Some(800), Some(2_000)),
    ]);

    let codex = usage_breakdown_at(&conn, "codex", "total", None, None, 2_000).unwrap();
    assert_eq!(codex.fresh_input, Some(200), "Codex 应扣除 800 缓存重读");
    assert_eq!(codex.cache_read, Some(800));

    let claude = usage_breakdown_at(&conn, "claude", "total", None, None, 2_000).unwrap();
    assert_eq!(claude.fresh_input, Some(1_000), "Claude 的输入本就不含缓存");

    let all = usage_breakdown_at(&conn, "all", "total", None, None, 2_000).unwrap();
    assert_eq!(all.fresh_input, Some(1_200));
    assert_eq!(all.real_total, Some(3_200), "真实消耗直接取 total_tokens 之和");
    assert_eq!(all.requests, 2);
}

/// 缓存重读多于输入时夹到 0，不产生负的「新增输入」
#[test]
fn fresh_input_never_goes_negative() {
    let conn = seeded(&[
        rec("codex", "c1", 1_000, Some("gpt-5"), Some(100), Some(10), Some(0), Some(900), Some(110)),
    ]);
    let breakdown = usage_breakdown_at(&conn, "codex", "total", None, None, 2_000).unwrap();
    assert_eq!(breakdown.fresh_input, Some(0));
}

/// 来源没提供的字段必须是 null，不能当成真实的 0——OpenAI 协议不上报缓存写入，
/// 画成 0 会让用户以为「确实没写过缓存」。
#[test]
fn missing_fields_stay_unknown_and_do_not_become_zero() {
    let conn = seeded(&[
        rec("codex", "c1", 1_000, Some("gpt-5"), Some(500), Some(100), None, Some(200), Some(600)),
    ]);
    let breakdown = usage_breakdown_at(&conn, "codex", "total", None, None, 2_000).unwrap();
    assert_eq!(breakdown.cache_write, None, "缺失的缓存写入保持未知");
    assert_eq!(breakdown.output, Some(100));
    assert_eq!(breakdown.cache_hit_rate, Some(0.4), "输入已含缓存，缺写入不影响缓存命中率分母");
}

/// Codex 的新增输入依赖 input 与 cache_read 两个字段，缺任一个都算未知；
/// Claude 只依赖 input，缺 cache_read 不影响。
#[test]
fn codex_fresh_input_unknown_when_cache_read_missing() {
    let conn = seeded(&[
        rec("codex", "c1", 1_000, Some("gpt-5"), Some(500), Some(100), Some(0), None, Some(600)),
    ]);
    assert_eq!(
        usage_breakdown_at(&conn, "codex", "total", None, None, 2_000).unwrap().fresh_input,
        None,
    );

    let conn = seeded(&[
        rec("claude", "a1", 1_000, Some("claude-sonnet-4"), Some(500), Some(100), Some(0), None, Some(600)),
    ]);
    assert_eq!(
        usage_breakdown_at(&conn, "claude", "total", None, None, 2_000).unwrap().fresh_input,
        Some(500),
    );
}

/// 区间内没有任何记录是真实的零用量，不是未知
#[test]
fn empty_range_reports_real_zero_not_unknown() {
    let conn = seeded(&[]);
    let breakdown = usage_breakdown_at(&conn, "claude", "total", None, None, 2_000).unwrap();
    assert_eq!(
        (
            breakdown.fresh_input,
            breakdown.output,
            breakdown.cache_write,
            breakdown.cache_read,
            breakdown.real_total,
        ),
        (Some(0), Some(0), Some(0), Some(0), Some(0)),
    );
    assert_eq!(breakdown.requests, 0);
    assert_eq!(breakdown.cache_hit_rate, None, "没有可缓存输入时不给出比率");
    assert_eq!(breakdown.avg_per_request, None, "零请求不做除法");
}

/// 命中率分母是「本可以命中的输入」：新增输入 + 缓存写入 + 缓存命中，输出不进分母
#[test]
fn cache_hit_rate_uses_cacheable_input_as_denominator() {
    let conn = seeded(&[
        rec("claude", "a1", 1_000, Some("claude-sonnet-4"), Some(100), Some(999), Some(300), Some(600), Some(1_999)),
    ]);
    let breakdown = usage_breakdown_at(&conn, "claude", "total", None, None, 2_000).unwrap();
    assert_eq!(breakdown.cache_hit_rate, Some(600.0 / 1_000.0));
    assert_eq!(breakdown.avg_per_request, Some(1_999.0));
}

/// 模型筛选要贯穿分项、汇总、趋势与日志——四者口径不一致就会出现
/// 「上面写 1 条、下面列 2 条」这种自相矛盾的页面。
#[test]
fn model_filter_applies_across_queries() {
    let conn = seeded(&[
        rec("claude", "a1", 1_000, Some("claude-sonnet-4"), Some(100), Some(10), Some(0), Some(0), Some(110)),
        rec("claude", "a2", 1_100, Some("claude-opus-4-1"), Some(500), Some(50), Some(0), Some(0), Some(550)),
    ]);
    let only = Some("claude-sonnet-4");

    let breakdown = usage_breakdown_at(&conn, "claude", "total", None, only, 2_000).unwrap();
    assert_eq!((breakdown.requests, breakdown.real_total), (1, Some(110)));

    assert_eq!(token_totals_at(&conn, "claude", None, only, 2_000).unwrap().total, Some(110));
    assert_eq!(request_log_at(&conn, "claude", "total", 1, None, only, 2_000).unwrap().total_count, 1);

    let trend = trend_at(&conn, "claude", "total", None, only, 2_000).unwrap();
    assert_eq!(trend.points.iter().filter_map(|p| p.tokens).sum::<i64>(), 110);

    let all = usage_breakdown_at(&conn, "claude", "total", None, None, 2_000).unwrap();
    assert_eq!((all.requests, all.real_total), (2, Some(660)));
}

/// 模型列表只给真实出现过的模型；NULL 表示「这条记录没说」，不是一个可选项
#[test]
fn model_list_skips_null_models() {
    let conn = seeded(&[
        rec("claude", "a1", 1_000, Some("claude-sonnet-4"), Some(1), Some(1), Some(0), Some(0), Some(2)),
        rec("claude", "a2", 1_100, None, Some(1), Some(1), Some(0), Some(0), Some(2)),
        rec("claude", "a3", 1_200, Some("claude-sonnet-4"), Some(1), Some(1), Some(0), Some(0), Some(2)),
    ]);
    assert_eq!(
        list_models_at(&conn, "claude", "total", None, 2_000).unwrap(),
        vec!["claude-sonnet-4".to_string()],
    );
}

/// 趋势的每个桶都要带齐四个分项，费用按桶单独估算
#[test]
fn trend_points_carry_breakdown_and_per_bucket_cost() {
    let now = Local::now().timestamp_millis();
    let conn = seeded(&[
        rec("codex", "c1", now - 3_600_000, Some("gpt-5"), Some(1_000), Some(500), Some(0), Some(800), Some(1_500)),
    ]);
    let trend = trend_at(&conn, "codex", "today", None, None, now + 1).unwrap();
    let point = trend.points.iter().find(|p| p.tokens.is_some_and(|n| n > 0)).expect("应有一个非空桶");

    assert_eq!(point.fresh_input, Some(200), "趋势里的新增输入同样扣除缓存重读");
    assert_eq!(point.cache_read, Some(800));
    assert_eq!(point.output, Some(500));

    // 计价用原始 input：Codex 的缓存扣除在 pricing::estimate 内部完成，
    // 若这里先减一次就会重复扣减，费用被系统性低估。
    let expected = crate::pricing::estimate(Some("codex"), Some("gpt-5"), 1_000, 500, 0, 800).unwrap();
    assert!((point.cost.unwrap() - expected).abs() < 1e-12);
}

/// 价目表覆盖不到的模型让整桶费用作废，不悄悄按 0 元计入
#[test]
fn unpriced_model_makes_bucket_cost_unknown() {
    let now = Local::now().timestamp_millis();
    let conn = seeded(&[
        rec("claude", "a1", now - 3_600_000, Some("某个未知模型"), Some(10), Some(5), Some(0), Some(0), Some(15)),
    ]);
    let trend = trend_at(&conn, "claude", "today", None, None, now + 1).unwrap();
    let point = trend.points.iter().find(|p| p.tokens.is_some_and(|n| n > 0)).expect("应有一个非空桶");
    assert_eq!(point.cost, None, "无价目的模型不能按 0 元计入");
}

/// 退化行排除（2026-09-19 鼠鼠定版）：已知 Token 字段全为 0 却缺缓存字段的记录
/// （实测来自第三方网关的空响应）不携带任何可统计信息，却会让区间的合计、
/// 命中率与费用整体变「未知」。统计聚合一律跳过它；记录保留在库中，请求日志照常显示。
#[test]
fn degenerate_zero_row_is_excluded_from_stats_but_kept_in_log() {
    let conn = seeded(&[
        rec("claude", "a1", 1_000, Some("claude-sonnet-4"), Some(100), Some(10), Some(0), Some(0), Some(110)),
        // 网关空响应：输入/输出已知为 0，缓存字段缺失，total 无法计算
        rec("claude", "bad", 1_100, Some("glm-5.3-flash"), Some(0), Some(0), None, None, None),
    ]);

    let breakdown = usage_breakdown_at(&conn, "claude", "total", None, None, 2_000).unwrap();
    assert_eq!(breakdown.real_total, Some(110), "退化行不进合计，健康行照常求和");
    assert_eq!(breakdown.requests, 1, "退化行不计请求数");
    assert!(breakdown.cache_hit_rate.is_some(), "命中率不再被退化行拖成未知");

    assert_eq!(
        token_totals_at(&conn, "claude", None, None, 2_000).unwrap().total,
        Some(110),
        "四张周期卡同样排除退化行",
    );

    let cost = cost_estimate_at(&conn, "claude", "total", None, None, 2_000).unwrap();
    assert!(cost.complete, "退化行不再让费用变未知");

    let trend = trend_at(&conn, "claude", "total", None, None, 2_000).unwrap();
    let point = trend.points.iter().find(|p| p.tokens.is_some_and(|n| n > 0)).expect("应有一个非空桶");
    assert!(point.cost.is_some(), "趋势桶费用不再被退化行作废");

    // 记录本身保留：请求日志仍列出退化行，总量按「未知」显示
    let log = request_log_at(&conn, "claude", "total", 1, None, None, 2_000).unwrap();
    assert_eq!(log.total_count, 2, "请求日志保留全部记录，含退化行");
    assert!(log.rows.iter().any(|row| row.total.is_none()), "退化行在日志中总量显示「—」");
}
