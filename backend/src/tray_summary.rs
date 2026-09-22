//! 按需创建的只读托盘摘要；不获取焦点、不穿插额外网络请求。
use std::sync::{Mutex, atomic::{AtomicBool, AtomicU64, Ordering}};
use tauri::{Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "tray-summary";
static GENERATION: AtomicU64 = AtomicU64::new(0);
static ANCHOR: Mutex<Option<(f64, f64)>> = Mutex::new(None);
/// 光标当前是否仍在托盘图标上。建窗在主线程异步完成，离开可能落在
/// 「代数校验之后、窗口注册之前」的空档导致销毁扑空，显示前须再查此标记。
static HOVERING: AtomicBool = AtomicBool::new(false);

#[derive(serde::Serialize)]
pub struct Summary {
    connection: String,
    quotas: Vec<Quota>,
    updated_at: Option<i64>,
    message: String,
}

#[derive(serde::Serialize)]
struct Quota { key: String, value: String }

fn cache_key(settings: &crate::settings::Settings) -> Option<String> {
    settings.island_connection_id.clone().or_else(|| {
        (settings.island_platform == "codex" && settings.island_kind == "auth").then(|| "local-codex".into())
    })
}

pub fn current_state(app: &tauri::AppHandle) -> crate::quota::QuotaState {
    let cfg = app.state::<crate::Cfg>().0.get();
    cache_key(&cfg).and_then(|key| crate::QUOTA_CACHE.state.lock().ok()
        .and_then(|cache| cache.entries.get(&key).map(|entry| entry.0.clone())))
        .unwrap_or(crate::quota::QuotaState::Unsupported { reason: "尚无额度数据".into() })
}

/// 套餐判定只认 5h / 7d（口径同前端 planCoverage）：月额度、总配额是别的形态
fn has_plan_windows(windows: &[crate::quota::QuotaWindow]) -> bool {
    windows.iter().any(|window| window.key == "5h" || window.key == "7d")
}

/// 千分位分隔，与前端 `toLocaleString("en-US")` 一致
fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let bytes = digits.as_bytes();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 && (bytes.len() - index) % 3 == 0 { out.push(','); }
        out.push(*byte as char);
    }
    out
}

/// API Key 且确认无套餐（额度来源没有 5h/7d 窗口，或明确不提供订阅额度）时的摘要：
/// 改显两条「今日Token / 剩余金额」（2026-09-19 鼠鼠定版）。只读现成缓存与本机统计，
/// 不穿插网络请求（模块纪律）。今日Token 优先官方组织用量（仅 Admin Key 可查），
/// 否则回退本机同平台今日（新鲜口径，与灵动岛「本机今日 Token」同源）；
/// 剩余金额来自余额缓存；任一缺失显示「—」不补零。
/// updated_at 取所读缓存写入时刻的最新值。
fn fill_api_key_usage(
    app: &tauri::AppHandle,
    summary: &mut Summary,
    key: &str,
    platform: &str,
    quota_expires: std::time::Instant,
) {
    let mut latest = quota_expires;
    let org_usage = crate::API_USAGE_CACHE.state.lock().ok()
        .and_then(|cache| {
            cache.entries.get(key).and_then(|(value, expires, _)| match value {
                crate::quota::ApiUsageState::Ok { total_tokens, .. } => Some((thousands(*total_tokens), *expires)),
                _ => None,
            })
        });
    let today_text = match org_usage {
        Some((text, expires)) => { if expires > latest { latest = expires; } format!("{text} Token") }
        None => app.try_state::<crate::Db>().and_then(|db| {
            let conn = db.0.lock().ok()?;
            crate::db::today_fresh_tokens(&conn, platform).ok().flatten()
        }).map_or_else(|| "—".into(), |tokens| format!("{} Token", thousands(tokens.max(0) as u64))),
    };
    summary.quotas.push(Quota { key: "今日Token".into(), value: today_text });

    let balance = crate::BALANCE_CACHE.state.lock().ok()
        .and_then(|cache| {
            cache.entries.get(key).and_then(|(value, expires, _)| match value {
                crate::quota::BalanceState::Ok { balance, currency, .. } => Some((format!("{balance:.2} {currency}"), *expires)),
                _ => None,
            })
        });
    summary.quotas.push(Quota {
        key: "剩余金额".into(),
        value: balance.as_ref().map_or_else(|| "—".into(), |(text, _)| text.clone()),
    });
    if let Some((_, expires)) = balance {
        if expires > latest { latest = expires; }
    }
    summary.updated_at = Some(chrono::Utc::now().timestamp_millis() - latest.elapsed().as_millis() as i64);
    summary.message = String::new();
}

#[tauri::command]
pub fn tray_summary_get(app: tauri::AppHandle) -> Summary {
    let cfg = app.state::<crate::Cfg>().0.get();
    let platform = crate::platform_name(&cfg.island_platform);
    let kind = if cfg.island_kind == "auth" { "Auth" } else { "API Key" };
    let name = cfg.island_connection_name.as_deref().unwrap_or(if cache_key(&cfg).as_deref() == Some("local-codex") { "本机授权" } else { "未选择连接" });
    let mut summary = Summary { connection: format!("{platform} · {kind} · {name}"), quotas: vec![], updated_at: None, message: "尚无额度数据".into() };
    if crate::platforms::native(&cfg.island_platform) && cfg.island_platform!="grok" {
        summary.message="本机留存记录；积分非余额，无法按账号区分".into();
        if let Some(db)=app.try_state::<crate::Db>() {
            if let Ok(conn)=db.0.lock() {
                let now=chrono::Utc::now().timestamp_millis();
                let (start,end)=crate::db::period_range_at(&conn,&cfg.island_platform,"today",now);
                let tokens=crate::db::today_fresh_tokens(&conn,&cfg.island_platform).ok().flatten();
                summary.quotas.push(Quota { key:"本机今日 Token".into(),value:tokens.map(|n|thousands(n.max(0) as u64)).unwrap_or_else(||"—".into()) });
                if crate::platforms::supports(&cfg.island_platform,"credits") {
                    let credits=crate::source_store::metrics(&conn,&cfg.island_platform,start,end,None).ok().and_then(|m|m.credits);
                    summary.quotas.push(Quota {key:"今日上报积分".into(),value:credits.map(|n|format!("{n:.4}")).unwrap_or_else(||"—".into())});
                }
            }
        }
        return summary;
    }
    if let Some(key) = cache_key(&cfg) {
        if let Ok(cache) = crate::QUOTA_CACHE.state.lock() {
            if let Some((state, expires, _)) = cache.entries.get(&key) {
                match state {
                    crate::quota::QuotaState::Ok { windows, .. } => {
                        // API Key 且确认无 5h/7d 套餐窗口：改显「今日Token + 剩余金额」
                        //（2026-09-19 鼠鼠定版）。查询失败/凭证失效不算无套餐，仍按原样说明原因。
                        if cfg.island_kind == "api" && !has_plan_windows(windows) {
                            fill_api_key_usage(&app, &mut summary, &key, &cfg.island_platform, *expires);
                        } else {
                            summary.quotas = windows.iter().map(|window| Quota {
                                key: window.key.clone(),
                                value: window.used_percent.filter(|v| v.is_finite()).map(|v| format!("{:.0}%", v.clamp(0.0, 100.0)))
                                    .or_else(|| window.amount_text.clone()).unwrap_or("—".into()),
                            }).collect();
                            // 成功缓存 TTL 固定 5 分钟；由写入时刻计算，命中缓存不伪装为刚更新。
                            let captured = *expires - std::time::Duration::from_secs(300);
                            summary.updated_at = Some(chrono::Utc::now().timestamp_millis() - captured.elapsed().as_millis() as i64);
                            summary.message = if windows.is_empty() { "来源未提供额度窗口" } else { "" }.into();
                        }
                    }
                    crate::quota::QuotaState::Unsupported { .. } if cfg.island_kind == "api" => {
                        // 明确不提供订阅额度 = API Key 无套餐的确定性结论
                        fill_api_key_usage(&app, &mut summary, &key, &cfg.island_platform, *expires);
                    }
                    crate::quota::QuotaState::Unsupported { .. } => summary.message = "此连接不提供订阅额度".into(),
                    crate::quota::QuotaState::Unauthorized { .. } => summary.message = "凭证已失效，请重新授权".into(),
                    crate::quota::QuotaState::Forbidden { .. } => summary.message = "无额度查询权限".into(),
                    crate::quota::QuotaState::RateLimited { .. } => summary.message = "查询限流，稍后重试".into(),
                    crate::quota::QuotaState::Failed { .. } => summary.message = "额度更新失败".into(),
                }
            }
        }
    }
    summary
}

pub fn close(app: &tauri::AppHandle) {
    HOVERING.store(false, Ordering::SeqCst);
    GENERATION.fetch_add(1, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window(LABEL) { let _ = window.destroy(); }
}

/// 光标离锚点多远就判定为已离开图标（物理像素）。
/// 托盘图标在常见缩放下约 16–32 物理像素，取 36 能容下图标内的小幅移动，
/// 又不至于把相邻图标也算作「仍在悬停」。
const LEAVE_RADIUS: f64 = 36.0;
/// 兜底轮询间隔。仅在摘要可见期间运行，开销可忽略。
const WATCH_INTERVAL_MS: u64 = 200;

pub fn enter(app: &tauri::AppHandle, x: f64, y: f64) {
    if app.get_webview_window("context-menu").is_some() { return; }
    HOVERING.store(true, Ordering::SeqCst);
    if let Ok(mut anchor) = ANCHOR.lock() { *anchor = Some((x, y)); }
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(std::time::Duration::from_millis(350));
        let app = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            if GENERATION.load(Ordering::SeqCst) != generation || app.get_webview_window(LABEL).is_some() { return; }
            match WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::App("index.html?window=tray-summary".into()))
                .title("CC Usage Summary").inner_size(254.0, 150.0)
                .decorations(false).transparent(true).shadow(false).resizable(false)
                .skip_taskbar(true).always_on_top(true).visible(false).focused(false).focusable(false)
                .build() {
                Ok(window) => {
                    // 建窗期间可能已收到离开事件，而当时窗口尚未注册、销毁扑空。
                    // 建成后立刻复查代数，过期就当场销毁，不留隐藏孤儿窗口。
                    if GENERATION.load(Ordering::SeqCst) != generation { let _ = window.destroy(); }
                    else { let _ = window.set_ignore_cursor_events(true); }
                }
                Err(error) => eprintln!("[托盘摘要] {error}"),
            }
        });
        watch_cursor(handle, generation, x, y);
    });
}

/// 光标是否已离开托盘图标。按轴向距离判断而非欧氏距离：托盘图标是方的，
/// 方形判定与图标形状一致，斜向移开时也不会比正向移开更晚关闭。
fn left_icon(cursor: (f64, f64), anchor: (f64, f64)) -> bool {
    (cursor.0 - anchor.0).abs() > LEAVE_RADIUS || (cursor.1 - anchor.1).abs() > LEAVE_RADIUS
}

/// 兜底关闭：托盘的 Leave 事件在 Windows 上并不可靠——光标快速移开、
/// 或直接滑到相邻图标时都可能不触发，只靠它摘要会一直留在屏幕上。
/// 这里按光标与锚点的距离自行判定离开；被新的 enter/close 换代后立即退出。
fn watch_cursor(app: tauri::AppHandle, generation: u64, x: f64, y: f64) {
    loop {
        std::thread::sleep(std::time::Duration::from_millis(WATCH_INTERVAL_MS));
        if GENERATION.load(Ordering::SeqCst) != generation { return; }
        // 取不到光标位置时不做判断：宁可多留一轮，也不误关正在看的摘要
        let Ok(position) = app.cursor_position() else { continue };
        if !left_icon((position.x, position.y), (x, y)) { continue; }
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || close(&handle));
        return;
    }
}

fn position(x: f64, y: f64, width: f64, height: f64, gap: f64, area: (f64, f64, f64, f64)) -> (f64, f64) {
    let (left, top, w, h) = area;
    let py = if y - gap - height >= top { y - gap - height } else { y + gap };
    ((x - width / 2.0).clamp(left, (left + w - width).max(left)), py.clamp(top, (top + h - height).max(top)))
}

#[tauri::command]
pub fn tray_summary_fit(app: tauri::AppHandle, width: f64, height: f64) -> Result<(), String> {
    if !width.is_finite() || !height.is_finite() || !(230.0..=300.0).contains(&width) || !(50.0..=800.0).contains(&height) { return Err("摘要尺寸无效".into()); }
    let window = app.get_webview_window(LABEL).ok_or("摘要已关闭")?;
    if !HOVERING.load(Ordering::SeqCst) {
        // 窗口建成时光标已离开托盘图标：摘要此时绝不能弹出，销毁迟到窗口。
        let _ = window.destroy();
        return Err("摘要已关闭".into());
    }
    let (x, y) = ANCHOR.lock().map_err(|e| e.to_string())?.ok_or("缺少摘要位置")?;
    let monitor = app.monitor_from_point(x, y).map_err(|e| e.to_string())?
        .or(app.primary_monitor().map_err(|e| e.to_string())?).ok_or("找不到显示器")?;
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    let width = (width * scale).ceil().min(f64::from(area.size.width));
    let height = (height * scale).ceil().min(f64::from(area.size.height));
    let (px, py) = position(x, y, width, height, 16.0 * scale, (area.position.x as f64, area.position.y as f64, area.size.width as f64, area.size.height as f64));
    window.set_position(PhysicalPosition::new(px.round() as i32, py.round() as i32)).map_err(|e| e.to_string())?;
    window.set_size(PhysicalSize::new(width as u32, height as u32)).map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_connection_never_uses_another_cache_key() {
        let mut cfg = crate::settings::Settings::default();
        assert_eq!(cache_key(&cfg), None);
        cfg.island_platform = "codex".into(); cfg.island_kind = "auth".into();
        assert_eq!(cache_key(&cfg).as_deref(), Some("local-codex"));
        cfg.island_connection_id = Some("another-account".into());
        assert_eq!(cache_key(&cfg).as_deref(), Some("another-account"));
    }
    #[test]
    fn cursor_leaving_the_icon_is_detected_without_relying_on_the_leave_event() {
        let anchor = (1000.0, 1050.0);
        // 图标范围内的小幅移动不算离开，否则摘要会在手抖时闪烁
        for offset in [(0.0, 0.0), (12.0, -12.0), (-35.9, 35.9), (36.0, 36.0)] {
            assert!(!left_icon((anchor.0 + offset.0, anchor.1 + offset.1), anchor), "{offset:?}");
        }
        // 任一轴超出即判定离开——滑向相邻图标通常只在水平方向移动
        for offset in [(37.0, 0.0), (0.0, -37.0), (-200.0, 0.0), (0.0, 500.0)] {
            assert!(left_icon((anchor.0 + offset.0, anchor.1 + offset.1), anchor), "{offset:?}");
        }
        // 负坐标屏幕（副屏在主屏左侧/上方）同样成立
        let negative = (-1800.0, -90.0);
        assert!(!left_icon((-1790.0, -85.0), negative));
        assert!(left_icon((-1700.0, -90.0), negative));
    }
    #[test]
    fn summary_fits_top_bottom_and_negative_work_areas() {
        for (x, y) in [(-1920.0, -100.0), (-1.0, 939.0), (-1000.0, 400.0)] {
            let (px, py) = position(x, y, 317.5, 220.0, 20.0, (-1920.0, -100.0, 1920.0, 1040.0));
            assert!(px >= -1920.0 && px + 317.5 <= 0.0);
            assert!(py >= -100.0 && py + 220.0 <= 940.0);
        }
    }

    /// 套餐判定只认 5h / 7d（口径同 planCoverage）：月额度、总配额是别的形态
    #[test]
    fn plan_detection_only_counts_5h_and_7d_windows() {
        let window = |key: &str| crate::quota::QuotaWindow {
            key: key.into(), window_name: "窗口".into(),
            used_percent: None, amount_text: None, resets_at: None,
        };
        assert!(has_plan_windows(&[window("5h")]));
        assert!(has_plan_windows(&[window("月消费"), window("7d")]));
        assert!(!has_plan_windows(&[]));
        assert!(!has_plan_windows(&[window("月消费"), window("total")]), "非套餐窗口不按套餐算");
    }

    /// 千分位与前端 toLocaleString("en-US") 一致
    #[test]
    fn thousands_separators_match_the_frontend_locale_format() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(12_345_678), "12,345,678");
    }
}
