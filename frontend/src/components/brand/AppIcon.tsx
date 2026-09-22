import { useEffect, useState } from "react"

import { cn } from "@/lib/utils"

/**
 * 应用图标 CC Usage —— 方案 B2 · 绿底。
 * 设计稿：prd/pencil-new.pen 画板「16B 应用图标 · 美化提案」B2 变体，
 * 在主面板里的效果见画板「16C 应用图标 · 主面板显示效果」。
 *
 * 构成：品牌绿圆角方底 + 居中深绿灵动岛胶囊 + 岛内白色额度水位条（长短对应 5h / 7d）。
 * 应用图标不随浅深主题变化，两套主题共用同一枚，这是桌面应用的通行做法。
 *
 * 几何与 scripts/icons/geometry.mjs 同源（位图产物由那份脚本生成），改一边必须同步另一边。
 */

const PLATE = "#FFFFFF"
const ISLAND = "#111111"
const TRACK = "#3A3A3A"
const BAR = "#06C167"
const PLATE_RADIUS = 0.22

type Tier = "single" | "dual" | "detail"

interface IconRect {
  x: number
  y: number
  w: number
  h: number
  r: number
  fill: string
}

/**
 * 尺寸分级。
 *
 * 两条水位在所有尺寸都保留（与位图产物一致，见 scripts/icons/geometry.mjs），
 * 分级只砍轨道：轨道是深灰叠深岛，32 物理像素以下只剩 1–2px，糊成噪点。
 *
 * 分级只看逻辑尺寸，不看物理像素——否则同一枚 16px 图标会因为机器缩放比例不同
 * 而长得不一样，两台电脑摆在一起就成了两个图标。
 */
function tierOf(logicalSize: number): Tier {
  return logicalSize >= 32 ? "detail" : "dual"
}

/** 按给定的像素边长解析所有矩形，坐标全部吸附到整数像素。 */
function layout(px: number, tier: Tier): IconRect[] {
  const micro = tier === "single"
  const detailed = tier === "detail"
  const rects: IconRect[] = []

  const islandW = Math.max(1, Math.round(px * (micro ? 0.78 : 0.72)))
  const islandX = Math.round((px - islandW) / 2)
  const islandH = Math.max(2, Math.round(px * (micro ? 0.44 : 0.46)))
  const islandY = Math.round((px - islandH) / 2)
  rects.push({ x: islandX, y: islandY, w: islandW, h: islandH, r: islandH / 2, fill: ISLAND })

  const padX = Math.round(px * (micro ? 0.1 : 0.11))
  const innerX = islandX + padX
  const innerW = islandW - padX * 2

  if (micro) {
    const barH = Math.max(2, Math.round(px * 0.16))
    const barW = Math.max(2, Math.round(px * 0.4))
    rects.push({
      x: innerX,
      y: Math.round(islandY + (islandH - barH) / 2),
      w: barW,
      h: barH,
      r: barH / 2,
      fill: BAR,
    })
    return rects
  }

  const barH = Math.max(2, Math.round(px * (detailed ? 0.085 : 0.11)))
  const gap = Math.max(1, Math.round(px * (detailed ? 0.075 : 0.08)))
  const firstY = Math.round(islandY + (islandH - (barH * 2 + gap)) / 2)
  const fills = detailed ? [0.38, 0.72] : [0.44, 0.835]

  for (let i = 0; i < 2; i += 1) {
    const y = firstY + i * (barH + gap)
    if (detailed) {
      rects.push({ x: innerX, y, w: innerW, h: barH, r: barH / 2, fill: TRACK })
    }
    rects.push({
      x: innerX,
      y,
      w: Math.max(barH, Math.round(innerW * fills[i])),
      h: barH,
      r: barH / 2,
      fill: BAR,
    })
  }
  return rects
}

/**
 * 跟随显示器缩放比例。
 *
 * Windows 笔记本默认就是 125% 或 150%，此时一个逻辑像素等于 1.25 / 1.5 个物理像素，
 * 把矩形对齐到整数逻辑像素等于对齐到半个物理像素，边缘照样会被抗锯齿抹成灰边。
 * 所以按物理像素布局、再用 viewBox 缩回逻辑尺寸，让每条边都压在真实的像素线上。
 */
function useDevicePixelRatio() {
  const read = () => (typeof window === "undefined" ? 1 : window.devicePixelRatio || 1)
  const [ratio, setRatio] = useState(read)

  useEffect(() => {
    if (typeof window === "undefined") return
    // 拖到另一块不同缩放的显示器上时，这条 query 会失配并触发 change。
    const query = window.matchMedia(`(resolution: ${ratio}dppx)`)
    const update = () => setRatio(window.devicePixelRatio || 1)
    query.addEventListener("change", update)
    return () => query.removeEventListener("change", update)
  }, [ratio])

  return ratio
}

export interface AppIconProps {
  size?: number
  className?: string
}

export function AppIcon({ size = 16, className }: AppIconProps) {
  const ratio = useDevicePixelRatio()
  const physical = Math.max(1, Math.round(size * ratio))

  return (
    <svg
      width={size}
      height={size}
      viewBox={`0 0 ${physical} ${physical}`}
      className={cn("shrink-0", className)}
      role="img"
      aria-label="CC Usage"
    >
      <rect width={physical} height={physical} rx={physical * PLATE_RADIUS} fill={PLATE} />
      {layout(physical, tierOf(size)).map((rect, index) => (
        <rect
          key={index}
          x={rect.x}
          y={rect.y}
          width={rect.w}
          height={rect.h}
          rx={rect.r}
          fill={rect.fill}
        />
      ))}
    </svg>
  )
}
