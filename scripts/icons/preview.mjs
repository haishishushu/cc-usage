/**
 * 把小尺寸图标按最近邻放大拼成一张对照图，用来肉眼检查像素对齐：
 *
 *   node scripts/icons/preview.mjs
 *
 * 看的是边缘有没有半像素灰边、水位条有没有被挤成 1px。
 * 产物写在 output/（已被 .gitignore 忽略），只服务于评审，不进仓库。
 */
import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"
import { rasterize } from "./geometry.mjs"
import { encodePNG } from "./encode.mjs"

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..")
const outDir = path.join(root, "output/icons")

const SIZES = [16, 20, 24, 32, 48, 64]
const ZOOM = 8
const PAD = 12
/** 中性灰。白色水位条和品牌绿底板在它上面都能被公平地看。 */
const BG = [0x3a, 0x3d, 0x40]

const cellH = Math.max(...SIZES) * ZOOM
const W = SIZES.reduce((acc, s) => acc + s * ZOOM + PAD, PAD)
const H = cellH + PAD * 2
const canvas = Math.max(W, H)
const out = Buffer.alloc(canvas * canvas * 4)
for (let i = 0; i < canvas * canvas; i += 1) {
  out[i * 4] = BG[0]; out[i * 4 + 1] = BG[1]; out[i * 4 + 2] = BG[2]; out[i * 4 + 3] = 255
}

let ox = PAD
for (const size of SIZES) {
  const src = rasterize(size)
  const oy = PAD + Math.round((cellH - size * ZOOM) / 2)
  for (let y = 0; y < size * ZOOM; y += 1) {
    for (let x = 0; x < size * ZOOM; x += 1) {
      const si = (((y / ZOOM) | 0) * size + ((x / ZOOM) | 0)) * 4
      const alpha = src[si + 3] / 255
      const di = ((oy + y) * canvas + ox + x) * 4
      for (let c = 0; c < 3; c += 1) {
        out[di + c] = Math.round(src[si + c] * alpha + out[di + c] * (1 - alpha))
      }
    }
  }
  ox += size * ZOOM + PAD
}

fs.mkdirSync(outDir, { recursive: true })
const file = path.join(outDir, "small-sizes-8x.png")
fs.writeFileSync(file, encodePNG(canvas, out))
console.log(`已写出 ${path.relative(root, file)}  （${SIZES.join(" / ")} px 放大 ${ZOOM}×）`)
