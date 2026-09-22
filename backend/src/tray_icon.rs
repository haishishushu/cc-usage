use tauri::image::Image;

/// 系统托盘图标的基准边长：Windows 在 100% DPI 下索取 16px，
/// 之后随缩放线性放大（125%→20、150%→24、175%→28、200%→32，以此类推）。
const BASE_TRAY_SIZE: usize = 16;

/// 预烘焙的托盘底图档位（未编码 RGBA，由 scripts/icons/generate.mjs 生成）。
///
/// 壳层按 WM_DISPLAYNAME 的 SM_CXSMICON 尺寸绘制托盘图标：喂 32px 而系统要 16px
/// 时会被二次缩小，这正是托盘发糊的来源。按系统实际尺寸出图后全程零缩放；
/// 档位覆盖 100%–250% 缩放，极端 DPI 下退化到最近一档，只余一次轻微采样。
static BASE_16: &[u8] = include_bytes!("../icons/tray-base-16.rgba");
static BASE_20: &[u8] = include_bytes!("../icons/tray-base-20.rgba");
static BASE_24: &[u8] = include_bytes!("../icons/tray-base-24.rgba");
static BASE_28: &[u8] = include_bytes!("../icons/tray-base-28.rgba");
static BASE_32: &[u8] = include_bytes!("../icons/tray-base-32.rgba");
static BASE_36: &[u8] = include_bytes!("../icons/tray-base-36.rgba");
static BASE_40: &[u8] = include_bytes!("../icons/tray-base-40.rgba");
const _: () = assert!(BASE_16.len() == 16 * 16 * 4, "tray-base-16.rgba 档位不符");
const _: () = assert!(BASE_20.len() == 20 * 20 * 4, "tray-base-20.rgba 档位不符");
const _: () = assert!(BASE_24.len() == 24 * 24 * 4, "tray-base-24.rgba 档位不符");
const _: () = assert!(BASE_28.len() == 28 * 28 * 4, "tray-base-28.rgba 档位不符");
const _: () = assert!(BASE_32.len() == 32 * 32 * 4, "tray-base-32.rgba 档位不符");
const _: () = assert!(BASE_36.len() == 36 * 36 * 4, "tray-base-36.rgba 档位不符");
const _: () = assert!(BASE_40.len() == 40 * 40 * 4, "tray-base-40.rgba 档位不符");

const TRAY_BASES: [(usize, &[u8]); 7] = [
    (16, BASE_16),
    (20, BASE_20),
    (24, BASE_24),
    (28, BASE_28),
    (32, BASE_32),
    (36, BASE_36),
    (40, BASE_40),
];

/// 按窗口缩放倍数算出系统托盘索取的边长（SM_CXSMICON = 16 × DPI / 96），
/// 再吸附到最近的烘焙档。超出烘焙范围的 DPI（>250%）封顶到最大档。
pub fn output_size(scale_factor: f64) -> usize {
    let wanted = (BASE_TRAY_SIZE as f64 * scale_factor).round().clamp(16.0, 40.0) as usize;
    TRAY_BASES
        .iter()
        .min_by_key(|(size, _)| size.abs_diff(wanted))
        .map(|(size, _)| *size)
        .unwrap_or(40)
}

/// 托盘底图。请求尺寸与烘焙档完全一致时全程零缩放；
/// 仅极端 DPI 才取最近一档，由壳层做唯一一次采样。
pub fn base(size: usize) -> Image<'static> {
    let (baked, rgba) = TRAY_BASES
        .iter()
        .min_by_key(|(baked, _)| baked.abs_diff(size))
        .unwrap_or(&TRAY_BASES[TRAY_BASES.len() - 1]);
    Image::new(rgba, *baked as u32, *baked as u32)
}

/// 图标本体在托盘画布里占的比例，右下角剩下的地方留给额度状态角标。
const ARTWORK: f64 = 1.0;

/// 角标几何，以 32px 画布为基准描述，再按实际输出尺寸等比放大。
const BADGE_CENTER: f64 = 23.7;
const BADGE_RADIUS: f64 = 5.1;
const BADGE_RING: f64 = 6.65;

/// 双线性采样底图，返回 [预乘 R, 预乘 G, 预乘 B, A]。
///
/// 预乘是必须的：图标边缘之外是全透明的黑，直接插值直色会把绿边拽向黑色，
/// 在托盘那种小尺寸上看起来就是一圈脏边。
fn sample(base: &Image<'_>, u: f64, v: f64) -> [f64; 4] {
    let width = base.width() as usize;
    let height = base.height() as usize;
    let fx = (u * width as f64 - 0.5).clamp(0.0, (width - 1) as f64);
    let fy = (v * height as f64 - 0.5).clamp(0.0, (height - 1) as f64);
    let x0 = fx.floor() as usize;
    let y0 = fy.floor() as usize;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let tx = fx - x0 as f64;
    let ty = fy - y0 as f64;
    let pixels = base.rgba();

    let mut out = [0.0f64; 4];
    for (x, y, weight) in [
        (x0, y0, (1.0 - tx) * (1.0 - ty)),
        (x1, y0, tx * (1.0 - ty)),
        (x0, y1, (1.0 - tx) * ty),
        (x1, y1, tx * ty),
    ] {
        let i = (y * width + x) * 4;
        let alpha = pixels[i + 3] as f64 / 255.0;
        for channel in 0..3 {
            out[channel] += pixels[i + channel] as f64 * alpha * weight;
        }
        out[3] += pixels[i + 3] as f64 * weight;
    }
    out
}

/// 原型画板 18：应用图标 + 右下角状态角标，白色描边在深浅任务栏均可辨识。
pub fn badge(base: &Image<'_>, color: [u8; 3]) -> Image<'static> {
    let size = base.width() as usize;
    let scale = size as f64 / 32.0;
    let center = BADGE_CENTER * scale;
    let radius = BADGE_RADIUS * scale;
    let ring = BADGE_RING * scale;
    let artwork = ARTWORK * size as f64;

    let mut pixels = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let mut sum = [0.0f64; 4];
            for sy in 0..4 {
                for sx in 0..4 {
                    let px = x as f64 + (sx as f64 + 0.5) / 4.0;
                    let py = y as f64 + (sy as f64 + 0.5) / 4.0;
                    let distance = (px - center).hypot(py - center);
                    let sample = if distance <= radius {
                        [color[0] as f64, color[1] as f64, color[2] as f64, 255.0]
                    } else if distance <= ring {
                        [255.0; 4]
                    } else if px < artwork && py < artwork {
                        sample(base, px / artwork, py / artwork)
                    } else {
                        [0.0; 4]
                    };
                    for channel in 0..4 {
                        sum[channel] += sample[channel];
                    }
                }
            }
            let i = (y * size + x) * 4;
            if sum[3] > 0.0 {
                for channel in 0..3 {
                    pixels[i + channel] = (sum[channel] * 255.0 / sum[3]).round() as u8;
                }
                pixels[i + 3] = (sum[3] / 16.0).round() as u8;
            }
        }
    }
    Image::new_owned(pixels, size as u32, size as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一张中心不透明的测试底图，边长 32，便于按 32 基准断言角标几何。
    fn test_base() -> Image<'static> {
        let mut pixels = vec![0u8; 32 * 32 * 4];
        for y in 4..28 {
            for x in 4..28 {
                let i = (y * 32 + x) * 4;
                pixels[i..i + 4].copy_from_slice(&[80; 4]);
            }
        }
        Image::new_owned(pixels, 32, 32)
    }

    fn at(icon: &Image<'_>, x: usize, y: usize) -> [u8; 4] {
        let w = icon.width() as usize;
        let i = (y * w + x) * 4;
        icon.rgba()[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn output_size_follows_system_small_icon_metric() {
        assert_eq!(output_size(1.0), 16);
        assert_eq!(output_size(1.25), 20);
        assert_eq!(output_size(1.5), 24);
        assert_eq!(output_size(1.75), 28);
        assert_eq!(output_size(2.0), 32);
        assert_eq!(output_size(2.5), 40);
        // 超出烘焙档的极端 DPI 封顶，非标准缩放吸附到最近一档。
        assert_eq!(output_size(3.0), 40);
        assert_eq!(output_size(1.1), 16);
    }

    #[test]
    fn base_matches_the_requested_tier_exactly() {
        for size in [16, 20, 24, 28, 32, 36, 40] {
            let base = base(size);
            assert_eq!(base.width(), size as u32);
            assert_eq!(base.height(), size as u32);
            assert_eq!(base.rgba().len(), size * size * 4);
        }
    }

    #[test]
    fn embedded_base_is_opaque_dark_island_in_the_middle() {
        // 底图中心落在深色灵动岛上，这条断言用来挡住「底图没跟着设计稿重新生成」。
        for size in [16, 32, 40] {
            let base = base(size);
            let mid = size / 2;
            let i = (mid * size + mid) * 4;
            let pixel = &base.rgba()[i..i + 4];
            assert_eq!(pixel[3], 255, "{size}px 底图中心应不透明");
            assert!(pixel[0] < 40 && pixel[1] < 40 && pixel[2] < 40, "{size}px 应为深色岛，实际 {pixel:?}");
        }
    }

    #[test]
    fn badge_retains_base_and_has_white_outline_and_transparent_corners() {
        let icon = badge(&test_base(), [6, 193, 103]);
        assert_eq!(icon.width(), 32);
        assert_eq!(at(&icon, 23, 23), [6, 193, 103, 255]);
        assert_eq!(at(&icon, 29, 23), [255, 255, 255, 255]);
        assert_eq!(at(&icon, 31, 31), [0; 4]);
        assert_eq!(at(&icon, 10, 10), [80; 4]);
    }

    #[test]
    fn badge_scales_down_to_the_native_tray_size() {
        // 16px 是 100% DPI 下托盘的真实尺寸：角标和白色描边都必须还画得出来。
        let icon = badge(&base(16), [6, 193, 103]);
        assert_eq!(icon.width(), 16);
        // 16px 下双线性采样会把邻像素的圆角边缘洇进角点（alpha ≤ 1 成），
        // 只要不是实心角块就算圆角成立；32px 的严格透明由另一条用例把守。
        assert!(at(&icon, 0, 0)[3] <= 32, "角点应为透明，实际 {:?}", at(&icon, 0, 0));
        assert_eq!(at(&icon, 11, 11), [6, 193, 103, 255]);
        // 16px 时描边只有 1–2px 宽且与底图混色，逐像素断言纯白太脆；
        // 改查中心行右侧存在近白像素，即白色描边可见。
        let ring = (12..16).map(|x| at(&icon, x, 11)).find(|p| p[0] > 220 && p[1] > 220);
        assert!(ring.is_some(), "16px 下白色描边应可见");
    }

    #[test]
    fn corners_stay_transparent_so_the_rounded_plate_reads_as_rounded() {
        let icon = badge(&base(32), [6, 193, 103]);
        assert_eq!(at(&icon, 0, 0), [0; 4]);
    }
}
