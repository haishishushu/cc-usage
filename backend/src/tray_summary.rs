//! 按需创建的只读托盘摘要；不获取焦点、不穿插额外网络请求。
//! 离开图标后窗口先隐藏保温，短时间内再次悬停直接复用，闲置一段时间才销毁，
//! 避免每次悬停都重建一个 WebView2。
use std::sync::{Mutex, atomic::{AtomicBool, AtomicU64, Ordering}};
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "tray-summary";
/// 隐藏后保温多久再销毁。
const KEEP_WARM: std::time::Duration = std::time::Duration::from_secs(60);
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
    if settings.island_platform == "grok" && settings.island_kind == "auth" {
        return Some("local-grok".into());
    }
    if settings.island_platform == "zcode" && settings.island_kind == "auth" {
        return Some("local-zcode".into());
    }
    settings.island_connection_id.clone().or_else(|| {
        if settings.island_kind != "auth" { return None; }
        match settings.island_platform.as_str() {
            "codex" => Some("local-codex".into()),
            _ => None,
        }
    })
}

fn remaining_percent(used: Option<f64>) -> Option<String> {
    let used = used.filter(|value| value.is_finite())?;
    let remaining = ((100.0 - used.clamp(0.0, 100.0)) * 10.0).round() / 10.0;
    Some(if remaining.fract() == 0.0 { format!("{remaining:.0}%") } else { format!("{remaining:.1}%") })
}

fn native_remaining_reason(platform: &str) -> &'static str {
    match platform {
        "gemini" => "未接入在线项目余额查询；本机 Token 为消耗量",
        "zcode" => "此 API 连接未绑定本机 ZCode BigModel Key",
        "trae" => "企业额度需管理员授权；个人额度暂不可查询",
        "qoder" => "剩余积分需 Qoder Agent SDK；本机积分为消耗量",
        "workbuddy" => "剩余积分需第三方应用授权；本机积分为消耗量",
        _ => "剩余额度暂不可查询",
    }
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
            cache.entries.get(key).and_then(|(value, expires, _, _)| match value {
                crate::quota::ApiUsageState::Ok { total_tokens, .. } => Some((thousands(*total_tokens), *expires)),
                _ => None,
            })
        });
    let today_text = match org_usage {
        Some((text, expires)) => { if expires > latest { latest = expires; } format!("{text} Token") }
        None => app.try_state::<crate::DbRead>().and_then(|db| {
            db.0.with(|conn| crate::db::today_fresh_tokens(conn, platform).ok().flatten())
        }).map_or_else(|| "—".into(), |tokens| format!("{} Token", thousands(tokens.max(0) as u64))),
    };
    summary.quotas.push(Quota { key: "今日Token".into(), value: today_text });

    let balance = crate::BALANCE_CACHE.state.lock().ok()
        .and_then(|cache| {
            cache.entries.get(key).and_then(|(value, expires, _, _)| match value {
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

/// 异步命令：同步命令在主线程执行，读库时会卡住所有窗口。
#[tauri::command]
pub async fn tray_summary_get(app: tauri::AppHandle) -> Result<Summary, String> {
    tauri::async_runtime::spawn_blocking(move || summary(&app)).await.map_err(|e| e.to_string())
}

fn summary(app: &tauri::AppHandle) -> Summary {
    let cfg = app.state::<crate::Cfg>().0.get();
    let platform = crate::platform_name(&cfg.island_platform);
    let kind = if cfg.island_kind == "auth" { "Auth" } else { "API Key" };
    let name = cfg.island_connection_name.as_deref().unwrap_or(match cache_key(&cfg).as_deref() {
        Some("local-zcode") => "本机 BigModel Key",
        Some("local-codex" | "local-grok") => "本机授权",
        _ => "未选择连接",
    });
    let mut summary = Summary { connection: format!("{platform} · {kind} · {name}"), quotas: vec![], updated_at: None, message: "尚无额度数据".into() };
    if crate::platforms::native(&cfg.island_platform) && cfg.island_platform != "grok"
        && !(cfg.island_platform == "zcode" && cfg.island_kind == "auth") {
        summary.message=native_remaining_reason(&cfg.island_platform).into();
        summary.quotas.push(Quota { key:"剩余额度".into(), value:"—".into() });
        return summary;
    }
    if cfg.island_platform == "grok" && cfg.island_kind == "api" {
        summary.quotas.push(Quota { key: "xAI API 预付余额".into(), value: "—".into() });
        summary.message = "需要独立 Management Key 和团队 ID".into();
        return summary;
    }
    if let Some(key) = cache_key(&cfg) {
        if let Ok(cache) = crate::QUOTA_CACHE.state.lock() {
            if let Some((state, expires, _, _)) = cache.entries.get(&key) {
                match state {
                    crate::quota::QuotaState::Ok { windows, .. } => {
                        if cfg.island_platform == "grok" {
                            summary.quotas = windows.iter().map(|window| Quota {
                                key: "SuperGrok 剩余".into(),
                                value: remaining_percent(window.used_percent).unwrap_or("—".into()),
                            }).collect();
                            let captured = *expires - std::time::Duration::from_secs(300);
                            summary.updated_at = Some(chrono::Utc::now().timestamp_millis() - captured.elapsed().as_millis() as i64);
                            summary.message = if summary.quotas.is_empty() { "来源未提供额度" } else { "" }.into();
                            return summary;
                        }
                        if cfg.island_platform == "zcode" && cfg.island_kind == "auth" {
                            summary.quotas = windows.iter().map(|window| Quota {
                                key: if window.key == "5h" { "5 小时剩余" } else { "周剩余" }.into(),
                                value: window.remaining_text.clone()
                                    .or_else(|| remaining_percent(window.used_percent))
                                    .unwrap_or("—".into()),
                            }).collect();
                            let captured = *expires - std::time::Duration::from_secs(300);
                            summary.updated_at = Some(chrono::Utc::now().timestamp_millis() - captured.elapsed().as_millis() as i64);
                            summary.message = if summary.quotas.is_empty() { "来源未提供额度" } else { "" }.into();
                            return summary;
                        }
                        // API Key 且确认无 5h/7d 套餐窗口：改显「今日Token + 剩余金额」
                        //（2026-09-19 鼠鼠定版）。查询失败/凭证失效不算无套餐，仍按原样说明原因。
                        if cfg.island_kind == "api" && !has_plan_windows(windows) {
                            fill_api_key_usage(app, &mut summary, &key, &cfg.island_platform, *expires);
                        } else {
                            // 口径统一为「剩余」（与 grok/zcode 分支及前端灵动岛一致）
                            summary.quotas = windows.iter().map(|window| Quota {
                                key: format!("{} 剩余", window.key),
                                value: remaining_percent(window.used_percent)
                                    .or_else(|| window.amount_text.clone()).unwrap_or("—".into()),
                            }).collect();
                            // 成功缓存 TTL 固定 5 分钟；由写入时刻计算，命中缓存不伪装为刚更新。
                            let captured = *expires - std::time::Duration::from_secs(300);
                            summary.updated_at = Some(chrono::Utc::now().timestamp_millis() - captured.elapsed().as_millis() as i64);
                            summary.message = if windows.is_empty() { "来源未提供额度窗口" } else { "" }.into();
                        }
                    }
                    crate::quota::QuotaState::Unsupported { reason }
                    | crate::quota::QuotaState::Unauthorized { reason }
                    | crate::quota::QuotaState::Forbidden { reason }
                    | crate::quota::QuotaState::RateLimited { reason }
                    | crate::quota::QuotaState::Failed { reason }
                        if cfg.island_platform == "zcode" && cfg.island_kind == "auth" => {
                        summary.message = reason.clone();
                    }
                    crate::quota::QuotaState::Unsupported { .. } if cfg.island_kind == "api" => {
                        // 明确不提供订阅额度 = API Key 无套餐的确定性结论
                        fill_api_key_usage(app, &mut summary, &key, &cfg.island_platform, *expires);
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
    if cfg.island_platform == "grok" && summary.quotas.is_empty() {
        summary.quotas.push(Quota { key: "SuperGrok 剩余".into(), value: "—".into() });
        if summary.message == "尚无额度数据" { summary.message = "等待 Grok 订阅额度查询".into(); }
    }
    if cfg.island_platform == "zcode" && cfg.island_kind == "auth" && summary.quotas.is_empty() {
        summary.quotas.push(Quota { key: "BigModel 套餐剩余".into(), value: "—".into() });
        if summary.message == "尚无额度数据" { summary.message = "等待 ZCode BigModel Key 额度查询".into(); }
    }
    summary
}

#[cfg(test)]
mod remaining_tests {
    use super::*;

    #[test]
    fn grok_remaining_requires_real_used_percentage() {
        assert_eq!(remaining_percent(Some(37.5)), Some("62.5%".into()));
        assert_eq!(remaining_percent(Some(100.0)), Some("0%".into()));
        assert_eq!(remaining_percent(None), None);
        assert_eq!(remaining_percent(Some(f64::NAN)), None);
    }
}

pub fn close(app: &tauri::AppHandle) {
    HOVERING.store(false, Ordering::SeqCst);
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    park(app, generation);
}

/// 隐藏摘要并在保温期后销毁；`generation` 之后若有新的悬停或关闭，本次销毁作废。
fn park(app: &tauri::AppHandle, generation: u64) {
    let Some(window) = app.get_webview_window(LABEL) else { return };
    let _ = window.hide();
    // 广播即可：只有摘要页监听这两个事件名
    let _ = app.emit("tray-summary-close", ());
    // 保温期内没有新的悬停才销毁；期间任何 enter / close 都会换代，旧计时作废。
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(KEEP_WARM);
        if GENERATION.load(Ordering::SeqCst) != generation { return; }
        let app = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            if GENERATION.load(Ordering::SeqCst) == generation && !HOVERING.load(Ordering::SeqCst) {
                if let Some(window) = app.get_webview_window(LABEL) { let _ = window.destroy(); }
            }
        });
    });
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
            if GENERATION.load(Ordering::SeqCst) != generation { return; }
            if app.get_webview_window(LABEL).is_some() {
                // 保温中的窗口：通知页面重读摘要并按内容尺寸重新定位、显示。
                let _ = app.emit("tray-summary-open", ());
                return;
            }
            match WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::App("index.html?window=tray-summary".into()))
                .title("CC Usage Summary").inner_size(254.0, 150.0)
                .decorations(false).transparent(true).shadow(false).resizable(false)
                .skip_taskbar(true).always_on_top(true).visible(false).focused(false).focusable(false)
                .build() {
                Ok(window) => {
                    // 建窗期间可能已收到离开事件，而当时窗口尚未注册、关闭扑空。
                    // 建成后立刻复查代数，过期就走正常关闭流程（隐藏保温、到期销毁），不留孤儿窗口。
                    // 若期间已重新悬停，新一轮 enter 会复用这个窗口，这里不能作废它。
                    if GENERATION.load(Ordering::SeqCst) != generation {
                        if !HOVERING.load(Ordering::SeqCst) { park(&app, GENERATION.load(Ordering::SeqCst)); }
                    }
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
        // 光标已离开托盘图标：摘要此时绝不能弹出，保持隐藏，由保温计时负责销毁。
        let _ = window.hide();
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
            used_percent: None, amount_text: None, remaining_text: None, resets_at: None,
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
