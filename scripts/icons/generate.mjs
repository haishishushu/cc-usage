/**
 * 生成应用图标的全部产物。改了 geometry.mjs 之后跑：
 *
 *   node scripts/icons/generate.mjs
 *
 * 产物全部提交进仓库，构建时不重新生成，避免打包机器上的差异。
 */
import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"
import { rasterize, toSVG, PALETTE } from "./geometry.mjs"
import { encodePNG, encodeICO } from "./encode.mjs"

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..")
const iconsDir = path.join(root, "backend/icons")

/** ICO 里打包的尺寸。覆盖 100%–250% DPI 下任务栏、资源管理器和 Alt+Tab 会索取的每一档。 */
const ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]

/** 独立 PNG 产物：前三个是 tauri.conf.json 引用的，icon.png 供安装包与商店用。 */
const PNG_FILES = [
  ["32x32.png", 32],
  ["128x128.png", 128],
  ["128x128@2x.png", 256],
  ["icon.png", 512],
]

/**
 * 托盘底图。托盘图标要在运行时叠加额度状态角标，所以存成未编码的 RGBA，
 * 让 Rust 侧 include_bytes! 直接用——tauri 没开 image-png feature，解不了 PNG。
 */
// 覆盖 100%–250% 系统缩放下 SM_CXSMICON 索取的每一档（16/20/24/28/32/36/40）：
// 运行时按窗口缩放倍数取用同尺寸底图，壳层全程零缩放，托盘不再发糊。
// 每档的图案复杂度跟着 render() 的尺寸分级走，<32px 无轨道双粗条。
const TRAY_BASE_SIZES = [16, 20, 24, 28, 32, 36, 40]

const cache = new Map()
const render = (size, tier) => {
  const key = `${size}:${tier ?? "auto"}`
  if (!cache.has(key)) cache.set(key, rasterize(size, tier))
  return cache.get(key)
}

fs.mkdirSync(iconsDir, { recursive: true })
const written = []

for (const [name, size] of PNG_FILES) {
  const file = path.join(iconsDir, name)
  fs.writeFileSync(file, encodePNG(size, render(size)))
  written.push([name, `${size}x${size}`, fs.statSync(file).size])
}

const ico = encodeICO(ICO_SIZES.map((size) => ({ size, png: encodePNG(size, render(size)) })))
fs.writeFileSync(path.join(iconsDir, "icon.ico"), ico)
written.push(["icon.ico", ICO_SIZES.join(" / "), ico.length])

// favicon 走矢量：浏览器 tab 的实际渲染尺寸不固定，位图给多少档都会有缩放。
const favicon = toSVG(32)
const faviconFile = path.join(root, "frontend/public/favicon.svg")
fs.writeFileSync(faviconFile, favicon)
written.push(["favicon.svg", "32x32 矢量", Buffer.byteLength(favicon)])

// 托盘底图按最终输出尺寸直接渲染，图案复杂度跟着该尺寸的分级走：
// 托盘上常见 16–24 物理像素，轨道到那个尺寸必然糊，分级会自动砍成无轨道双粗条。
// 状态角标由 Rust 侧 badge() 在运行时按同一画布叠加。
for (const traySize of TRAY_BASE_SIZES) {
  const trayBase = render(traySize)
  fs.writeFileSync(path.join(iconsDir, `tray-base-${traySize}.rgba`), trayBase)
  written.push([`tray-base-${traySize}.rgba`, `${traySize}x${traySize} RGBA`, trayBase.length])
}

const hex = (c) => "#" + c.map((v) => v.toString(16).padStart(2, "0")).join("")
console.log(`应用图标 B2 · 绿底  底板 ${hex(PALETTE.plate)}  岛 ${hex(PALETTE.island)}  水位 ${hex(PALETTE.bar)}`)
for (const [name, spec, bytes] of written) {
  console.log(`  ${name.padEnd(18)} ${String(spec).padEnd(34)} ${(bytes / 1024).toFixed(1)} KB`)
}
