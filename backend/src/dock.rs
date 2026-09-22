//! 灵动岛拖动与贴边停靠（§2.1.3）
//!
//! 分工：拖动本身由 webview 的 `data-tauri-drag-region` 承担，
//! 这里负责**吸附判定与几何落位**——只有 Rust 能拿到显示器工作区，
//! 才能做到「不跨显示器、不覆盖任务栏」。
//!
//! 停靠态与收缩/展开是两个维度：停靠 = 位置贴边 + 尺寸收到极小。

use serde::{Deserialize, Serialize};
use tauri::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize, WebviewWindow};

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Rect { left: i32, top: i32, right: i32, bottom: i32 }
#[cfg(target_os = "windows")]
#[repr(C)]
struct MonitorInfo { size: u32, monitor: Rect, work: Rect, flags: u32 }
#[cfg(target_os = "windows")]
#[link(name = "user32")]
extern "system" {
    fn MonitorFromWindow(hwnd: isize, flags: u32) -> isize;
    fn GetMonitorInfoW(monitor: isize, info: *mut MonitorInfo) -> i32;
    fn ReleaseCapture() -> i32;
    fn SendMessageW(hwnd: isize, message: u32, wparam: usize, lparam: isize) -> isize;
}

/// SendMessage 的系统移动循环返回时，鼠标已松开，不依赖 WebView 的 pointerup。
#[cfg(target_os = "windows")]
pub fn drag(win: &WebviewWindow) -> Result<(), String> {
    let hwnd = win.hwnd().map_err(|e| e.to_string())?.0 as isize;
    unsafe {
        ReleaseCapture();
        SendMessageW(hwnd, 0x0112, 0xF012, 0); // WM_SYSCOMMAND / SC_MOVE | HTCAPTION
    }
    Ok(())
}
#[cfg(not(target_os = "windows"))]
pub fn drag(win: &WebviewWindow) -> Result<(), String> {
    win.start_dragging().map_err(|e| e.to_string())
}

/// 窗口在工作区内距边缘 ≤ 24 逻辑像素，或已经越过边缘时触发吸附。
/// 沿边距中点同样采用此阈值。
pub const SNAP_THRESHOLD: f64 = 24.0;
pub static DRAGGING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 停靠条尺寸：上/下 120×14，左/右 14×120
const BAR_LONG: f64 = 120.0;
const BAR_SHORT: f64 = 14.0;

/// 自由态收缩尺寸，解除停靠时恢复
const FREE_W: f64 = 428.0;
const FREE_H: f64 = 124.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

impl Edge {
    fn as_str(self) -> &'static str {
        match self {
            Edge::Top => "top",
            Edge::Bottom => "bottom",
            Edge::Left => "left",
            Edge::Right => "right",
        }
    }
}

/// 持久化的停靠状态（§2.1.3）：显示器 + 边缘 + 沿边偏移
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DockState {
    pub edge: Option<Edge>,
    /// 沿边偏移：横边为 x，竖边为 y，单位为逻辑像素
    pub offset: f64,
    /// 显示器名称。显示器变更后据此判断能否恢复
    pub monitor: Option<String>,
}

/// 吸附结果，交给前端切换形态
#[derive(Debug, Serialize)]
pub struct SnapResult {
    pub edge: Option<Edge>,
    pub offset: f64,
}

/// 当前窗口所在显示器的工作区（逻辑像素）。
/// 使用窗口主要所在的显示器，避免左上角与 Win32 工作区选择不同显示器。
pub(crate) fn work_area(win: &WebviewWindow) -> Option<(f64, f64, f64, f64, f64, Option<String>)> {
    // 与 MonitorFromWindow 一致，选择窗口主要所在的显示器。
    let hit = win.current_monitor().ok().flatten()
        .or_else(|| win.primary_monitor().ok().flatten())?;

    let scale = hit.scale_factor();
    let mp = hit.position();
    let ms = hit.size();
    #[cfg(target_os = "windows")]
    if let Ok(hwnd) = win.hwnd() {
        let mut info = MonitorInfo { size: std::mem::size_of::<MonitorInfo>() as u32,
            monitor: Rect::default(), work: Rect::default(), flags: 0 };
        unsafe {
            let monitor = MonitorFromWindow(hwnd.0 as isize, 2);
            if GetMonitorInfoW(monitor, &mut info) != 0 {
                let r = info.work;
                return Some((r.left as f64 / scale, r.top as f64 / scale,
                    (r.right-r.left) as f64 / scale, (r.bottom-r.top) as f64 / scale,
                    scale, hit.name().cloned()));
            }
        }
    }
    Some((
        mp.x as f64 / scale,
        mp.y as f64 / scale,
        ms.width as f64 / scale,
        ms.height as f64 / scale,
        scale,
        hit.name().cloned(),
    ))
}

/// 松手时判定是否吸附。返回 None 表示不靠边、保持自由态。
///
/// 使用窗口边缘到工作区边缘的距离，沿边方向用窗口中心判断居中。
pub fn snap_on_release(win: &WebviewWindow) -> Option<SnapResult> {
    let hint = hover_hint(win)?;
    apply_dock(win, hint.edge, hint.offset);
    Some(SnapResult { edge: Some(hint.edge), offset: hint.offset })
}

/// 预览与松手共用的落位结果，绿色仅表示松手后会居中。
#[derive(Debug, Clone, Copy, Serialize)]
pub struct SnapHint {
    pub edge: Edge,
    pub offset: f64,
    pub centered: bool,
}

fn edge_center(edge: Edge, width: f64, height: f64) -> f64 {
    match edge {
        Edge::Top | Edge::Bottom => width / 2.0,
        Edge::Left | Edge::Right => height / 2.0,
    }
}

pub fn centered_offset(win: &WebviewWindow, edge: Edge) -> Option<f64> {
    let (_, _, width, height, _, _) = work_area(win)?;
    Some(edge_center(edge, width, height))
}

fn snap_hint(area: (f64, f64, f64, f64), rect: (f64, f64, f64, f64)) -> Option<SnapHint> {
    let (wx, wy, ww, wh) = area;
    let (x, y, w, h) = rect;
    let mut best = (SNAP_THRESHOLD, None);
    for (distance, edge) in [
        (x - wx, Edge::Left),
        (y - wy, Edge::Top),
        (wx + ww - x - w, Edge::Right),
        (wy + wh - y - h, Edge::Bottom),
    ] {
        // 正数是工作区内的间距，负数是越界深度。快速拖动可能直接跨过
        // 整个提示区；越界越深越应优先回收，不能取绝对值后排除。
        if distance <= best.0 { best = (distance, Some(edge)); }
    }
    let edge = best.1?;
    let offset = match edge {
        Edge::Top | Edge::Bottom => x + w / 2.0 - wx,
        Edge::Left | Edge::Right => y + h / 2.0 - wy,
    };
    let center = edge_center(edge, ww, wh);
    let centered = (offset - center).abs() <= SNAP_THRESHOLD;
    Some(SnapHint { edge, offset: if centered { center } else { offset }, centered })
}

/// 把窗口收成停靠条并对齐到指定边缘。
/// 沿边偏移会被夹在工作区内，避免停靠条半截在屏幕外。
pub fn apply_dock(win: &WebviewWindow, edge: Edge, offset: f64) {
    let (w, h) = match edge {
        Edge::Top | Edge::Bottom => (BAR_LONG, BAR_SHORT),
        Edge::Left | Edge::Right => (BAR_SHORT, BAR_LONG),
    };
    if let Err(error) = anchor(win, edge, offset, w, h) {
        eprintln!("[停靠] {} 落位失败: {error}", edge.as_str());
    }
}

/// 按保存的显示器名称恢复停靠位置。目标屏仍存在时先把窗口移入该屏，
/// 再由 `work_area` 读取其真实工作区；屏幕已拔除时回退主显示器。
/// 返回最终使用的显示器名称，调用方据此修正过期配置。
pub fn restore_dock(win: &WebviewWindow, state: &DockState) -> Option<String> {
    let edge = state.edge?;
    let monitors = win.available_monitors().ok()?;
    let target = state.monitor.as_deref()
        .and_then(|name| monitors.iter().find(|monitor| monitor.name().map(String::as_str) == Some(name)))
        .or_else(|| monitors.iter().find(|monitor| {
            win.primary_monitor().ok().flatten().as_ref().is_some_and(|primary| {
                primary.name() == monitor.name()
            })
        }))
        .or_else(|| monitors.first())?;
    let position = target.position();
    let size = target.size();
    // 先放进目标显示器中心，确保 Windows 的 MonitorFromWindow 选中正确屏幕。
    let _ = win.set_position(PhysicalPosition::new(
        position.x + size.width as i32 / 2,
        position.y + size.height as i32 / 2,
    ));
    apply_dock(win, edge, state.offset);
    win.current_monitor().ok().flatten().and_then(|monitor| monitor.name().cloned())
}

pub fn monitor_exists(win: &WebviewWindow, name: Option<&str>) -> bool {
    let Some(name) = name else { return false };
    win.available_monitors().ok().is_some_and(|monitors| {
        monitors.iter().any(|monitor| monitor.name().map(String::as_str) == Some(name))
    })
}

// 尺寸和坐标使用同一组物理像素，避免 125% DPI 下右/下边缘取整越界。
fn anchored_rect(area: (f64, f64, f64, f64), scale: f64, edge: Edge,
    offset: f64, w: f64, h: f64) -> (i32, i32, u32, u32) {
    let (x, y, width, height) = area;
    let left = (x * scale).round() as i32;
    let top = (y * scale).round() as i32;
    let right = ((x + width) * scale).round() as i32;
    let bottom = ((y + height) * scale).round() as i32;
    let pw = ((w * scale).round() as u32).max(1).min((right - left).max(1) as u32);
    let ph = ((h * scale).round() as u32).max(1).min((bottom - top).max(1) as u32);
    let cx = (((x + offset) * scale).round() as i32 - pw as i32 / 2)
        .clamp(left, (right - pw as i32).max(left));
    let cy = (((y + offset) * scale).round() as i32 - ph as i32 / 2)
        .clamp(top, (bottom - ph as i32).max(top));
    let (px, py) = match edge {
        Edge::Top => (cx, top),
        Edge::Bottom => (cx, bottom - ph as i32),
        Edge::Left => (left, cy),
        Edge::Right => (right - pw as i32, cy),
    };
    (px, py, pw, ph)
}

/// 先读取原显示器工作区，再统一调整尺寸和贴边位置。
pub fn anchor(win: &WebviewWindow, edge: Edge, offset: f64, w: f64, h: f64) -> Result<(), String> {
    let (x, y, width, height, scale, _) = work_area(win).ok_or("无法读取停靠工作区")?;
    let (px, py, pw, ph) = anchored_rect((x, y, width, height), scale, edge, offset, w, h);
    win.set_size(PhysicalSize::new(pw, ph)).map_err(|e| e.to_string())?;
    win.set_position(PhysicalPosition::new(px, py)).map_err(|e| e.to_string())
}

fn free_rect(area: (f64, f64, f64, f64), scale: f64,
    position: (i32, i32), size: (f64, f64)) -> (i32, i32, u32, u32) {
    let (wx, wy, ww, wh) = area;
    let left = (wx * scale).round() as i32;
    let top = (wy * scale).round() as i32;
    let right = ((wx + ww) * scale).round() as i32;
    let bottom = ((wy + wh) * scale).round() as i32;
    let width = ((size.0 * scale).round() as u32).max(1);
    let height = ((size.1 * scale).round() as u32).max(1);
    (position.0.clamp(left, (right - width as i32).max(left)),
     position.1.clamp(top, (bottom - height as i32).max(top)), width, height)
}

/// 自由态展开时保持内容尺寸，并将窗口移回当前工作区，避免底部落到屏幕外。
pub fn resize_free(win: &WebviewWindow, width: f64, height: f64) -> Result<(), String> {
    let (wx, wy, ww, wh, scale, _) = work_area(win).ok_or("无法读取窗口工作区")?;
    let position = win.outer_position().map_err(|error| error.to_string())?;
    let (x, y, w, h) = free_rect((wx, wy, ww, wh), scale,
        (position.x, position.y), (width, height));
    win.set_size(PhysicalSize::new(w, h)).map_err(|error| error.to_string())?;
    win.set_position(PhysicalPosition::new(x, y)).map_err(|error| error.to_string())
}

/// 解除停靠：恢复自由态尺寸，并从边缘挪开一点，避免立刻又被判定吸附
pub fn undock(win: &WebviewWindow) {
    let _ = win.set_size(LogicalSize::new(FREE_W, FREE_H));
    if let Some((wx, wy, ww, wh, scale, _)) = work_area(win) {
        if let Ok(pos) = win.outer_position() {
            let x = (pos.x as f64 / scale).clamp(wx + SNAP_THRESHOLD + 1.0, wx + ww - FREE_W);
            let y = (pos.y as f64 / scale).clamp(wy + SNAP_THRESHOLD + 1.0, wy + wh - FREE_H);
            let _ = win.set_position(LogicalPosition::new(x, y));
        }
    }
}

/// 拖动中判断当前离哪条边够近，用于前端画吸附指示条。
/// 只读，不改窗口几何。
pub fn hover_edge(win: &WebviewWindow) -> Option<Edge> {
    hover_hint(win).map(|hint| hint.edge)
}

pub fn hover_hint(win: &WebviewWindow) -> Option<SnapHint> {
    let (wx, wy, ww, wh, scale, _) = work_area(win)?;
    let pos = win.outer_position().ok()?;
    let size = win.outer_size().ok()?;
    snap_hint((wx, wy, ww, wh), (
        pos.x as f64 / scale, pos.y as f64 / scale,
        size.width as f64 / scale, size.height as f64 / scale,
    ))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_drag_past_any_edge_still_snaps_on_final_position() {
        // 使用负坐标副屏和非零工作区起点，直接测试最终位置，不依赖中途命中提示。
        let area = (-1600.0, 40.0, 1600.0, 960.0);
        for overshoot in [25.0, 80.0, 400.0, 2000.0] {
            for (edge, rect) in [
                (Edge::Left, (-1600.0 - overshoot, 460.0, 400.0, 120.0)),
                (Edge::Right, (-400.0 + overshoot, 460.0, 400.0, 120.0)),
                (Edge::Top, (-1000.0, 40.0 - overshoot, 400.0, 120.0)),
                (Edge::Bottom, (-1000.0, 880.0 + overshoot, 400.0, 120.0)),
            ] {
                let hint = snap_hint(area, rect).expect("越过边缘仍应吸附");
                assert_eq!(hint.edge, edge);
                for scale in [1.0, 1.25, 1.5, 2.0] {
                    let (bw, bh) = if matches!(edge, Edge::Top | Edge::Bottom) { (120.0, 14.0) } else { (14.0, 120.0) };
                    let (x, y, w, h) = anchored_rect(area, scale, edge, hint.offset, bw, bh);
                    assert!(x >= (-1600.0 * scale) as i32 && x + w as i32 <= 0);
                    assert!(y >= (40.0 * scale) as i32 && y + h as i32 <= (1000.0 * scale) as i32);
                }
            }
        }
    }

    #[test]
    fn crossed_edge_wins_over_nearby_edge_and_inside_threshold_is_unchanged() {
        let area = (0.0, 0.0, 1600.0, 960.0);
        assert_eq!(snap_hint(area, (10.0, 930.0, 400.0, 120.0)).unwrap().edge, Edge::Bottom);
        assert_eq!(snap_hint(area, (-70.0, -130.0, 400.0, 120.0)).unwrap().edge, Edge::Top);
        assert!(snap_hint(area, (25.0, 300.0, 400.0, 120.0)).is_none());
        assert_eq!(snap_hint(area, (24.0, 300.0, 400.0, 120.0)).unwrap().edge, Edge::Left);
    }

    #[test]
    fn expanded_free_window_moves_inside_work_area_without_clipping() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let (x, y, w, h) = free_rect((-1600.0, 40.0, 1600.0, 960.0), scale,
                ((-200.0 * scale) as i32, (900.0 * scale) as i32), (428.0, 420.0));
            assert_eq!(w, (428.0 * scale).round() as u32);
            assert_eq!(h, (420.0 * scale).round() as u32);
            assert_eq!(x + w as i32, 0);
            assert_eq!(y + h as i32, (1000.0 * scale).round() as i32);
        }
    }

    #[test]
    fn four_edges_snap_to_center_within_threshold() {
        let area = (-1600.0, 40.0, 1600.0, 960.0);
        for (edge, rect, expected) in [
            (Edge::Top, (-980.0, 45.0, 400.0, 120.0), 800.0),
            (Edge::Bottom, (-980.0, 875.0, 400.0, 120.0), 800.0),
            (Edge::Left, (-1595.0, 480.0, 400.0, 120.0), 480.0),
            (Edge::Right, (-405.0, 480.0, 400.0, 120.0), 480.0),
        ] {
            let hint = snap_hint(area, rect).unwrap();
            assert_eq!(hint.edge, edge);
            assert!(hint.centered);
            assert_eq!(hint.offset, expected);
            assert_eq!(edge_center(edge, area.2, area.3), expected);
        }
    }

    #[test]
    fn center_hint_boundary_and_non_edge_drag() {
        let area = (0.0, 0.0, 1600.0, 960.0);
        let near = snap_hint(area, (624.0, 0.0, 400.0, 120.0)).unwrap();
        assert!(near.centered);
        assert_eq!(near.offset, 800.0);
        let away = snap_hint(area, (625.0, 0.0, 400.0, 120.0)).unwrap();
        assert!(!away.centered);
        assert_eq!(away.offset, 825.0);
        assert!(snap_hint(area, (600.0, 300.0, 400.0, 120.0)).is_none());
    }

    #[test]
    fn centered_bar_has_balanced_margins_at_multiple_scales() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
                let horizontal = matches!(edge, Edge::Top | Edge::Bottom);
                let (w, h) = if horizontal { (120.0, 14.0) } else { (14.0, 120.0) };
                let (x, y, pw, ph) = anchored_rect((0.0, 40.0, 1600.0, 960.0), scale,
                    edge, edge_center(edge, 1600.0, 960.0), w, h);
                let error = if horizontal {
                    (x * 2 + pw as i32) as f64 - 1600.0 * scale
                } else {
                    (y * 2 + ph as i32) as f64 - 1040.0 * scale
                };
                assert!(error.abs() <= 1.0);
            }
        }
    }

    #[test]
    fn right_bar_stays_inside_at_125_percent() {
        let (x, _, w, _) = anchored_rect((0.0, 0.0, 1536.0, 816.0), 1.25,
            Edge::Right, 351.6, 14.0, 120.0);
        assert_eq!((x, w, x + w as i32), (1902, 18, 1920));
    }

    #[test]
    fn all_edges_keep_bar_and_peek_in_work_area() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
                for offset in [-1000.0, 351.6, 5000.0] {
                    for (w, h) in [(14.0, 120.0), (120.0, 14.0), (428.0, 129.0)] {
                        let (x, y, pw, ph) = anchored_rect((-1536.0, 0.0, 1536.0, 816.0),
                            scale, edge, offset, w, h);
                        assert!(x >= (-1536.0 * scale).round() as i32 && y >= 0);
                        assert!(x + pw as i32 <= 0);
                        assert!(y + ph as i32 <= (816.0 * scale).round() as i32);
                    }
                }
            }
        }
    }
}
