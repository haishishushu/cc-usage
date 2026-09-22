//! 自绘菜单使用独立、按需创建的 WebView；不占用灵动岛的内容窗口。
use std::sync::Mutex;
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "context-menu";
static ANCHOR: Mutex<Option<(f64, f64)>> = Mutex::new(None);

fn fit_position(x: f64, y: f64, width: f64, height: f64, area: (f64, f64, f64, f64)) -> (f64, f64) {
    let (left, top, w, h) = area;
    (x.clamp(left, (left + w - width).max(left)), y.clamp(top, (top + h - height).max(top)))
}

pub fn open(app: &tauri::AppHandle, island: bool) -> Result<(), String> {
    let cursor = app.cursor_position().map_err(|e| e.to_string())?;
    *ANCHOR.lock().map_err(|e| e.to_string())? = Some((cursor.x, cursor.y));
    if let Some(window) = app.get_webview_window(LABEL) {
        window.emit("menu-reopen", island).map_err(|e| e.to_string())?;
        // 前端切回根菜单并完成布局后再更新尺寸，避免先撑大再收缩。
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(format!("index.html?window=menu&source={}", if island { "island" } else { "tray" }).into()))
        .title("CC Usage Menu").inner_size(292.0, 390.0)
        .decorations(false).transparent(true).shadow(false).resizable(false)
        .skip_taskbar(true).always_on_top(true).visible(false).focused(false)
        .build().map_err(|e| e.to_string())?;
    let handle = window.clone();
    let focused_once = std::sync::atomic::AtomicBool::new(false);
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Focused(true)) {
            focused_once.store(true, std::sync::atomic::Ordering::Relaxed);
        } else if matches!(event, tauri::WindowEvent::Focused(false))
            && focused_once.load(std::sync::atomic::Ordering::Relaxed)
        {
            let _ = handle.close();
        }
    });
    // 前端应用布局后由 menu_show 显示，避免空白闪烁与提前失焦。
    Ok(())
}

#[tauri::command]
pub fn menu_close(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) { let _ = window.close(); }
}

#[tauri::command]
pub fn menu_refreshing() -> bool { super::TRAY_REFRESHING.load(std::sync::atomic::Ordering::Relaxed) }

// 根菜单与侧边子菜单共用稳定画布，悬浮时不移动或缩放窗口。
#[derive(serde::Serialize)]
pub struct MenuLayout {
    root_x: f64,
    root_y: f64,
    root_width: f64,
    sub_width: f64,
    side: &'static str,
}

fn menu_layout(x: f64, y: f64, root_height: f64, w: f64, h: f64) -> (f64, f64, f64, f64, MenuLayout) {
    let root_width = 268.0_f64.min(((w - 18.0) / 2.0).max(1.0));
    let sub_width = 300.0_f64.min((w - root_width - 18.0).max(1.0));
    let width = (root_width + sub_width + 18.0).min(w);
    let height = 620.0_f64.min(h);
    let root_height = root_height.min(height - 12.0);
    let mut root_left = x.clamp(6.0, (w - root_width - 6.0).max(6.0));
    let side = if root_left + root_width + sub_width + 12.0 <= w { "right" } else { "left" };
    if side == "left" { root_left = root_left.max(sub_width + 12.0); }
    let root_top = y.clamp(6.0, (h - root_height - 6.0).max(6.0));
    let canvas_left = if side == "right" { root_left - 6.0 } else { root_left - sub_width - 12.0 };
    let (_, canvas_top) = fit_position(canvas_left, root_top - 6.0, width, height, (0.0, 0.0, w, h));
    (canvas_left, canvas_top, width, height, MenuLayout {
        root_x: root_left - canvas_left, root_y: root_top - canvas_top,
        root_width, sub_width, side,
    })
}

#[tauri::command]
pub fn menu_fit(app: tauri::AppHandle, root_height: f64) -> Result<MenuLayout, String> {
    if !root_height.is_finite() || !(40.0..=2000.0).contains(&root_height) {
        return Err("菜单尺寸无效".into());
    }
    let window = app.get_webview_window(LABEL).ok_or("菜单已关闭")?;
    let (x, y) = ANCHOR.lock().map_err(|e| e.to_string())?.ok_or("缺少菜单位置")?;
    let monitor = app.monitor_from_point(x, y).map_err(|e| e.to_string())?
        .or(app.primary_monitor().map_err(|e| e.to_string())?).ok_or("找不到显示器")?;
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    let left = f64::from(area.position.x);
    let top = f64::from(area.position.y);
    let (px, py, width, height, layout) = menu_layout((x - left) / scale, (y - top) / scale,
        root_height, f64::from(area.size.width) / scale, f64::from(area.size.height) / scale);
    window.set_position(PhysicalPosition::new((left + px * scale).round() as i32, (top + py * scale).round() as i32)).map_err(|e| e.to_string())?;
    window.set_size(PhysicalSize::new((width * scale).ceil() as u32, (height * scale).ceil() as u32)).map_err(|e| e.to_string())?;
    Ok(layout)
}

#[tauri::command]
pub fn menu_show(app: tauri::AppHandle) -> Result<(), String> {
    let window = app.get_webview_window(LABEL).ok_or("菜单已关闭")?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn menu_action(app: tauri::AppHandle, id: String) -> Result<(), String> {
    if !matches!(id.as_str(), "open_main" | "refresh" | "toggle_island" | "dnd" | "topmost" | "reset_layout" | "about" | "open_source_settings" | "pos_free" | "pos_top" | "pos_bottom" | "pos_left" | "pos_right" | "quit") {
        return Err("不支持的菜单操作".into());
    }
    // 操作先在后端接收，再销毁菜单，避免前端关闭后丢失 IPC。
    super::handle_tray_menu(&app, &id);
    menu_close(app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{fit_position, menu_layout};
    #[test]
    fn submenu_canvas_fits_edges_and_keeps_root_at_cursor() {
        for (w, h) in [(1920.0, 1040.0), (1280.0, 720.0), (512.0, 400.0)] {
            for (x, y) in [(0.0, 0.0), (w - 1.0, 0.0), (0.0, h - 1.0), (w - 1.0, h - 1.0), (w / 2.0, h / 2.0)] {
                let (left, top, width, height, layout) = menu_layout(x, y, 160.0, w, h);
                assert!(left >= 0.0 && left + width <= w);
                assert!(top >= 0.0 && top + height <= h);
                assert!(layout.root_x >= 6.0 && layout.root_x + layout.root_width <= width - 6.0);
                assert!(layout.root_y >= 6.0 && layout.root_y + 160.0 <= height - 6.0);
                let sub_x = if layout.side == "left" { layout.root_x - layout.sub_width - 6.0 } else { layout.root_x + layout.root_width + 6.0 };
                assert!(sub_x >= 6.0 && sub_x + layout.sub_width <= width - 6.0);
            }
        }
        let (x, y, _, _, layout) = menu_layout(100.0, 200.0, 160.0, 1920.0, 1040.0);
        assert_eq!((x + layout.root_x, y + layout.root_y), (100.0, 200.0));
        assert_eq!(layout.side, "right");
        assert_eq!(menu_layout(1919.0, 900.0, 160.0, 1920.0, 1040.0).4.side, "left");
    }
    #[test]
    fn fits_all_edges_negative_monitors_and_scaled_windows() {
        let area = (-1920.0, -100.0, 1920.0, 1040.0);
        for (x, y) in [(-1920.0, -100.0), (-1.0, -100.0), (-1920.0, 939.0), (-1.0, 939.0)] {
            let (x, y) = fit_position(x, y, 365.0, 487.5, area);
            assert!(x >= -1920.0 && x + 365.0 <= 0.0);
            assert!(y >= -100.0 && y + 487.5 <= 940.0);
        }
        assert_eq!(fit_position(100.0, 100.0, 300.0, 400.0, (0.0, 0.0, 200.0, 200.0)), (0.0, 0.0));
    }
}
