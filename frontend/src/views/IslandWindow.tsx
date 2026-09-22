import { IslandConnectionSwitcher } from "@/components/island/IslandConnectionSwitcher"
import { IslandMotion } from "@/components/island/IslandMotion"
import { useCallback, useEffect, useRef, useState } from "react"
import { IslandCollapsed, IslandExpanded, type IslandData } from "@/components/island/UsageIsland"
import { DockedIsland } from "@/components/island/DockedIsland"
import type { DeltaPhase } from "@/components/island/TokenDelta"
import { CLAUDE_QUOTAS, LOCAL_SOURCE, RUNNING_SESSION_TEXT, SESSIONS, TOTAL_DELTA_TEXT } from "@/mock/data"
import { useConnectionKind, useLiveUsage } from "@/lib/useLiveUsage"
import { useSettings } from "@/lib/useSettings"
import { useConnections } from "@/lib/useConnections"
import { shouldRefreshConnection } from "@/lib/refreshScope"
import { useApiUsage, useBalance, useCostEstimate, useQuota } from "@/lib/useQuota"
import { toQuotaWindows } from "@/lib/quotaMap"
import { api, isTauri, listenEvent } from "@/lib/api"
import { createWindowSizeSync } from "@/lib/windowSizeSync"
import { authQuotaRows } from "@/lib/authQuotaRows"
import { collapseExpandedView, snapIndicatorVisible } from "@/lib/islandViewState"
import type { DockEdge, IslandMode, PlatformId } from "@/types"
import { platformConfig, isFetchOnlyPlatform } from "@/lib/platforms"
import { useSourceMetrics } from "@/lib/useSourceMetrics"
import { useCollectionStatus } from "@/lib/useCollectionStatus"
import { dockPulseActive } from "@/lib/dockPulse"
import { balanceSnapshotTime, connectionQueries, islandActiveConnection, islandConnectionStatus, islandCost } from "@/lib/islandPresentation"
import { dockQuotaView } from "@/lib/dockQuotaView"

/** 平台展示名。未知平台原样显示，不臆测 */
const PLATFORM_NAMES: Record<string, string> = {
  claude: "Claude",
  codex: "Codex",
  gemini: "Gemini",
  grok: "Grok",
}

/**
 * 灵动岛窗口 —— 形态状态机（§2.1.3）
 *
 *   自由态 collapsed ──双击──▶ expanded ──双击──▶ collapsed
 *        │  拖到边缘吸附
 *        ▼
 *   停靠态 docked ──悬停──▶ peek（完整收缩态）──双击──▶ expanded
 *        └──双击──▶ 回到自由态 collapsed
 *
 * 手势：桌面端按住直接拖动，不使用长按。
 */
const SNAP_ZONE = 24

/**
 * 桌面端：让 Tauri 窗口尺寸跟随岛的实际内容。
 * 收缩 / 展开 / 停靠三态尺寸差别很大，写死窗口高度会把内容截断或留大片空白。
 */
function useSyncWindowSize() {
  const roRef = useRef<ResizeObserver | null>(null)
  const cleanupRef = useRef<(() => void) | null>(null)
  const syncRef = useRef<ReturnType<typeof createWindowSizeSync> | null>(null)
  if (!syncRef.current) {
    syncRef.current = createWindowSizeSync(async (size) => {
      const { invoke } = await import("@tauri-apps/api/core")
      const maxHeight = await invoke<number | null>("resize_island", size)
      window.dispatchEvent(new CustomEvent("island-size-limit", { detail: maxHeight }))
    })
  }

  // 用 callback ref：收缩/展开/停靠切换时会换整棵 JSX 树，
  // useRef + useLayoutEffect 只在挂载时跑一次，会盯住已卸载的节点。
  return useCallback((el: HTMLDivElement | null) => {
    cleanupRef.current?.()
    cleanupRef.current = null
    roRef.current?.disconnect()
    roRef.current = null
    syncRef.current?.cancelPending()
    if (!isTauri || !el) return

    const apply = (reconcile = false) => {
      if (!el.isConnected) return
      const r = el.getBoundingClientRect()
      const zoom = Number.parseFloat(getComputedStyle(el).zoom) || 1
      const w = Math.ceil(Math.max(r.width, el.scrollWidth * zoom))
      const h = Math.ceil(Math.max(r.height, el.scrollHeight * zoom))
      if (w < 8 || h < 8) return
      // Rust 停靠/重置也会改窗口尺寸；内容没变时 ResizeObserver 不会通知。
      // 允许一个 CSS 像素的 DPI 取整误差，防止反复校正正常的分数像素。
      const mismatch = Math.abs(window.innerWidth - w) > 1 || Math.abs(window.innerHeight - h) > 1
      syncRef.current?.request({ width: w, height: h }, reconcile && mismatch)
    }
    let frame = 0
    const reconcile = () => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(() => apply(true))
    }
    window.addEventListener("resize", reconcile)
    cleanupRef.current = () => {
      window.removeEventListener("resize", reconcile)
      cancelAnimationFrame(frame)
    }
    const ro = new ResizeObserver(() => void apply())
    ro.observe(el)
    roRef.current = ro
    // 新节点挂载时原生窗口可能仍是上一种形态，必须校正而非命中旧尺寸缓存。
    void apply(true)
    reconcile()
  }, [])
}

export function IslandWindow() {
  const shellRef = useSyncWindowSize()
  const [mode, setMode] = useState<IslandMode>("collapsed")
  const [edge, setEdge] = useState<DockEdge>("top")
  const [peek, setPeek] = useState(false)
  const [dragging, setDragging] = useState(false)
  const draggingRef = useRef(false)
  const [connectionMenuOpen, setConnectionMenuOpen] = useState(false)
  const [quotaNow, setQuotaNow] = useState(() => Date.now())
  const peekTimer = useRef<number | null>(null)

  useEffect(() => {
    const timer = window.setInterval(() => setQuotaNow(Date.now()), 30_000)
    return () => window.clearInterval(timer)
  }, [])

  // 真实实时数据：停靠且未探出时暂停轮询（无可见提示区，省掉无谓渲染）
  // 灵动岛显示的平台由持久化设置决定，与托盘子菜单同一个值
  const { settings } = useSettings()
  const islandPlatform = settings.island_platform
  const live = useLiveUsage(islandPlatform, true, settings.dnd, dragging)
  const collection = useCollectionStatus()
  // 接入方式来自本机真实配置，不写死
  const conn = useConnectionKind(islandPlatform)

  // 额度按连接查；灵动岛只看当前配置平台的那一个连接（§7.3）
  const { connections, loading: connectionsLoading } = useConnections()
  // 灵动岛生效连接：明确选择优先；未选择时自动启用当前平台第一个连接成功的
  // 连接（2026-09-19 鼠鼠定版）。仍不回退到本机账号或唯一候选。
  const active = islandActiveConnection(settings.island_connection_id, connections, islandPlatform)
  const islandKind = active?.kind ?? settings.island_kind
  const queries = connectionQueries(active)
  const quota = useQuota(queries.quota)
  const balance = useBalance(queries.balance)
  const officialApiUsage = useApiUsage(queries.usage)
  const localCost = useCostEstimate(islandPlatform, "today")
  useEffect(() => setQuotaNow(Date.now()), [islandPlatform])
  const sourceMetrics = useSourceMetrics(islandPlatform, "today", null, null, quotaNow)
  const nativeMonitor = isFetchOnlyPlatform(islandPlatform as PlatformId) && islandPlatform !== "grok"
  const refreshing = useRef(false)
  // 刷新反馈动画的序号：任一入口触发刷新都递增，岛的三种形态据此重放动画
  const [refreshSerial, setRefreshSerial] = useState(0)
  // 刷新是否进行中：驱动绿色流光循环，直到全部查询完成再收尾
  const [refreshActive, setRefreshActive] = useState(false)
  useEffect(() => {
    let disposed = false
    const offs: Array<() => void> = []
    const listen = (promise: Promise<() => void>) =>
      void promise.then((un) => { if (disposed) un(); else offs.push(un) }).catch(console.error)
    listen(listenEvent<{ platform: string }>("live-usage", usage => { if (usage.platform === islandPlatform) setQuotaNow(Date.now()) }))
    // 网络额度/余额/用量刷新，多个刷新事件共用同一防重入闸门。
    const refreshNetwork = (scanSessions: boolean) => {
      if (refreshing.current) return
      refreshing.current = true
      setRefreshSerial((value) => value + 1)
      setRefreshActive(true)
      const jobs: Array<Promise<unknown>> = [quota.refresh(), balance.refresh(), officialApiUsage.refresh()]
      if (scanSessions) jobs.push(api.scanLocalSessions())
      void Promise.all(jobs).catch(console.error).finally(() => { refreshing.current = false; setRefreshActive(false) })
    }
    // 右键“立即刷新”：连同本机会话一起重扫。
    listen(listenEvent("island-refresh", () => refreshNetwork(true)))
    // 连接管理“更新”凭证/网关、平台切换、监听器发现凭证变化：只需重查网络额度，
    // 会话由采集器/watcher 单独维护，不必在此重扫。
    listen(listenEvent<string | null>("refresh-requested", (connectionId) => {
      if (shouldRefreshConnection(connectionId, active?.id)) refreshNetwork(false)
    }))
    return () => { disposed = true; offs.forEach((off) => off()) }
  }, [active?.id, islandPlatform, quota.refresh, balance.refresh, officialApiUsage.refresh])
  const quotaWindows =
    quota.state?.state === "ok" ? toQuotaWindows(quota.state.windows, quotaNow) : null

  // 停靠条的额度形态：真实窗口 / API Key 无套餐满格蓝 / 未知空轨道（§2.1.3）。
  // 岛卡片与停靠条共用同一份查询结果，判定收敛在 dockQuotaView 一处。
  const quotaView = dockQuotaView({
    kind: islandKind,
    connected: Boolean(active && active.status !== "paused"),
    quotaQueried: Boolean(queries.quota),
    quotaLoading: quota.loading,
    quotaState: quota.state?.state ?? null,
    windows: quotaWindows,
  })
  const dockQuotas = quotaView.type === "windows" ? authQuotaRows(quotaView.windows) : authQuotaRows(undefined)

  // 采集异常按平台定界：只有当前灵动岛平台的来源出错时，会话状态才不可判定。
  // 其他平台的文件错误照常在来源说明里展示，但不把本平台会话打成「状态未知」。
  const sourceFailed = Boolean(
    collection.status && !collection.status.ok && collection.status.failed_sources.includes(islandPlatform),
  )

  const data: IslandData = isTauri || live.live
    ? {
        platform: islandPlatform as PlatformId,
        platformName: platformConfig(islandPlatform as PlatformId).name,
        localMetrics: nativeMonitor ? { token: live.todayTokenText, credits: sourceMetrics.data?.credits?.toLocaleString("zh-CN", { maximumFractionDigits: 4 }) ?? null, message: "本机留存记录；上报积分不等于余额。" } : undefined,
        kind: islandKind,
        // Auth 保持额度布局；未知值不回退成 API 用量。
        quotas: quotaWindows ?? undefined,
        quotaUnavailable: !quotaWindows,
        quotaMessage: active?.status === "paused" ? "连接已断开，请在设置中重新连接"
          : quota.stale ? "额度可能已过期，请刷新"
          : quota.loading && !quotaWindows ? "正在查询额度…"
          : quota.state && quota.state.state !== "ok" ? quota.state.reason
          : islandPlatform !== "grok" && quotaWindows && !quotaWindows.some(w => w.key === "5h") ? "当前连接未提供 5h 额度"
          : !quotaWindows && islandKind === "auth" ? "暂无额度信息" : undefined,
        apiToday: {
          tokenLabel: officialApiUsage.state?.state === "ok" ? "组织今日 Token" : "本机今日 Token",
          token: officialApiUsage.state?.state === "ok"
            ? `${officialApiUsage.state.total_tokens.toLocaleString("en-US")} Token`
            : live.todayTokenText,
          ...islandCost(officialApiUsage.state?.state === "ok" ? officialApiUsage.state.cost_usd : null, localCost),
        },
        balanceText: queries.balance && balance.state?.state === "ok" ? `${balance.state.balance.toFixed(2)} ${balance.state.currency}` : undefined,
        balanceLevel: queries.balance && balance.state?.state === "ok" ? (balance.state.balance <= 0 ? "empty" : "healthy") : undefined,
        balanceTimeText: queries.balance && balance.state?.state === "ok" ? balanceSnapshotTime(balance.fetchedAt) ?? undefined : undefined,
        apiMessage: islandKind !== "api" ? undefined : active?.status === "paused" ? "连接已断开，当前仅显示本机统计"
          : !active ? "请先在连接管理中启用连接，当前仅显示本机统计"
          : queries.balance ? balance.stale ? "余额可能已过期，请刷新" : balance.state && balance.state.state !== "ok" ? balance.state.reason : undefined
          : officialApiUsage.state?.state === "ok" ? officialApiUsage.state.cost_reason ?? undefined
          : officialApiUsage.state?.reason ?? (officialApiUsage.loading ? "正在查询组织用量，暂显示本机统计" : undefined),
        todayTokenText: live.todayTokenText,
        connectionLabel: active?.name ?? (settings.island_connection_id
          ? `${settings.island_connection_name ?? "所选连接"}（不可用）`
          : conn.label ?? undefined),
        sessions: sourceFailed ? live.sessions.map((session) => ({ ...session, state: "unknown" as const })) : live.sessions,
        sessionStatusUnknown: sourceFailed,
        sessionCountText: sourceFailed && live.sessions.length > 0
          ? `${live.sessions.length} 个会话状态未知`
          : live.activityCountText,
        sessionSectionTitle: sourceFailed && live.sessions.length > 0
          ? `${live.sessions.length} 个会话状态未知`
          : live.activityCountText ?? undefined,
        deltaText: live.deltaText,
        deltaTokens: live.deltaTokens,
        deltaPhase: live.deltaPhase,
        status: nativeMonitor && active?.status === "connected" ? { tone: "success", label: "本机监控" } : islandConnectionStatus(active?.status ?? null, false, Boolean(settings.island_connection_id)),
        sourceText: `${collection.status && !collection.status.ok ? `采集异常：${collection.status.errors.join("；")} · ` : ""}${officialApiUsage.state?.state === "ok" ? `API 用量来源：${officialApiUsage.state.source} · ` : ""}统计来源：本机 ${PLATFORM_NAMES[islandPlatform] ?? islandPlatform} 会话记录（无法按账号区分）· ${live.activityKind === "running" ? "按本轮开始／结束事件及会话存活状态判断；思考、等待输入及压缩期间保留会话与 Token" : `${live.activeWindowSeconds} 秒内有新记录视为最近活跃`}`,
      }
    : {
        platform: "claude",
        platformName: "Claude",
        kind: "auth",
        quotas: CLAUDE_QUOTAS,
        sessions: SESSIONS,
        sessionCountText: RUNNING_SESSION_TEXT,
        deltaText: TOTAL_DELTA_TEXT,
        deltaPhase: "hold" as DeltaPhase,
        todayTokenText: "12.84M",
        status: { tone: "success", label: "已连接" },
        sourceText: `统计来源：${LOCAL_SOURCE.name}（${LOCAL_SOURCE.scope}）· 设计示例`,
      }

  const [snapHint, setSnapHint] = useState<{ edge: DockEdge; offset: number; centered: boolean } | null>(null)
  const snapEdge = snapHint?.edge
  useEffect(() => {
    let disposed = false
    let off: (() => void) | undefined
    void listenEvent<{ edge: DockEdge; offset: number; centered: boolean } | null>("dock-hint", setSnapHint).then((un) => {
      if (disposed) un(); else off = un
    })
    return () => { disposed = true; off?.() }
  }, [])
  const onPointerDown = useCallback((e: React.PointerEvent) => {
    if (!isTauri || e.button !== 0 || e.detail > 1) return
    if ((e.target as HTMLElement).closest("button, input, a")) return
    const x = e.clientX, y = e.clientY
    const move = (event: PointerEvent) => {
      if (Math.hypot(event.clientX - x, event.clientY - y) < 4) return
      cleanup()
      // 清掉上一次拖动可能延迟到达的提示，等待本轮原生移动事件给出新位置。
      setSnapHint(null)
      draggingRef.current = true
      if (peekTimer.current) window.clearTimeout(peekTimer.current)
      peekTimer.current = null
      setDragging(true)
      void api.islandDrag().catch(console.error).finally(() => { draggingRef.current = false; setDragging(false); setSnapHint(null) })
    }
    const cleanup = () => {
      window.removeEventListener("pointermove", move)
      window.removeEventListener("pointerup", cleanup)
    }
    window.addEventListener("pointermove", move)
    window.addEventListener("pointerup", cleanup, { once: true })
  }, [])
  const contextMenu = (e: React.MouseEvent) => {
    if (!isTauri) return
    e.preventDefault()
    if (peekTimer.current) window.clearTimeout(peekTimer.current)
    void api.islandMenu(refreshing.current || quota.loading || balance.loading).catch(console.error)
  }

  // 启动时恢复上次停靠状态（几何已由 Rust 落位，这里只对齐形态）
  const resetDockView = useCallback((e: DockEdge | null) => {
    if (peekTimer.current) window.clearTimeout(peekTimer.current)
    peekTimer.current = null
    setPeek(false)
    if (e) setEdge(e)
    setMode(e ? "docked" : "collapsed")
  }, [])

  useEffect(() => {
    resetDockView(settings.dock.edge)
  }, [settings.dock.edge, resetDockView])

  // 同一侧再次停靠时 edge 值没变，也必须退出探出/展开态并清掉旧定时器。
  useEffect(() => {
    let disposed = false
    let off: (() => void) | undefined
    void listenEvent<{ edge: DockEdge | null }>("dock-changed", (dock) => {
      resetDockView(dock.edge)
    }).then((un) => { if (disposed) un(); else off = un })
    return () => {
      disposed = true
      off?.()
      if (peekTimer.current) window.clearTimeout(peekTimer.current)
    }
  }, [resetDockView])

  const undock = useCallback(() => {
    setPeek(false)
    setMode("collapsed")
    if (isTauri) void api.dockUndock()
  }, [])

  const onPeekEnter = useCallback(() => {
    if (draggingRef.current) return
    if (peekTimer.current) window.clearTimeout(peekTimer.current)
    peekTimer.current = window.setTimeout(() => setPeek(true), 150)
  }, [])

  // 指针移开 500ms 后自动缩回
  const onPeekLeave = useCallback(() => {
    if (draggingRef.current) return
    if (peekTimer.current) window.clearTimeout(peekTimer.current)
    peekTimer.current = window.setTimeout(() => setPeek(false), 500)
  }, [])

  const collapseExpanded = useCallback(() => {
    if (peekTimer.current) window.clearTimeout(peekTimer.current)
    peekTimer.current = null
    const next = collapseExpandedView(settings.dock.edge)
    if (next.edge) setEdge(next.edge)
    setPeek(next.peek)
    setMode(next.mode)
  }, [settings.dock.edge])

  // 阴影 0 2px 10px：窗口需为岛留出四周空白，否则被窗口边缘切掉
  const shellPad = isTauri && (mode !== "docked" || peek) ? "p-3.5" : ""

  const snapIndicator = snapIndicatorVisible(dragging, mode, Boolean(snapEdge)) && snapEdge && (
        <span
          aria-hidden
          className={`pointer-events-none fixed z-50 ${snapHint?.centered ? "bg-success" : "bg-accent-blue"} ${
            snapEdge === "top"
              ? "left-0 right-0 top-0 h-[3px]"
              : snapEdge === "bottom"
                ? "bottom-0 left-0 right-0 h-[3px]"
                : snapEdge === "left"
                  ? "bottom-0 left-0 top-0 w-[3px]"
                  : "bottom-0 right-0 top-0 w-[3px]"
          }`}
        />
      )

  if (mode === "docked") {
    return (
      <div
        ref={shellRef}
        style={{ opacity: settings.island_opacity / 100, zoom: peek ? settings.island_scale / 100 : settings.island_shrink_scale / 100 }}
        className={`flex w-fit flex-col items-center ${shellPad} ${dragging ? "island-dragging" : ""}`}
        onContextMenu={contextMenu}
        onPointerDown={onPointerDown}
        onMouseEnter={onPeekEnter}
        onMouseLeave={onPeekLeave}
      >
        {snapIndicator}
        {peek ? (
          /* 探出后双击才进入展开态（§2.1.3） */
          <IslandCollapsed data={data} onExpand={() => setMode("expanded")} refreshKey={refreshSerial} refreshing={refreshActive} />
        ) : (
          /* 对停靠条双击 = 解除停靠、回到自由态收缩态。
             处理器只挂在停靠条上，避免探出时与「双击展开」同时触发 */
          <div
            onDoubleClick={undock}
            className={edge === "top" || edge === "bottom" ? "dock-contract-horizontal" : "dock-contract-vertical"}
          >
            {/* 查到额度画真实水位；API Key 无套餐画两条满格蓝（§2.1.3，2026-09-19 鼠鼠定版）；
                其余查不到的只留空轨道，不着色不满格。
                浏览器预览仍用设计示例展示三档水位 */}
            <DockedIsland
              edge={edge}
              quotas={isTauri ? dockQuotas : CLAUDE_QUOTAS}
              unlimited={isTauri && quotaView.type === "unlimited"}
              unavailable={isTauri && !quotaWindows && quotaView.type !== "unlimited"}
              pulse={dockPulseActive(live.deltaTokens, live.deltaPhase, settings.dnd, dragging)}
              pulseKey={live.pulseSerial}
              refreshing={refreshActive}
              refreshKey={refreshSerial}
            />
          </div>
        )}
        {!isTauri && <DockControls edge={edge} onEdge={setEdge} onUndock={undock} />}
      </div>
    )
  }

  return (
    <div
      ref={shellRef}
      style={{ opacity: settings.island_opacity / 100, zoom: settings.island_scale / 100 }}
      className={`flex w-fit flex-col items-center gap-3 ${shellPad} ${dragging ? "island-dragging" : ""}`}
      onContextMenu={contextMenu}
      onPointerDown={onPointerDown}
    >
      {snapIndicator}
      <IslandMotion expanded={mode === "expanded"} dragging={dragging}>
      {mode === "collapsed" ? (
        <IslandCollapsed data={data} onExpand={() => setMode("expanded")} refreshKey={refreshSerial} refreshing={refreshActive} />
      ) : (
        <IslandExpanded
          data={data}
          connectionMenuOpen={connectionMenuOpen}
          connectionSwitcher={
            <IslandConnectionSwitcher
              connections={connections}
              selectedId={active?.id}
              loading={connectionsLoading}
              onOpenChange={setConnectionMenuOpen}
              onSelect={isTauri ? (id) => api.setIslandConnection(id) : undefined}
            />
          }
          onCollapse={collapseExpanded}
          refreshKey={refreshSerial}
          refreshing={refreshActive}
        />
      )}
      </IslandMotion>
      {!isTauri && (
        <FreeControls mode={mode} onMode={setMode} onDock={(e) => { setEdge(e); setMode("docked") }} />
      )}
    </div>
  )
}

/* 以下两个控制条只用于在浏览器里演示状态机；Tauri 中由真实窗口手势驱动，不出现在界面上 */
function FreeControls({
  mode,
  onMode,
  onDock,
}: {
  mode: IslandMode
  onMode: (m: IslandMode) => void
  onDock: (e: DockEdge) => void
}) {
  return (
    <div className="flex items-center gap-2 text-[11px] text-text-tertiary">
      <span>演示：</span>
      <button
        type="button"
        className="rounded border px-2 py-0.5"
        onClick={() => onMode(mode === "collapsed" ? "expanded" : "collapsed")}
      >
        双击{mode === "collapsed" ? "展开" : "收缩"}
      </button>
      <span>吸附到</span>
      {(["top", "bottom", "left", "right"] as DockEdge[]).map((e) => (
        <button key={e} type="button" className="rounded border px-2 py-0.5" onClick={() => onDock(e)}>
          {{ top: "上", bottom: "下", left: "左", right: "右" }[e]}
        </button>
      ))}
      <span className="text-text-tertiary">（吸附区 {SNAP_ZONE}px）</span>
    </div>
  )
}

function DockControls({
  edge,
  onEdge,
  onUndock,
}: {
  edge: DockEdge
  onEdge: (e: DockEdge) => void
  onUndock: () => void
}) {
  return (
    <div className="mt-3 flex items-center gap-2 text-[11px] text-text-tertiary">
      <span>已停靠（悬停探出 · 双击解除）：</span>
      {(["top", "bottom", "left", "right"] as DockEdge[]).map((e) => (
        <button
          key={e}
          type="button"
          className={`rounded border px-2 py-0.5 ${e === edge ? "text-accent-blue" : ""}`}
          onClick={() => onEdge(e)}
        >
          {{ top: "上", bottom: "下", left: "左", right: "右" }[e]}
        </button>
      ))}
      <button type="button" className="rounded border px-2 py-0.5" onClick={onUndock}>
        解除停靠
      </button>
    </div>
  )
}
