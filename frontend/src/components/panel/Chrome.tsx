import { useEffect, useRef, useState } from "react"
import { ChevronDown } from "lucide-react"
import { getCurrentWindow } from "@tauri-apps/api/window"
import { api, isTauri } from "@/lib/api"
import { cn } from "@/lib/utils"
import { AppIcon } from "@/components/brand/AppIcon"
import { UpdateBadge } from "@/components/panel/UpdateBadge"
import { PlatformLogo } from "@/components/brand/PlatformLogo"
import { Segmented } from "@/components/ui/primitives"
import { PLATFORMS, platformAvailabilityText } from "@/lib/platforms"
import type { PlatformId } from "@/types"

/** 主面板窗口宽 1128；标题栏 40、工具栏 56、内容 padding 32（附录 A.4） */
export const PANEL_WIDTH = 1128
export const CONTENT_WIDTH = 1064

/** 窗口图标统一为 10px 大小、1px 线宽。 */
function CaptionIcon({ action, maximized }: { action: "minimize" | "maximize" | "close"; maximized: boolean }) {
  return (
    <svg aria-hidden="true" width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1">
      {action === "minimize" ? (
        <path d="M0 5.5h10" />
      ) : action === "close" ? (
        <path d="m.5.5 9 9m0-9-9 9" />
      ) : maximized ? (
        <>
          <path d="M2.5 2.5v-2h7v7h-2" />
          <rect x=".5" y="2.5" width="7" height="7" />
        </>
      ) : (
        <rect x=".5" y=".5" width="9" height="9" />
      )}
    </svg>
  )
}

export function TitleBar({ title = "CC Usage" }: { title?: string }) {
  const [maximized, setMaximized] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!isTauri) return
    let disposed = false
    let unlisten: (() => void) | undefined
    const win = getCurrentWindow()
    const sync = async () => {
      try {
        const value = await win.isMaximized()
        if (!disposed) setMaximized(value)
      } catch {
        if (!disposed) setError("无法读取窗口状态")
      }
    }
    void win.onResized(() => void sync()).then((off) => {
      if (disposed) off()
      else unlisten = off
    }).catch(() => {
      if (!disposed) setError("无法监听窗口状态")
    })
    void sync()
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [])

  const act = async (action: "minimize" | "maximize" | "close") => {
    if (!isTauri) return
    setError(null)
    try {
      const win = getCurrentWindow()
      if (action === "minimize") await win.minimize()
      else if (action === "maximize") {
        await win.toggleMaximize()
        setMaximized(await win.isMaximized())
      } else {
        // 关闭会销毁主面板 WebView；托盘、灵动岛与后台采集继续运行。
        await win.close()
      }
    } catch {
      setError("窗口操作失败，请重试")
    }
  }

  const actions = [
    { action: "minimize" as const, label: "最小化" },
    { action: "maximize" as const, label: maximized ? "还原" : "最大化" },
    { action: "close" as const, label: "关闭主面板" },
  ]

  return (
    <div
      className="flex h-10 w-full shrink-0 select-none items-center justify-between border-b bg-surface-2 pl-3.5"
      onPointerDown={(event) => {
        if (!isTauri || event.button !== 0 || (event.target as HTMLElement).closest("button")) return
        void api.mainDrag().catch(() => setError("无法拖动窗口"))
      }}
      onDoubleClick={(event) => {
        if ((event.target as HTMLElement).closest("button")) return
        void act("maximize")
      }}
    >
      <div className="flex items-center gap-2">
        <AppIcon size={16} />
        <span className="text-xs font-medium text-text-primary">{title}</span>
        {/* 有新版本时亮起的绿色更新按钮；点击直接进入安装（画布 17） */}
        <UpdateBadge />
      </div>
      <div className="flex h-full shrink-0 items-center">
        {error && <span role="alert" className="mr-2 text-[11px] text-danger">{error}</span>}
        {actions.map(({ action, label }) => (
          <button
            key={action}
            type="button"
            aria-label={label}
            title={isTauri ? label : `${label}（仅桌面端可用）`}
            disabled={!isTauri}
            onClick={() => void act(action)}
            className={cn("window-caption-button", action === "close" && "window-caption-button-close")}
          >
            <CaptionIcon action={action} maximized={maximized} />
          </button>
        ))}
      </div>
    </div>
  )
}

/**
 * 工具栏：左侧总览/设置导航，右侧平台图标选择器。
 *
 * 选择器**只在按平台组织内容的页面出现**，且两处绑定的是两个相互独立的值（§7.3）：
 *   - 总览          → 当前查看平台：决定额度、Token 汇总、趋势图与请求日志显示哪个平台
 *   - 设置 · 灵动岛  → 灵动岛显示平台：决定灵动岛显示哪个平台的数据
 *   - 设置 · 连接管理 → **不显示**，该表一次列出所有平台的连接，不按平台过滤
 *
 * 因为两处绑定不同的值，设置页的选择器要带「灵动岛显示」标签，避免被误认为同一个选择。
 */
export function Toolbar({
  tab,
  onTabChange,
  platform,
  onPlatformChange,
  /** 有值时在选择器左侧显示上下文标签 */
  contextLabel,
  /** 传 false 可隐藏选择器（设置 · 连接管理） */
  showPlatformSelector = true,
}: {
  tab: "overview" | "settings"
  onTabChange: (t: "overview" | "settings") => void
  platform?: PlatformId
  onPlatformChange?: (p: PlatformId) => void
  contextLabel?: string
  showPlatformSelector?: boolean
}) {
  return (
    <div className="flex h-14 w-full shrink-0 items-center justify-between border-b px-8">
      <Segmented
        items={[
          { value: "overview", label: "总览" },
          { value: "settings", label: "设置" },
        ]}
        value={tab}
        onChange={onTabChange}
      />
      {showPlatformSelector && platform && (
        // 平台选择器占右侧 60%（鼠鼠需求）：8 个图标 pill 横排，极窄时换行
        <div className="flex max-w-[60%] flex-wrap items-center justify-end gap-2.5">
          {contextLabel && (
            <span className="text-xs text-text-secondary">{contextLabel}</span>
          )}
          <PlatformSelector value={platform} onChange={onPlatformChange} platforms={PLATFORMS} showMore={false} />
        </div>
      )}
    </div>
  )
}

/**
 * 「更多」平台下拉 —— 总览页的 pill 选择器与设置页的下划线标签共用，
 * 触发按钮样式由调用方通过 triggerClassName 决定，交互（点外关闭、Esc、进出动画）一致。
 */
export function PlatformMoreMenu({
  platforms,
  onSelect,
  triggerClassName,
}: {
  platforms: readonly (typeof PLATFORMS)[number][]
  onSelect?: (p: PlatformId) => void
  triggerClassName?: string
}) {
  const [moreOpen, setMoreOpen] = useState(false)
  const [moreRendered, setMoreRendered] = useState(false)
  const menuRef = useRef<HTMLDivElement>(null)
  const closeTimerRef = useRef<number | null>(null)
  const openFrameRef = useRef<number | null>(null)

  const closeMore = () => {
    if (openFrameRef.current !== null) {
      window.cancelAnimationFrame(openFrameRef.current)
      openFrameRef.current = null
    }
    if (closeTimerRef.current !== null) window.clearTimeout(closeTimerRef.current)
    setMoreOpen(false)
    const closeDelay = window.matchMedia("(prefers-reduced-motion: reduce)").matches ? 0 : 160
    closeTimerRef.current = window.setTimeout(() => {
      setMoreRendered(false)
      closeTimerRef.current = null
    }, closeDelay)
  }

  const toggleMore = () => {
    if (moreOpen) {
      closeMore()
      return
    }
    if (closeTimerRef.current !== null) {
      window.clearTimeout(closeTimerRef.current)
      closeTimerRef.current = null
    }
    setMoreRendered(true)
    // 让隐藏态先完成挂载，再切换到显示态，确保 transition 有起始帧。
    openFrameRef.current = window.requestAnimationFrame(() => {
      openFrameRef.current = null
      setMoreOpen(true)
    })
  }

  useEffect(() => () => {
    if (closeTimerRef.current !== null) window.clearTimeout(closeTimerRef.current)
    if (openFrameRef.current !== null) window.cancelAnimationFrame(openFrameRef.current)
  }, [])

  useEffect(() => {
    if (!moreRendered) return
    const onPointerDown = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) closeMore()
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        closeMore()
        menuRef.current?.querySelector<HTMLButtonElement>("[aria-expanded]")?.focus()
      }
    }
    document.addEventListener("pointerdown", onPointerDown)
    document.addEventListener("keydown", onKeyDown)
    return () => {
      document.removeEventListener("pointerdown", onPointerDown)
      document.removeEventListener("keydown", onKeyDown)
    }
  }, [moreRendered])

  return (
    <div ref={menuRef} className="relative">
      <button
        type="button"
        aria-haspopup="menu"
        aria-expanded={moreOpen}
        onClick={toggleMore}
        className={cn(
          "flex cursor-pointer items-center gap-[3px] text-text-secondary focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent-blue",
          triggerClassName ?? "h-[26px] rounded-[7px] px-2 text-[11px] hover:bg-hover",
        )}
      >
        更多
        <ChevronDown className={cn("size-3 transition-transform", moreOpen && "rotate-180")} />
      </button>
      {moreRendered && (
        <div
          role="menu"
          aria-label="更多平台"
          data-state={moreOpen ? "open" : "closed"}
          className="motion-popover absolute right-0 top-full z-50 mt-1.5 w-44 rounded-[10px] border bg-surface p-1.5 shadow-lg"
        >
          {platforms.map((platform) => {
            const disabled = platform.availability === "not-integrated"
            return (
              <button
                key={platform.id}
                type="button"
                role="menuitem"
                disabled={disabled}
                onClick={() => {
                  onSelect?.(platform.id)
                  closeMore()
                }}
                className={cn(
                  "flex w-full items-center gap-2 rounded-[7px] px-2.5 py-2 text-left hover:bg-hover focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent-blue",
                  disabled && "cursor-not-allowed opacity-45",
                )}
              >
                <PlatformLogo platform={platform.id} className="opacity-70" />
                <span className="min-w-0 flex-1 text-xs text-text-primary">{platform.name}</span>
                <span className="text-[10px] text-text-tertiary">{platformAvailabilityText(platform)}</span>
              </button>
            )
          })}
        </div>
      )}
    </div>
  )
}

/**
 * 平台图标选择器 —— 紧凑横排、浅灰圆角容器、选中图标有独立高亮底色。
 * 未接入的平台禁用并说明，不能画成已连接。
 */
export function PlatformSelector({
  value,
  onChange,
  showMore = true,
  platforms = PLATFORMS.slice(0, 2),
}: {
  value: PlatformId
  onChange?: (p: PlatformId) => void
  showMore?: boolean
  platforms?: readonly (typeof PLATFORMS)[number][]
}) {
  const hiddenPlatforms = PLATFORMS.filter((platform) => !platforms.some((item) => item.id === platform.id))
  return (
    <div className="relative inline-flex items-center gap-0.5 rounded-[9px] bg-surface-3 p-0.5">
      {platforms.map((p) => {
        const active = p.id === value
        const disabled = p.availability === "not-integrated"
        return (
          <button
            key={p.id}
            type="button"
            title={p.name + (disabled ? "（待接入）" : "")}
            disabled={disabled}
            onClick={() => onChange?.(p.id)}
            className={cn(
              "grid h-[26px] w-[30px] place-items-center rounded-[7px]",
              active && "border bg-surface",
              disabled && "pointer-events-none opacity-35",
            )}
          >
            <PlatformLogo platform={p.id} className={active ? "" : "opacity-55"} />
          </button>
        )
      })}
      {showMore && hiddenPlatforms.length > 0 && <PlatformMoreMenu platforms={hiddenPlatforms} onSelect={onChange} />}
    </div>
  )
}
