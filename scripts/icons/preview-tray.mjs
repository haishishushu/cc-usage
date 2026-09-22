/**
 * 托盘图标预览：
 *
 *   node scripts/icons/preview-tray.mjs
 *
 * 读的是真实产物 backend/icons/tray-base-{16,32}.rgba，角标几何抄自 backend/src/tray_icon.rs，
 * 两边常量必须一致；改了 Rust 那份记得同步这里，否则预览会骗人。
 * 输出四种额度状态 × 两档真实托盘尺寸（16px = 100% DPI、32px = 200% DPI），
 * 每档左边 1:1、右边放大 6× 看边缘。
 */
import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"
import { encodePNG } from "./encode.mjs"

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..")

// 与 backend/src/tray_icon.rs 保持一致
const BASE_TRAY_SIZE = 16
const ARTWORK = 1
const BADGE_CENTER = 23.7
const BADGE_RADIUS = 5.1
const BADGE_RING = 6.65

const STATES = [
  ["正常", [6, 193, 103]],
  ["接近上限", [217, 133, 0]],
  ["已耗尽", [220, 38, 38]],
  ["未知", [154, 160, 170]],
]
const SIZES = [16, 32]

const bases = SIZES.map((size) => {
  const file = `tray-base-${size}.rgba`
  const rgba = fs.readFileSync(path.join(root, "backend/icons", file))
  if (rgba.length !== size * size * 4) {
    throw new Error(`${file} 长度不对：${rgba.length}，请先跑 generate.mjs`)
  }
  return { size, rgba }
})

/** 双线性采样，预乘 alpha——和 Rust 侧同样的理由：直色插值会让绿边发黑。 */
function sample(base, u, v) {
  const n = base.size
  const fx = Math.min(Math.max(u * n - 0.5, 0), n - 1)
  const fy = Math.min(Math.max(v * n - 0.5, 0), n - 1)
  const x0 = Math.floor(fx), y0 = Math.floor(fy)
  const x1 = Math.min(x0 + 1, n - 1), y1 = Math.min(y0 + 1, n - 1)
  const tx = fx - x0, ty = fy - y0
  const out = [0, 0, 0, 0]
  for (const [x, y, w] of [
    [x0, y0, (1 - tx) * (1 - ty)], [x1, y0, tx * (1 - ty)],
    [x0, y1, (1 - tx) * ty], [x1, y1, tx * ty],
  ]) {
    const i = (y * n + x) * 4
    const a = base.rgba[i + 3] / 255
    for (let c = 0; c < 3; c += 1) out[c] += base.rgba[i + c] * a * w
    out[3] += base.rgba[i + 3] * w
  }
  return out
}

function renderTray(base, color) {
  const size = base.size
  const scale = size / 32
  const center = BADGE_CENTER * scale
  const radius = BADGE_RADIUS * scale
  const ring = BADGE_RING * scale
  const artwork = ARTWORK * size
  const out = Buffer.alloc(size * size * 4)

  for (let y = 0; y < size; y += 1) {
    for (let x = 0; x < size; x += 1) {
      const sum = [0, 0, 0, 0]
      for (let sy = 0; sy < 4; sy += 1) {
        for (let sx = 0; sx < 4; sx += 1) {
          const px = x + (sx + 0.5) / 4
          const py = y + (sy + 0.5) / 4
          const d = Math.hypot(px - center, py - center)
          const s = d <= radius ? [color[0], color[1], color[2], 255]
            : d <= ring ? [255, 255, 255, 255]
            : px < artwork && py < artwork ? sample(base, px / artwork, py / artwork)
            : [0, 0, 0, 0]
          for (let c = 0; c < 4; c += 1) sum[c] += s[c]
        }
      }
      const i = (y * size + x) * 4
      if (sum[3] > 0) {
        for (let c = 0; c < 3; c += 1) out[i + c] = Math.round((sum[c] * 255) / sum[3])
        out[i + 3] = Math.round(sum[3] / 16)
      }
    }
  }
  return out
}

const ZOOM = 6
const PAD = 14
const rowH = 32 * ZOOM
const W = PAD + SIZES.reduce((acc, size) => acc + size + PAD + size * ZOOM + PAD, 0)
const H = PAD + STATES.length * (rowH + PAD)
const canvas = Math.max(W, H)
const out = Buffer.alloc(canvas * canvas * 4)
// 中性灰底：托盘实际会落在浅色和深色任务栏上，灰底两头都不偏袒
for (let i = 0; i < canvas * canvas; i += 1) {
  out[i * 4] = 0x3a; out[i * 4 + 1] = 0x3d; out[i * 4 + 2] = 0x40; out[i * 4 + 3] = 255
}

const blend = (src, si, dx, dy) => {
  const a = src[si + 3] / 255
  const di = (dy * canvas + dx) * 4
  for (let c = 0; c < 3; c += 1) out[di + c] = Math.round(src[si + c] * a + out[di + c] * (1 - a))
}

STATES.forEach(([, color], row) => {
  const oy = PAD + row * (rowH + PAD)
  let x = PAD
  for (const base of bases) {
    const icon = renderTray(base, color)
    const size = base.size
    const inset = Math.round((rowH - size) / 2)
    for (let py = 0; py < size; py += 1) {
      for (let px = 0; px < size; px += 1) {
        blend(icon, (py * size + px) * 4, x + px, oy + inset + py)
      }
    }
    x += size + PAD
    for (let py = 0; py < size * ZOOM; py += 1) {
      for (let px = 0; px < size * ZOOM; px += 1) {
        blend(icon, (((py / ZOOM) | 0) * size + ((px / ZOOM) | 0)) * 4, x + px, oy + py)
      }
    }
    x += size * ZOOM + PAD
  }
})

const outDir = path.join(root, "output/icons")
fs.mkdirSync(outDir, { recursive: true })
const file = path.join(outDir, "tray-states.png")
fs.writeFileSync(file, encodePNG(canvas, out))
console.log(`已写出 ${path.relative(root, file)}  （${STATES.map((s) => s[0]).join(" / ")}；${SIZES.map((s) => `${s}px 左 1:1 右 ${ZOOM}×`).join("，")}）`)
