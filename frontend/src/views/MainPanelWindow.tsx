import { useContentMotion, useExitPresence } from "@/lib/motion"
import { useEffect, useId, useRef, useState } from "react"
import { api, isTauri, listenEvent, openUrl, type MainIntentDto } from "@/lib/api"
import { AppIcon } from "@/components/brand/AppIcon"
import { ToastProvider, ToastViewport } from "@/components/ui/Toast"
import { useSettings } from "@/lib/useSettings"
import { PANEL_WIDTH, TitleBar, Toolbar } from "@/components/panel/Chrome"
import { UpdateProvider } from "@/components/panel/UpdateContext"
import { UpdateDialog } from "@/components/panel/UpdateDialog"
import { OverviewView } from "./OverviewView"
import { SettingsView, type SettingsSection } from "./SettingsView"
import type { PlatformId } from "@/types"

/**
 * 主面板窗口 —— 宽 1128；关闭时由后端隐藏，后台采集继续运行。
 * 一级导航只有「总览 / 设置」两项。
 *
 * 「当前查看平台」与「灵动岛显示平台」是两个相互独立的值（§7.3）：
 * 在总览里切换平台不会回写灵动岛配置，反之亦然。工具栏的选择器按当前页面
 * 绑定其中之一，并用标签标明改的是哪一个。
 */
export function MainPanelWindow({
  variant = "auth",
  initialTab = "overview",
  /** true = 作为真实桌面窗口铺满，不要预览用的固定宽度与投影 */
  embedded = false,
}: {
  variant?: "auth" | "api"
  initialTab?: "overview" | "settings"
  embedded?: boolean
}) {
  const [tab, setTab] = useState<"overview" | "settings">(initialTab)
  const [section, setSection] = useState<SettingsSection>("connections")
  const [navigationRequest, setNavigationRequest] = useState(0)
  const [about, setAbout] = useState<string | null>(null)
  const [viewPlatform, setViewPlatform] = useState<PlatformId>("claude")
  const [preferredConnectionId, setPreferredConnectionId] = useState<string | null>(null)
  const [openAddRequest, setOpenAddRequest] = useState(0)
  const [panelActive, setPanelActive] = useState(true)
  // 页面包含 fixed 弹窗，避免祖先 transform 改变弹窗的定位参照。
  const pageMotion = useContentMotion(tab, 180, 0)
  const aboutPresence = useExitPresence(about)

  // 托盘菜单可以直接把主面板带到指定位置
  useEffect(() => {
    const offs: Array<() => void> = []
    let stopped = false
    const applyIntent = (intent: MainIntentDto | null) => {
      if (!intent || stopped) return
      if (intent.kind === "overview") {
        setAbout(null)
        setTab("overview")
        setViewPlatform(intent.settings.island_platform as PlatformId)
        setPreferredConnectionId(intent.settings.island_connection_id)
      } else if (intent.kind === "settings") {
        setAbout(null)
        setTab("settings")
        setSection(intent.section === "island" ? "island" : "connections")
        setNavigationRequest((value) => value + 1)
      } else {
        setAbout(intent.version)
      }
    }
    const consumeIntent = () => void api.takeMainIntent().then(applyIntent).catch(() => {})
    void listenEvent("main-intent", consumeIntent).then((un) => {
      if (stopped) un()
      else {
        offs.push(un)
        consumeIntent()
      }
    })
    return () => {
      stopped = true
      offs.forEach((f) => f())
    }
  }, [])

  useEffect(() => {
    if (!isTauri) return
    let stopped = false
    const offs: Array<() => void> = []
    const update = async () => {
      const { getCurrentWindow } = await import("@tauri-apps/api/window")
      const current = getCurrentWindow()
      const [visible, minimized] = await Promise.all([current.isVisible(), current.isMinimized()])
      if (!stopped) setPanelActive(visible && !minimized && document.visibilityState !== "hidden")
    }
    const onVisibility = () => void update()
    document.addEventListener("visibilitychange", onVisibility)
    void import("@tauri-apps/api/window").then(async ({ getCurrentWindow }) => {
      if (stopped) return
      const current = getCurrentWindow()
      const listeners = await Promise.all([
        current.onResized(() => void update()),
        current.onFocusChanged(() => void update()),
      ])
      if (stopped) listeners.forEach((off) => off())
      else offs.push(...listeners)
      await update()
    }).catch(() => {})
    return () => {
      stopped = true
      document.removeEventListener("visibilitychange", onVisibility)
      offs.forEach((off) => off())
    }
  }, [])

  // 两个独立的平台选择，互不回写
  // 灵动岛平台是持久化设置，与托盘子菜单同一个值；不在此处另存一份
  const cfg = useSettings()
  const islandPlatform = cfg.settings.island_platform as PlatformId

  const onIslandSection = tab === "settings" && section === "island"

  return (
    <ToastProvider>
    <UpdateProvider>
    <div
      className={
        embedded
          ? "relative flex h-full min-h-0 w-full flex-col overflow-hidden bg-bg"
          : "relative flex flex-col overflow-hidden rounded-xl border bg-bg shadow-window"
      }
      style={embedded ? undefined : { width: PANEL_WIDTH, maxWidth: "100%" }}
    >
      <TitleBar />
      <Toolbar
        tab={tab}
        onTabChange={setTab}
        platform={onIslandSection ? islandPlatform : viewPlatform}
        onPlatformChange={
          onIslandSection
            ? (p) => void cfg.setIslandPlatform(p)
            : setViewPlatform
        }
        contextLabel={onIslandSection ? "灵动岛显示" : undefined}
        // 连接管理一次列出所有平台的连接，不按平台过滤，因此不显示选择器
        showPlatformSelector={tab === "overview"}
      />
      {/* 标题栏与工具栏不参与滚动；桌面端只有下方内容区滚动。 */}
      <div data-panel-scroll className={embedded ? "min-h-0 min-w-0 flex-1 overflow-auto" : tab === "settings" ? "max-h-[calc(100dvh-210px)] min-h-[360px] overflow-auto" : undefined}>
        <div ref={pageMotion}>
        {tab === "overview" ? (
          <OverviewView
            active={panelActive}
            platform={viewPlatform}
            variant={variant}
            preferredConnectionId={preferredConnectionId}
            onAddConnection={() => {
              setSection("connections")
              setTab("settings")
              setOpenAddRequest((value) => value + 1)
            }}
          />
        ) : (
          <SettingsView
            section={section}
            navigationRequest={navigationRequest}
            islandPlatform={islandPlatform}
            openAddRequest={openAddRequest}
          />
        )}
        </div>
      </div>

      {aboutPresence.rendered && <AboutDialog version={aboutPresence.rendered} exiting={aboutPresence.exiting} onClose={() => setAbout(null)} />}
      <UpdateDialog />
      {/* 视口放在面板根节点内：标题栏 40 + 工具栏 56 = 96，正好落在内容区顶部居中 */}
      <ToastViewport />
    </div>
    </UpdateProvider>
    </ToastProvider>
  )
}

/** 关于（§2.6）：版本号与仓库链接 */
function AboutDialog({ version, onClose, exiting }: { version: string; onClose: () => void; exiting: boolean }) {
  const repo = "https://github.com/haishishushu/cc-usage"
  const titleId = useId()
  const dialogRef = useRef<HTMLDivElement>(null)
  const closeRef = useRef(onClose)
  closeRef.current = onClose
  useEffect(() => {
    if (exiting) return
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null
    const dialog = dialogRef.current
    if (!dialog) return
    const items = () => Array.from(dialog.querySelectorAll<HTMLElement>(
      'button:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])',
    ))
    ;(items()[0] ?? dialog).focus()
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault()
        closeRef.current()
        return
      }
      if (event.key !== "Tab") return
      const focusable = items()
      if (!focusable.length) return event.preventDefault()
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault()
        last.focus()
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault()
        first.focus()
      }
    }
    document.addEventListener("keydown", onKeyDown)
    return () => {
      document.removeEventListener("keydown", onKeyDown)
      previous?.focus()
    }
  }, [exiting])
  return (
    <div inert={exiting} data-state={exiting ? "closed" : "open"} className="motion-overlay absolute inset-0 z-20 grid place-items-center bg-black/25 p-8">
      <div ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby={titleId} tabIndex={-1} className="motion-dialog flex w-[380px] max-w-full flex-col gap-4 rounded-xl border bg-surface p-5 shadow-dialog">
        <div className="flex items-center gap-2.5">
          <AppIcon size={32} />
          <div className="flex flex-col">
            <span id={titleId} className="text-[13px] font-semibold text-text-primary">CC Usage</span>
            <span className="tnum font-mono text-[11px] text-text-tertiary">v{version}</span>
          </div>
        </div>
        <p className="text-[11px] leading-[1.6] text-text-tertiary">
          本机只读的 AI 用量监控。统计来自本地会话记录与只读接口查询，凭证只保存在本机。
        </p>
        <button
          type="button"
          onClick={() => void openUrl(repo)}
          className="self-start text-[11px] text-accent-blue underline"
        >
          {repo}
        </button>
        <div className="flex justify-end">
          <button
            type="button"
            onClick={onClose}
            className="rounded-[8px] border bg-surface px-4 py-2 text-xs text-text-primary"
          >
            关闭
          </button>
        </div>
      </div>
    </div>
  )
}
