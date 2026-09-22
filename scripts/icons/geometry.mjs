/**
 * CC Usage 应用图标 · 几何与光栅化（方案 B2 · 绿底）
 *
 * 设计稿：prd/pencil-new.pen 画板「16B 应用图标 · 美化提案」B2 变体。
 * 构成：品牌绿圆角方底 + 居中深绿灵动岛胶囊 + 岛内白色额度水位条。
 *
 * 高清的关键不在于渲染得多精细，而在于两点：
 *   1. 每个尺寸都从下面这套比例独立光栅化，绝不从大图缩放下来；
 *   2. 光栅化前把所有矩形吸附到整数像素网格，让边缘正好压在像素边界上。
 * 前端 SVG 版本在 frontend/src/components/brand/AppIcon.tsx，两边参数必须一致。
 */

/** 方案 B2 色板。应用图标不随浅深主题变化，所以这里是固定色值，不接主题变量。 */
export const PALETTE = {
  plate: [0xff, 0xff, 0xff],
  island: [0x11, 0x11, 0x11],
  track: [0x3a, 0x3a, 0x3a],
  bar: [0x06, 0xc1, 0x67],
}

/** 圆角占边长的比例，Windows 11 任务栏图标的常见取值。 */
export const PLATE_RADIUS = 0.22

/**
 * 位图产物的尺寸分级。
 *
 * 这里出的图用在任务栏、托盘和安装包上，那是产品的门面，两条水位必须都在，
 * 再小也不降级成单条——单条版本只给主面板标题栏用，由 AppIcon.tsx 单独出。
 *
 * 分级按尺寸砍掉的只有轨道：轨道颜色是深灰叠深岛（#3a3a3a 上 #111111），
 * 32px 以下只剩 2px 高，两种深色挤成一团噪点，还会把绿色水位衬得发糊。
 * 去掉之后绿色双条在深岛上最干净；32px 起轨道才解析得动，照常保留。
 */
export function tierOf(size) {
  return size >= 32 ? "detail" : "clean"
}

/** 把矩形吸附到整数像素网格，并在容器内水平居中（居中偏移同样取整）。 */
function snapCentered(outer, w) {
  const width = Math.max(1, Math.round(w))
  return { x: Math.round((outer - width) / 2), w: width }
}

/**
 * 解析出某个尺寸下所有矩形的整数像素坐标。
 * 返回的每一项都是 { x, y, w, h, r, color }，按绘制顺序排列，后面的盖住前面的。
 *
 * tier 可以单独传：前端在高 DPI 下要按物理像素对齐，但分级必须跟着逻辑尺寸走，
 * 否则同一个 16px 图标会因为机器缩放比例不同而长得不一样。
 */
export function layout(size, tier = tierOf(size)) {
  const micro = tier === "single"
  const rects = []

  const island = snapCentered(size, size * (micro ? 0.78 : 0.72))
  const islandH = Math.max(2, Math.round(size * (micro ? 0.44 : 0.46)))
  const islandY = Math.round((size - islandH) / 2)
  rects.push({
    x: island.x, y: islandY, w: island.w, h: islandH,
    r: islandH / 2, color: PALETTE.island,
  })

  // 水位条的可用宽度 = 岛宽减去左右内边距，同样压在整数像素上。
  const padX = Math.round(size * (micro ? 0.1 : 0.11))
  const innerX = island.x + padX
  const innerW = island.w - padX * 2

  if (micro) {
    const barH = Math.max(2, Math.round(size * 0.16))
    const barW = Math.max(2, Math.round(size * 0.4))
    rects.push({
      x: innerX, y: Math.round(islandY + (islandH - barH) / 2),
      w: barW, h: barH, r: barH / 2, color: PALETTE.bar,
    })
    return rects
  }

  const detailed = tier === "detail"
  const barH = Math.max(2, Math.round(size * (detailed ? 0.085 : 0.11)))
  const gap = Math.max(1, Math.round(size * (detailed ? 0.075 : 0.08)))
  const stackH = barH * 2 + gap
  const firstY = Math.round(islandY + (islandH - stackH) / 2)
  // 两条对应 5h / 7d 两个额度窗口，长短不同是为了让图标一眼能看出是「用量」而不是菜单。
  const fills = detailed ? [0.38, 0.72] : [0.44, 0.835]

  for (let i = 0; i < 2; i += 1) {
    const y = firstY + i * (barH + gap)
    if (detailed) {
      rects.push({ x: innerX, y, w: innerW, h: barH, r: barH / 2, color: PALETTE.track })
    }
    const w = Math.max(barH, Math.round(innerW * fills[i]))
    rects.push({ x: innerX, y, w, h: barH, r: barH / 2, color: PALETTE.bar })
  }
  return rects
}

/** 圆角矩形的有符号距离场，<= 0 表示点落在形状内部。 */
function sdRoundRect(px, py, x, y, w, h, r) {
  const radius = Math.min(r, w / 2, h / 2)
  const dx = Math.abs(px - (x + w / 2)) - (w / 2 - radius)
  const dy = Math.abs(py - (y + h / 2)) - (h / 2 - radius)
  const ox = Math.max(dx, 0)
  const oy = Math.max(dy, 0)
  return Math.hypot(ox, oy) + Math.min(Math.max(dx, dy), 0) - radius
}

/**
 * 光栅化成 RGBA。用超采样做抗锯齿：小尺寸样本给得更密，
 * 因为那里每一个像素的权重都高，糊一个像素就看得出来。
 */
export function rasterize(size, tier = tierOf(size)) {
  const rects = layout(size, tier)
  const plateR = size * PLATE_RADIUS
  const ss = size <= 64 ? 8 : 4
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
          if (sdRoundRect(px, py, 0, 0, size, size, plateR) > 0) continue
          let color = PALETTE.plate
          for (const rect of rects) {
            if (sdRoundRect(px, py, rect.x, rect.y, rect.w, rect.h, rect.r) <= 0) color = rect.color
          }
          r += color[0]; g += color[1]; b += color[2]; a += 255
        }
      }
      const i = (y * size + x) * 4
      if (a > 0) {
        // 按覆盖率还原直色，避免边缘被透明样本拉暗（预乘色会让绿边发黑）。
        out[i] = Math.round(r / (a / 255))
        out[i + 1] = Math.round(g / (a / 255))
        out[i + 2] = Math.round(b / (a / 255))
        out[i + 3] = Math.round(a / samples)
      }
    }
  }
  return out
}

/**
 * 导出成 SVG。走的是和位图完全同一套 layout()，所以矢量版和 PNG 版不会漂移。
 * 坐标已经吸附到整数像素，浏览器按 1:1 渲染时边缘同样是实的。
 */
export function toSVG(size, tier = tierOf(size)) {
  const hex = (c) => "#" + c.map((v) => v.toString(16).padStart(2, "0")).join("")
  const rects = layout(size, tier)
    .map((r) => `  <rect x="${r.x}" y="${r.y}" width="${r.w}" height="${r.h}" rx="${r.r}" fill="${hex(r.color)}"/>`)
    .join("\n")
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}" role="img" aria-label="CC Usage">
  <rect width="${size}" height="${size}" rx="${size * PLATE_RADIUS}" fill="${hex(PALETTE.plate)}"/>
${rects}
</svg>
`
}

