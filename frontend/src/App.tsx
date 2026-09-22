import { resolveDarkTheme } from "@/lib/displayPreferences"
import { lazy, Suspense, useEffect, useState } from "react"
import { Moon, Sun } from "lucide-react"
import { cn } from "@/lib/utils"
import { AppIcon } from "@/components/brand/AppIcon"
import { isTauri } from "@/lib/api"
import { useSettings } from "@/lib/useSettings"
const ContextMenuWindow = lazy(() => import("@/views/ContextMenuWindow").then((module) => ({ default: module.ContextMenuWindow })))
const TraySummaryWindow = lazy(() => import("@/views/TraySummaryWindow").then((module) => ({ default: module.TraySummaryWindow })))
const IslandWindow = lazy(() => import("@/views/IslandWindow").then((module) => ({ default: module.IslandWindow })))
const MainPanelWindow = lazy(() => import("@/views/MainPanelWindow").then((module) => ({ default: module.MainPanelWindow })))
const Showcase = lazy(() => import("@/views/Showcase"))

/**
 * 应用外壳。
 *
 * Tauri 窗口按用途分离，用查询参数区分：
 *   ?window=island  → 灵动岛（常驻，只加载摘要）
 *   ?window=menu    → 按需创建的自绘菜单
 *   默认             → 主面板
 * 浏览器里额外提供一个 showcase 视图，逐屏对照 pencil-new.pen 的画布。
 */

type View = "panel" | "island" | "showcase" | "menu" | "tray-summary"

function useTheme() {
  const cfg = useSettings()
  const [previewDark, setPreviewDark] = useState(false)
  const [systemDark, setSystemDark] = useState(() => window.matchMedia("(prefers-color-scheme: dark)").matches)
  useEffect(() => {
    const query = window.matchMedia("(prefers-color-scheme: dark)")
    const changed = () => setSystemDark(query.matches)
    query.addEventListener("change", changed)
    changed()
    return () => query.removeEventListener("change", changed)
  }, [])
  const dark = isTauri ? resolveDarkTheme(cfg.settings.theme, systemDark) : previewDark
  useEffect(() => {
    const root = document.documentElement
    const changed = root.classList.contains("dark") !== dark
    if (changed) root.classList.add("theme-transition")
    root.classList.toggle("dark", dark)
    const timer = window.setTimeout(() => root.classList.remove("theme-transition"), 200)
    return () => { window.clearTimeout(timer); root.classList.remove("theme-transition") }
  }, [dark])
  return { dark, setDark: setPreviewDark }
}

function AppContent() {
  const { dark, setDark } = useTheme()
  const params = new URLSearchParams(window.location.search)
  const initial = (params.get("window") as View) ?? "panel"
  const [view, setView] = useState<View>(initial)
  const [variant, setVariant] = useState<"auth" | "api">("auth")

  /*
   * 桌面端（Tauri）直接渲染窗口本体，**不套预览外壳**。
   * 顶部那条「前端预览 / 主面板 / 灵动岛 / 画布对照」只在浏览器里用于对照设计，
   * 放进真实窗口会把灵动岛挤没。
   */
  if (initial === "menu") return <ContextMenuWindow />
  if (initial === "tray-summary") return <TraySummaryWindow />

  if (isTauri) {
    if (initial === "island") {
      // 灵动岛窗口 400×92、无边框、透明：背景必须透明，由岛自己画圆角与阴影
      return (
        <div className="flex h-full w-full items-start justify-start overflow-hidden bg-transparent">
          <IslandWindow />
        </div>
      )
    }
    // 主面板窗口：铺满整个窗口，不要预览里的固定宽度与外边距
    return (
      <div className="h-dvh w-full overflow-hidden bg-bg">
        <MainPanelWindow variant={variant} embedded />
      </div>
    )
  }

  return (
    <div className="min-h-full w-full bg-surface-2">
      <header className="sticky top-0 z-20 flex w-full items-center gap-3 border-b bg-surface px-5 py-2.5">
        <AppIcon size={18} />
        <span className="text-sm font-semibold text-text-primary">CC Usage</span>
        <span className="text-[11px] text-text-tertiary">前端预览 · 数据均为设计示例</span>
        <span className="flex-1" />
        {(
          [
            ["panel", "主面板"],
            ["island", "灵动岛"],
            ["showcase", "画布对照"],
          ] as [View, string][]
        ).map(([v, l]) => (
          <button
            key={v}
            type="button"
            onClick={() => setView(v)}
            className={cn(
              "rounded-[8px] border px-3 py-1.5 text-xs",
              view === v ? "border-accent-blue bg-accent-blue-soft font-semibold text-accent-blue" : "bg-surface text-text-secondary",
            )}
          >
            {l}
          </button>
        ))}
        {view === "panel" && (
          <button
            type="button"
            onClick={() => setVariant(variant === "auth" ? "api" : "auth")}
            className="rounded-[8px] border bg-surface px-3 py-1.5 text-xs text-text-secondary"
          >
            {variant === "auth" ? "官方账号" : "API 连接"}
          </button>
        )}
        <button
          type="button"
          onClick={() => setDark(!dark)}
          className="grid size-8 place-items-center rounded-[8px] border bg-surface"
          aria-label="切换主题"
        >
          {dark ? <Sun className="size-3.5 text-text-secondary" /> : <Moon className="size-3.5 text-text-secondary" />}
        </button>
      </header>

      <main className="flex w-full justify-center p-8">
        {view === "panel" && <MainPanelWindow variant={variant} />}
        {view === "island" && (
          <div className="flex min-h-[70vh] items-start justify-center pt-20">
            <IslandWindow />
          </div>
        )}
        {view === "showcase" && <Showcase />}
      </main>
    </div>
  )
}

export default function App() {
  return <Suspense fallback={null}><AppContent /></Suspense>
}
