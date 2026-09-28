//! 主面板位置与尺寸：关闭后销毁 WebView，重建时仍恢复上次所在的屏幕区域。

use serde::{Deserialize, Serialize};
use tauri::{PhysicalPosition, PhysicalSize, WebviewWindow};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    /// 客户区物理像素，与 WindowEvent::Resized 的尺寸一致。
    pub width: u32,
    pub height: u32,
}

impl Placement {
    pub fn valid(&self) -> bool {
        // Windows 最小化时可能把窗口临时移到 (-32000, -32000)，不能记成用户位置。
        self.x > -20_000 && self.y > -20_000 && self.x < 100_000 && self.y < 100_000
            && (600..=20_000).contains(&self.width) && (400..=20_000).contains(&self.height)
    }
}

pub fn capture(window: &WebviewWindow) -> Option<Placement> {
    let position = window.outer_position().ok()?;
    let size = window.inner_size().ok()?;
    let placement = Placement {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };
    placement.valid().then_some(placement)
}

fn fit(area: (i32, i32, i32, i32), saved: &Placement, center: bool) -> (i32, i32, u32, u32) {
    let (left, top, right, bottom) = area;
    let width = saved.width.min((right - left).max(1) as u32);
    let height = saved.height.min((bottom - top).max(1) as u32);
    let x = if center { left + (right - left - width as i32) / 2 } else { saved.x.clamp(left, right - width as i32) };
    let y = if center { top + (bottom - top - height as i32) / 2 } else { saved.y.clamp(top, bottom - height as i32) };
    (x, y, width, height)
}

pub fn restore(window: &WebviewWindow, saved: &Placement) -> Result<(), String> {
    if !saved.valid() { return Ok(()); }
    let center_x = saved.x + saved.width as i32 / 2;
    let center_y = saved.y + saved.height as i32 / 2;
    let target = window.available_monitors().ok().and_then(|monitors| monitors.into_iter().find(|monitor| {
        let position = monitor.position();
        let size = monitor.size();
        center_x >= position.x && center_x < position.x + size.width as i32
            && center_y >= position.y && center_y < position.y + size.height as i32
    }));
    let missing_monitor = target.is_none();
    if let Some(monitor) = target {
        let position = monitor.position();
        let size = monitor.size();
        let current_size = window.outer_size().map_err(|error| error.to_string())?;
        window.set_position(PhysicalPosition::new(
            position.x + (size.width as i32 - current_size.width as i32) / 2,
            position.y + (size.height as i32 - current_size.height as i32) / 2,
        )).map_err(|error| error.to_string())?;
    }
    let (wx, wy, ww, wh, scale, _) = crate::dock::work_area(window).ok_or("无法读取主面板工作区")?;
    let area = (
        (wx * scale).round() as i32,
        (wy * scale).round() as i32,
        ((wx + ww) * scale).round() as i32,
        ((wy + wh) * scale).round() as i32,
    );
    let (x, y, width, height) = fit(area, saved, missing_monitor);
    window.set_size(PhysicalSize::new(width, height)).map_err(|error| error.to_string())?;
    window.set_position(PhysicalPosition::new(x, y)).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_position_stays_visible_after_resolution_change() {
        let saved = Placement { x: 1800, y: 900, width: 1128, height: 860 };
        assert_eq!(fit((0, 0, 1920, 1080), &saved, false), (792, 220, 1128, 860));
        assert_eq!(fit((0, 0, 1280, 720), &saved, false), (152, 0, 1128, 720));
    }

    #[test]
    fn missing_monitor_centers_window_and_minimized_coordinates_are_rejected() {
        let saved = Placement { x: 2400, y: 200, width: 1000, height: 700 };
        assert_eq!(fit((0, 0, 1920, 1080), &saved, true), (460, 190, 1000, 700));
        assert!(!Placement { x: -32000, ..saved }.valid());
    }
}
