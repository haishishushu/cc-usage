/**
 * 一次性对照：托盘图标「有 / 无 1px 浅灰描边」在浅色与深色任务栏上的观感。
 *
 *   node scripts/icons/compare-border.mjs
 *
 * 只写 output/icons/border-compare.png，不改任何图标产物。看完可删。
 * 几何直接复用 geometry.mjs 的 layout()，描边逻辑照抄删除前的那行判断。
 */
import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"
import { layout, PALETTE, PLATE_RADIUS } from "./geometry.mjs"
import { encodePNG } from "./encode.mjs"

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..")

const BORDER = [0xe2, 0xe2, 0xe2]
// Windows 11 任务栏的近似底色（实际带亚克力模糊，会受壁纸影响，这里取常见值）
const LIGHT_BAR = [0xf3, 0xf3, 0xf3]
const DARK_BAR = [0x20, 0x20, 0x20]

function sdRoundRect(px, py, x, y, w, h, r) {
  const radius = Math.min(r, w / 2, h / 2)
  const dx = Math.abs(px - (x + w / 2)) - (w / 2 - radius)
  const dy = Math.abs(py - (y + h / 2)) - (h / 2 - radius)
  const ox = Math.max(dx, 0)
  const oy = Math.max(dy, 0)
  return Math.hypot(ox, oy) + Math.min(Math.max(dx, dy), 0) - radius
}

/** 与 geometry.mjs 的 rasterize() 同源，只多一个描边开关。 */
function raster(size, withBorder) {
  const rects = layout(size)
  const plateR = size * PLATE_RADIUS
  const ss = 8
  const step = 1 / ss
  const samples = ss * ss
  const out = Buffer.alloc(size * size * 4)

  for (let y = 0; y < size; y += 1) {
    for (let x = 0; x < size; x += 1) {
      let r = 0, g = 0, b = 0, a = 0
      for (let sy = 0; sy < ss; sy += 1) {
        for (let sx = 0; sx < ss; sx += 1) {
          const px = x + (sx + 0.5) * step
          const py = y + (sy + 0.5) * step
          const d = sdRoundRect(px, py, 0, 0, size, size, plateR)
          if (d > 0) continue
          let color = PALETTE.plate
          if (withBorder && d > -0.75) color = BORDER
          for (const rect of rects) {
            if (sdRoundRect(px, py, rect.x, rect.y, rect.w, rect.h, rect.r) <= 0) color = rect.color
          }
          r += color[0]; g += color[1]; b += color[2]; a += 255
        }
      }
      const i = (y * size + x) * 4
      if (a > 0) {
        out[i] = Math.round(r / (a / 255))
        out[i + 1] = Math.round(g / (a / 255))
        out[i + 2] = Math.round(b / (a / 255))
        out[i + 3] = Math.round(a / samples)
      }
    }
  }
  return out
}

const COLS = [
  { size: 16, border: false },
  { size: 16, border: true },
  { size: 24, border: false },
  { size: 24, border: true },
]
const ZOOM = 10
const PAD = 20
const MAX_CELL = Math.max(...COLS.map((c) => c.size)) * ZOOM
const ROW_H = PAD + MAX_CELL + 14 + 24 + PAD
const W = PAD + COLS.reduce((acc, c) => acc + c.size * ZOOM + PAD, 0)
const H = ROW_H * 2

const out = Buffer.alloc(W * H * 4)
for (let y = 0; y < H; y += 1) {
  const bg = y < ROW_H ? LIGHT_BAR : DARK_BAR
  for (let x = 0; x < W; x += 1) {
    const i = (y * W + x) * 4
    out[i] = bg[0]; out[i + 1] = bg[1]; out[i + 2] = bg[2]; out[i + 3] = 255
  }
}

const blend = (src, si, dx, dy) => {
  if (dx < 0 || dy < 0 || dx >= W || dy >= H) return
  const a = src[si + 3] / 255
  const di = (dy * W + dx) * 4
  for (let c = 0; c < 3; c += 1) out[di + c] = Math.round(src[si + c] * a + out[di + c] * (1 - a))
}

for (let row = 0; row < 2; row += 1) {
  const top = row * ROW_H + PAD
  let x = PAD
  for (const col of COLS) {
    const icon = raster(col.size, col.border)
    const zoomW = col.size * ZOOM
    // 放大图：顶部对齐，看边缘像素
    for (let py = 0; py < zoomW; py += 1) {
      for (let px = 0; px < zoomW; px += 1) {
        blend(icon, (((py / ZOOM) | 0) * col.size + ((px / ZOOM) | 0)) * 4, x + px, top + py)
      }
    }
    // 1:1 实际大小，放在放大图下方居中
    const oneX = x + Math.round((zoomW - col.size) / 2)
    const oneY = top + MAX_CELL + 14
    for (let py = 0; py < col.size; py += 1) {
      for (let px = 0; px < col.size; px += 1) {
        blend(icon, (py * col.size + px) * 4, oneX + px, oneY + py)
      }
    }
    x += zoomW + PAD
  }
}

const outDir = path.join(root, "output/icons")
fs.mkdirSync(outDir, { recursive: true })
const file = path.join(outDir, "border-compare.png")
fs.writeFileSync(file, encodePNG2(W, H, out))
console.log(`已写出 ${path.relative(root, file)}`)
console.log("上行 = 浅色任务栏 #f3f3f3，下行 = 深色任务栏 #202020")
console.log("每行从左到右：16px 无描边 / 16px 有描边 / 24px 无描边 / 24px 有描边（大图 10×，下方 1:1）")

/** encode.mjs 的 encodePNG 只收正方形边长，这里要非正方形，所以本地补一个。 */
function encodePNG2(w, h, rgba) {
  if (w === h) return encodePNG(w, rgba)
  const square = Math.max(w, h)
  const padded = Buffer.alloc(square * square * 4)
  for (let y = 0; y < h; y += 1) {
    rgba.copy(padded, y * square * 4, y * w * 4, (y * w + w) * 4)
  }
  return encodePNG(square, padded)
}
