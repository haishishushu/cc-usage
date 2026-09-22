import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react"
import { AppWindow, BellOff, Check, ChevronRight, Info, LayoutGrid, LogOut, Move, PanelBottom, PanelLeft, PanelRight, PanelTop, PanelTopOpen, Pin, RefreshCw, Settings } from "lucide-react"
import { api, isTauri, listenEvent } from "@/lib/api"
import { useSettings } from "@/lib/useSettings"
import { useConnections } from "@/lib/useConnections"
import { cn } from "@/lib/utils"
import { PlatformLogo } from "@/components/brand/PlatformLogo"

export function ContextMenuWindow() {
  const [islandMenu, setIslandMenu] = useState(() => new URLSearchParams(window.location.search).get("source") === "island")
  const [reopenRequest, setReopenRequest] = useState(0)
  const { settings } = useSettings()
  const { connections, loading, error: connectionError } = useConnections()
  const [page, setPage] = useState<"root" | "connections" | "position">("root")
  const [busy, setBusy] = useState(false)
  const [refreshing, setRefreshing] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const ref = useRef<HTMLDivElement>(null)
  const subRef = useRef<HTMLDivElement>(null)
  const closeTimer = useRef<number | undefined>(undefined)
  const [layout, setLayout] = useState<Awaited<ReturnType<typeof api.menuFit>> | null>(null)
  const [subTop, setSubTop] = useState(6)
  const cancelClose = () => window.clearTimeout(closeTimer.current)
  const closeLater = () => { cancelClose(); closeTimer.current = window.setTimeout(() => setPage("root"), 180) }
  const openSub = (next: "connections" | "position") => { cancelClose(); setPage(next) }
  useEffect(() => () => window.clearTimeout(closeTimer.current), [])
  useEffect(() => {
    let disposed = false
    const offs: Array<() => void> = []
    const refresh = () => { void api.menuRefreshing().then((v) => { if (!disposed) setRefreshing(v) }).catch(() => {}) }
    refresh()
    for (const name of ["menu-state-changed", "menu-reopen"]) {
      void listenEvent<boolean>(name, (island) => {
        refresh()
        if (name === "menu-reopen") {
          setIslandMenu(island)
          setPage("root")
          setBusy(false)
          setError(null)
          setReopenRequest((value) => value + 1)
        }
      })
        .then((off) => { if (disposed) off(); else offs.push(off) })
    }
    return () => { disposed = true; offs.forEach((off) => off()) }
  }, [])
  useLayoutEffect(() => {
    const element = ref.current
    if (!element) return
    let disposed = false
    const fit = () => {
      if (isTauri) void api.menuFit(Math.ceil(element.scrollHeight + 2))
        .then((next) => { if (!disposed) setLayout(next) })
        .catch((reason) => { if (!disposed) setError(String(reason)) })
    }
    const observer = new ResizeObserver(fit)
    observer.observe(element)
    fit()
    element.focus()
    return () => { disposed = true; observer.disconnect() }
  }, [islandMenu, reopenRequest])
  useLayoutEffect(() => {
    if (layout && isTauri) {
      ref.current?.focus()
      void api.menuShow().catch((reason) => setError(String(reason)))
    }
  }, [layout])
  useLayoutEffect(() => {
    if (page === "root") return
    const place = () => {
      const trigger = ref.current?.querySelector<HTMLElement>(`[data-submenu="${page}"]`)
      const panel = subRef.current
      if (trigger && panel) setSubTop(Math.max(6, Math.min(trigger.getBoundingClientRect().top, window.innerHeight - panel.offsetHeight - 6)))
    }
    place()
    const observer = new ResizeObserver(place)
    if (subRef.current) observer.observe(subRef.current)
    window.addEventListener("resize", place)
    return () => { observer.disconnect(); window.removeEventListener("resize", place) }
  }, [page, layout])
  const act = async (action: () => Promise<unknown>) => {
    if (busy) return
    setBusy(true)
    setError(null)
    try { await action() } catch (reason) { setError(String(reason)); setBusy(false) }
  }
  const icons: Partial<Record<string, typeof RefreshCw>> = {
    "打开主面板": PanelTopOpen, "立即刷新": RefreshCw, "刷新中…": RefreshCw,
    "显示灵动岛": PanelTop, "切换连接": LayoutGrid, "显示位置": Move, "始终置顶": Pin, "免打扰": BellOff,
    "重置窗口位置": Move, "设置": Settings, "关于 CC Usage": Info, "退出": LogOut,
  }
  const row = (label: string, action: () => void, options: { checked?: boolean; sub?: "connections" | "position"; radio?: boolean; disabled?: boolean; danger?: boolean; platform?: "claude" | "codex"; itemIcon?: typeof RefreshCw } = {}) => {
    const Icon = icons[label]
    const ItemIcon = options.itemIcon
    return (
    <button key={label} type="button" role={options.checked !== undefined ? (options.radio ? "menuitemradio" : "menuitemcheckbox") : "menuitem"}
      aria-checked={options.checked} aria-haspopup={options.sub ? "menu" : undefined}
      aria-expanded={options.sub ? page === options.sub : undefined} aria-controls={options.sub && page === options.sub ? "island-submenu" : undefined}
      data-submenu={options.sub} disabled={busy || options.disabled} onClick={action}
      onPointerEnter={(event) => {
        if (event.currentTarget.closest("#island-submenu")) cancelClose()
        else if (!busy && !options.disabled) { if (options.sub) openSub(options.sub); else closeLater() }
      }}
      className={cn("flex h-8 shrink-0 w-full items-center gap-2.5 rounded-md px-2 text-left text-[13px] outline-none transition-colors hover:bg-surface-3 focus-visible:bg-surface-3 focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-border-strong disabled:pointer-events-none", options.disabled && "opacity-[0.42]", options.sub === page && "bg-surface-3", options.danger ? "text-danger" : "text-text-primary")}
    >
      <span className="grid size-4 shrink-0 place-items-center">{Icon ? <Icon className={cn("size-3.5", !options.danger && "text-text-secondary", refreshing && label === "刷新中…" && "animate-spin")} /> : options.checked ? <Check className="size-3.5 text-text-primary" /> : null}</span>
      {options.platform && <PlatformLogo platform={options.platform} size={14} />}
      {ItemIcon && <ItemIcon aria-hidden="true" className="size-3.5 shrink-0 text-text-secondary" />}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {Icon && options.checked !== undefined && <span className="grid size-3.5 shrink-0 place-items-center">{options.checked && <Check className="size-3.5 text-text-primary" />}</span>}
      {options.sub && <ChevronRight className="size-3.5 text-text-primary" />}
    </button>
    )
  }
  const command = (id: string) => () => { void act(() => api.menuAction(id)) }
  const separator = <div role="separator" className="mx-2 my-1 border-t" />
  const keyDown = (event: KeyboardEvent<HTMLDivElement>, submenu = false) => {
    if (event.key === "Escape" || (submenu && event.key === "ArrowLeft")) {
      event.preventDefault()
      cancelClose()
      if (page !== "root") {
        ref.current?.querySelector<HTMLButtonElement>(`[data-submenu="${page}"]`)?.focus()
        setPage("root")
      } else void api.menuClose()
    }
    if (!submenu && event.key === "ArrowRight") {
      const target = (document.activeElement as HTMLElement)?.dataset.submenu
      if (target === "connections" || target === "position") {
        event.preventDefault(); openSub(target)
        window.requestAnimationFrame(() => subRef.current?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus())
      }
    }
    if (["ArrowDown", "ArrowUp", "Home", "End", "Tab"].includes(event.key)) {
      event.preventDefault()
      const buttons = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>("button:not(:disabled)"))
      const index = buttons.indexOf(document.activeElement as HTMLButtonElement)
      const step = event.key === "ArrowUp" || (event.key === "Tab" && event.shiftKey) ? -1 : 1
      const next = event.key === "Home" ? 0 : event.key === "End" ? buttons.length - 1 : index < 0 ? (step > 0 ? 0 : buttons.length - 1) : (index + step + buttons.length) % buttons.length
      buttons[next]?.focus()
    }
  }
  const rootX = layout?.root_x ?? 6
  const rootY = layout?.root_y ?? 6
  const rootWidth = layout?.root_width ?? 268
  const subWidth = layout?.sub_width ?? 300
  const panelClass = "absolute outline-none flex flex-col gap-px max-h-[calc(100dvh-12px)] overflow-y-auto rounded-lg border bg-surface p-1 shadow-popover"
  return (
    <div className="fixed inset-0" onContextMenu={(event) => event.preventDefault()}
      onPointerDown={(event) => { if (event.target === event.currentTarget) void api.menuClose() }}>
      <div ref={ref} tabIndex={-1} role="menu" aria-label={islandMenu ? "灵动岛菜单" : "托盘菜单"}
        className={panelClass} style={{ left: rootX, top: rootY, width: rootWidth, visibility: isTauri && !layout ? "hidden" : undefined }}
        onPointerEnter={cancelClose} onPointerLeave={closeLater} onKeyDown={(event) => keyDown(event)}>
          {row("打开主面板", command("open_main"))}
          {row(refreshing ? "刷新中…" : "立即刷新", command("refresh"), { disabled: refreshing })}
          {separator}
          {!islandMenu && row("显示灵动岛", command("toggle_island"), { checked: settings.island_visible })}
          {row("切换连接", () => openSub("connections"), { sub: "connections", disabled: !settings.island_visible })}
          {row("显示位置", () => openSub("position"), { sub: "position", disabled: !settings.island_visible })}
          {row("始终置顶", command("topmost"), { checked: settings.always_on_top })}
          {!islandMenu && <>
          {row("免打扰", command("dnd"), { checked: settings.dnd })}
          {separator}
          {row("重置窗口位置", command("reset_layout"))}
          {row("设置", command("open_source_settings"))}
          {row("关于 CC Usage", command("about"))}
          {separator}
          {row("退出", command("quit"), { danger: true })}
          </>}
        {error && <p role="alert" className="p-2 text-xs text-danger">{error}</p>}
      </div>
      {page !== "root" && <div ref={subRef} id="island-submenu" role="menu" aria-label={page === "position" ? "显示位置" : "切换连接"}
        className={panelClass} style={{ top: subTop, left: layout?.side === "left" ? rootX - subWidth - 6 : rootX + rootWidth + 6, width: subWidth }}
        onPointerEnter={cancelClose} onPointerLeave={closeLater} onKeyDown={(event) => keyDown(event, true)}>
        {page === "position" && ([
          ["free", "自由悬浮", AppWindow], ["top", "上边居中", PanelTop], ["bottom", "下边居中", PanelBottom], ["left", "左边居中", PanelLeft], ["right", "右边居中", PanelRight],
        ] as const).map(([id, label, itemIcon]) => row(label, command(`pos_${id}`), { radio: true, itemIcon, checked: (settings.dock.edge ?? "free") === id }))}
        {page === "connections" && <>
          {loading && <p className="flex items-center gap-2 p-3 text-xs text-text-tertiary"><RefreshCw className="size-3 animate-spin" />正在读取连接…</p>}
          {connectionError && <p role="alert" className="p-2 text-xs text-danger">连接读取失败：{connectionError}</p>}
          {(["claude", "codex"] as const).flatMap((platform) => (["auth", "api"] as const).flatMap((kind) => {
            const found = connections.filter((c) => c.platformId === platform && c.kind === kind)
            const name = `${platform === "claude" ? "Claude" : "Codex"} · ${kind === "auth" ? "Auth" : "API Key"}`
            return found.length ? found.map((c) => row(`${name} · ${c.name}`, () => {
              void act(async () => { await api.setIslandConnection(c.id); await api.menuClose() })
            }, { radio: true, checked: c.id === settings.island_connection_id, disabled: c.status !== "connected", platform })) : islandMenu ? [] : [row(`${name}（未配置）`, () => {}, { disabled: true, platform })]
          }))}
          {islandMenu && !loading && !connectionError && connections.length === 0 && <p className="p-3 text-xs text-text-tertiary">暂无连接，请在主面板添加</p>}
          {separator}{row("管理连接…", command("open_source_settings"))}
        </>}
      </div>}
    </div>
  )
}
