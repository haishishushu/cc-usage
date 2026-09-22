import { Activity, ChevronDown, LoaderCircle, Pencil, Plus, Plug, Power, Trash2 } from "lucide-react"
import { useLayoutEffect, useRef, useState } from "react"
import { cn } from "@/lib/utils"
import { PlatformLogo } from "@/components/brand/PlatformLogo"
import { Button, ConnectionKindChip } from "@/components/ui/primitives"
import { connectionToggleAction } from "@/lib/connectionActions"
import { canAddConnection } from "@/lib/connectionPlatformTab.ts"
import { PLATFORMS, platformConfig, isFetchOnlyPlatform } from "@/lib/platforms"
import { useToast } from "@/components/ui/Toast"
import { useSlidingIndicator } from "@/lib/useSlidingIndicator"
import type { Connection, PlatformId } from "@/types"

const STATUS = { connected: ["已连接", "text-success-text"], paused: ["已断开", "text-text-tertiary"], expired: ["连接已过期", "text-warn"], invalid: ["凭证无效", "text-danger"], offline: ["离线", "text-warn"] }
type Props = {
  connections: Connection[]; selectedId?: string | null
  /** 当前查看的平台；列表只显示该平台的连接 */
  platform: PlatformId
  onPlatformChange: (platform: PlatformId) => void
  onAdd: () => void
  onEnable: (id: string | null) => Promise<boolean>
  /** 启用时把连接的凭证与地址写入对应 CLI 的配置文件，返回后端说明文案（画布 18 后续，对齐 cc-switch） */
  onApplyToCli: (id: string) => Promise<string>
  onEdit: (connection: Connection) => void
  /** 检测连接：用保存的凭证对上游做一次轻量验证并计时（对齐 cc-switch 的检测按钮） */
  onTest: (id: string) => Promise<{ ok: boolean; latency_ms: number; message: string | null }>
  onPause: (id: string, paused: boolean) => Promise<void>
  onRemove: (connection: Connection) => void
  loading?: boolean; error?: string | null; onRetry: () => void
}

function Actions({ connection: c, selectedId, onEnable, onApplyToCli, onTest, onEdit, onPause, onRemove }: Pick<Props, "selectedId" | "onEnable" | "onApplyToCli" | "onTest" | "onEdit" | "onPause" | "onRemove"> & { connection: Connection }) {
  const [busy, setBusy] = useState(false)
  const [testing, setTesting] = useState(false)
  const toast = useToast()
  const using = c.status === "connected" && selectedId === c.id
  const toggle = connectionToggleAction(c.status, using)
  /** 失败统一弹红色 Toast：操作按钮在表格右侧，行内红字容易被表格撑得看不见 */
  const run = async (label: string, action: () => Promise<unknown>) => {
    setBusy(true)
    try { await action() } catch (reason) { toast.danger(`${label}失败`, String(reason)) } finally { setBusy(false) }
  }
  /** 检测连接（对齐 cc-switch）：上游验证一次并计时，结果 Toast 播报 */
  const testConnection = async () => {
    setTesting(true)
    try {
      const result = await onTest(c.id)
      if (result.ok) toast.success(`${(isFetchOnlyPlatform(c.platformId) && c.kind === "auth") ? "本机来源可读" : "连接可用"} · ${result.latency_ms}ms`, result.message ?? undefined)
      else toast.danger("连接不可用", result.message ?? "上游验证未通过")
    } finally { setTesting(false) }
  }
  const toggleConnection = async () => {
    if (toggle.action === "clear") {
      await onPause(c.id, true)
      if (!await onEnable(null)) throw new Error("连接已暂停，但灵动岛选择未清除，请重试")
      toast.success(`已断开 ${c.name}`, "配置、凭证与历史统计都保留")
    } else if (toggle.action === "reconnect" || toggle.action === "select") {
      // 启用 = 真正生效：先把凭证与地址写入对应 CLI 的配置文件（cc-switch 的核心作用），再联动灵动岛显示
      const note = await onApplyToCli(c.id)
      if (!await onEnable(c.id)) throw new Error("启用未生效，请重试")
      toast.success(`已启用 ${c.name}${(isFetchOnlyPlatform(c.platformId) && c.kind === "auth") ? " 本机监控" : ""}`, note)
    }
  }
  const style = "motion-button inline-flex h-8 w-[86px] shrink-0 items-center justify-center gap-1.5 rounded-lg text-xs font-medium transition-colors focus-visible:outline-2 focus-visible:outline-accent-blue disabled:cursor-default"
  return <div className="flex flex-col items-center gap-1">
    <div className="flex items-center justify-center gap-2">
      <button type="button" disabled={busy || testing || (c.status === "paused" && !(isFetchOnlyPlatform(c.platformId) && c.kind === "auth"))} onClick={() => void run("检测", testConnection)} title={c.status === "paused" ? "连接已断开，请先重新连接" : "用保存的凭证对上游做一次验证并计时"} className={cn(style, "bg-success-soft text-success-text hover:brightness-95 disabled:cursor-default disabled:opacity-50")}>
        {testing ? <LoaderCircle aria-hidden className="size-3 animate-spin" /> : <Activity aria-hidden className="size-3" />}{testing ? "检测中" : "检测"}
      </button>
      <button type="button" disabled={busy || c.status === "paused"} onClick={() => onEdit(c)} title={c.kind === "auth" ? "重命名连接，或重新读取 CLI 授权" : "重命名连接，或更换 API Key"} className={cn(style, "bg-accent-blue-soft text-accent-blue hover:brightness-95 disabled:cursor-default disabled:opacity-50")}>
        <Pencil aria-hidden className="size-3" />编辑
      </button>
      <button type="button" disabled={busy || toggle.disabled} onClick={() => void run(toggle.label, toggleConnection)} title={toggle.action === "unavailable" ? "连接当前不可用，请先修复凭证" : undefined} className={cn(style, toggle.label === "断开" ? "bg-warn-soft text-warn hover:brightness-95" : "bg-success-soft text-success-text hover:brightness-95", "disabled:opacity-50")}>
        <Power aria-hidden className="size-3" />{toggle.label}
      </button>
      <button type="button" disabled={busy} onClick={() => onRemove(c)} className={cn(style, "bg-danger-soft text-danger hover:brightness-95 disabled:opacity-50")}><Trash2 aria-hidden className="size-3" />移除</button>
    </div>
  </div>
}

/**
 * 平台分页标签 —— 下划线形态，刻意区别于顶部「总览 / 设置」的 pill 分段控件，
 * 避免两级导航撞脸。只平铺 Claude 与 Codex，其余收进「更多」下拉；
 * 从下拉里选中的平台会临时补进标签条，否则看不出当前停在哪。
 * 容器的 -mb-px 让选中项的 2px 下边框压在标题行的分隔线上。
 */
/** 标签条的一项：平台（切换连接列表）或供应商预设（点击打开添加弹窗预填） */
type TabItem = { id: string; kind: "platform"; name: string; platform: PlatformId; disabled: boolean }

/** 「更多」按钮自身宽度预留：测量第一行可用宽度时先扣掉 */
const MORE_RESERVE = 64

function PlatformTabs({ value, onChange }: {
  value: PlatformId
  onChange: (platform: PlatformId) => void
}) {
  const [moreOpen, setMoreOpen] = useState(false)
  const rowRef = useRef<HTMLDivElement>(null)
  const itemRefs = useRef(new Map<string, HTMLButtonElement>())
  const [overflowIds, setOverflowIds] = useState<string[]>([])
  const items: TabItem[] = PLATFORMS.map((p): TabItem => ({ id: p.id, kind: "platform", name: p.name, platform: p.id, disabled: p.availability === "not-integrated" }))
  // 选中下划线滑过去，而不是从一个标签硬切到另一个
  const { rootRef, plateRef } = useSlidingIndicator<HTMLDivElement, HTMLSpanElement>({
    selector: 'button[aria-selected="true"]',
    mode: "underline",
    itemsKey: items.map((i) => i.id).join(","),
    value,
  })
  // 测量：第一行放不下的项收进「更多」（保留渲染以便测宽，仅视觉隐藏）
  useLayoutEffect(() => {
    const measure = () => {
      const row = rowRef.current
      if (!row) return
      const available = row.clientWidth - MORE_RESERVE
      let used = 0
      const overflow: string[] = []
      for (const item of items) {
        const el = itemRefs.current.get(item.id)
        if (!el) continue
        used += el.offsetWidth
        if (used > available) overflow.push(item.id)
      }
      setOverflowIds(overflow)
    }
    measure()
    const observer = new ResizeObserver(measure)
    if (rowRef.current) observer.observe(rowRef.current)
    return () => observer.disconnect()
    // items 为模块级常量组合，不随渲染变化
  }, [])
  const hiddenIds = new Set(overflowIds)
  const hiddenItems = items.filter((item) => hiddenIds.has(item.id))
  const renderItem = (item: TabItem, inSecondRow: boolean) => {
    const hidden = !inSecondRow && hiddenIds.has(item.id)
    return (
      <button
        key={item.id}
        ref={(el) => {
          if (el) itemRefs.current.set(item.id, el)
          else itemRefs.current.delete(item.id)
        }}
        type="button"
        role="tab"
        aria-selected={item.platform === value}
        disabled={item.disabled}
        title={item.disabled ? `${item.name}（待接入）` : undefined}
        style={hidden ? { visibility: "hidden", position: "absolute" } : undefined}
        onClick={() => { onChange(item.platform); setMoreOpen(false) }}
        className={cn(
          "flex shrink-0 items-center gap-1.5 whitespace-nowrap px-3 pb-[13px] pt-1 text-xs transition-colors focus-visible:outline-2 focus-visible:outline-accent-blue",
          item.platform === value ? "font-semibold text-accent-blue" : "text-text-secondary hover:text-text-primary",
          item.disabled && "pointer-events-none opacity-40",
          hidden && !moreOpen && "pointer-events-none",
        )}
      >
        <PlatformLogo platform={item.platform} size={14} className={item.platform === value ? "" : "opacity-60"} />
        {item.name}
      </button>
    )
  }
  return <div className="flex min-w-0 flex-1 flex-col">
    <div ref={(node) => { rootRef.current = node; rowRef.current = node }} role="tablist" aria-label="平台" className="relative -mb-px flex w-full shrink-0 items-end justify-end gap-1">
      <span ref={plateRef} aria-hidden="true" data-motion-indicator className="motion-indicator pointer-events-none absolute bottom-0 left-0 h-0.5 rounded-full bg-accent-blue" />
      {items.map((item) => renderItem(item, false))}
      {hiddenItems.length > 0 && (
        <button
          type="button"
          aria-expanded={moreOpen}
          onClick={() => setMoreOpen((v) => !v)}
          className={cn(
            "flex shrink-0 items-center gap-1 px-3 pb-[13px] pt-1 text-xs transition-colors",
            moreOpen ? "font-semibold text-accent-blue" : "text-text-secondary hover:text-text-primary",
          )}
        >
          更多
          <ChevronDown aria-hidden className={cn("size-3 transition-transform", moreOpen && "rotate-180")} />
        </button>
      )}
    </div>
    {/* 第二层：点「更多」在下一行展开放不下的标签，而不是弹出浮层 */}
    {moreOpen && hiddenItems.length > 0 && (
      <div className="flex flex-wrap items-center gap-x-1 gap-y-1 border-b pb-1">
        {hiddenItems.map((item) => renderItem(item, true))}
      </div>
    )}
  </div>
}

export function ConnectionTable(props: Props) {
  const { connections, loading, error, onRetry, platform, onPlatformChange, selectedId } = props
  const current = platformConfig(platform)
  const canAdd = canAddConnection(platform)
  /** 只展示当前平台的连接；使用中提示仍按全量计算 */
  const rows = connections.filter((c) => c.platformId === platform)
  /** 灵动岛全局只用一个连接，切到别的平台时靠这行说明避免误解 */
  const using = connections.find((c) => c.id === selectedId)
  const name = (c: Connection) => <div className="flex w-full min-w-0 items-center justify-center gap-2 overflow-hidden"><span title={c.name} className="min-w-0 truncate font-mono text-xs">{c.name}</span><ConnectionKindChip kind={c.kind} local={(isFetchOnlyPlatform(c.platformId) && c.kind === "auth") && c.platformId !== "grok"} /></div>
  const status = (c: Connection) => <span className={cn("inline-flex items-center gap-2 whitespace-nowrap text-xs", STATUS[c.status][1])}><span className="size-1.5 rounded-full bg-current" />{(isFetchOnlyPlatform(c.platformId) && c.kind === "auth") && c.status === "connected" ? "监控已启用" : STATUS[c.status][0]}</span>
  return <div className="flex w-full flex-col gap-3">
    {/* 标题占左半 50%；右半 50% 排平台/供应商标签——第一行放不下的收进「更多」，点击在下一行展开 */}
    <header className="flex min-h-11 flex-wrap items-end justify-between gap-x-6 gap-y-2 border-b">
      <h2 className="flex h-8 w-2/5 min-w-[160px] items-center gap-2 pb-3 text-sm font-semibold"><Plug aria-hidden className="size-4 text-accent-blue" strokeWidth={1.75} />连接管理</h2>
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-end justify-end gap-4">
          <PlatformTabs value={platform} onChange={onPlatformChange} />
          {/* 按钮与标签条同行：mb 让它与标签文字视觉居中，而不是贴着分隔线 */}
          <Button variant="primary" className="mb-1.5 shrink-0" disabled={!canAdd} title={canAdd ? `添加 ${current.name} 连接` : `${current.name} 不支持在此添加连接`} onClick={props.onAdd}><Plus aria-hidden className="size-3" />添加连接</Button>
        </div>
      </div>
    </header>
    <p className="text-[11px] leading-relaxed text-text-tertiary">{isFetchOnlyPlatform(platform) ? `发现 ${current.name} 本机来源，启用／暂停本应用的采集与展示，不切换外部账号。${current.limitation ?? ""}` : `Auth 与 API Key 由 CC Switch 管理；此处读取已应用到本机 ${current.name} 的配置，负责启用、断开、编辑与移除。`}</p>
    {rows.length === 0 ? <div className="flex min-h-[168px] flex-col items-center justify-center gap-2 rounded-xl border bg-surface p-5 text-center">
      {loading ? <LoaderCircle className="size-5 animate-spin text-accent-blue" /> : <Plug className="size-5 text-text-tertiary" />}
      <p className="text-xs">{loading ? "正在读取连接…" : error ? "连接列表读取失败" : `${current.name} 还没有任何连接`}</p>
      <p className="text-[11px] text-text-tertiary">{error ?? (canAdd
        ? isFetchOnlyPlatform(platform) ? `请先在 ${current.name} 中产生会话，再点击添加连接检测本机来源。` : `请先在 CC Switch 中配置，再点击「添加 ${current.name} 连接」选择类型。`
        : `${current.name} 的额度直接读取本机 CLI 凭证，无需也不支持在此添加连接。`)}</p>
      {error && <Button onClick={onRetry}>重试</Button>}
    </div> : <>
      <div className="hidden overflow-hidden rounded-xl border bg-surface min-[1120px]:block">
        <table className="w-full table-fixed text-center text-xs">
          <colgroup>{[30, 12, 13, 13, 32].map((width, i) => <col key={i} style={{ width: `${width}%` }} />)}</colgroup>
          <thead className="h-10 border-b bg-surface-2 text-text-secondary"><tr>{["连接名称", "类型", "状态", "最近同步", "操作"].map(label => <th key={label} className="px-2 align-middle font-medium">{label}</th>)}</tr></thead>
          <tbody>{rows.map(c => <tr key={c.id} className="h-[60px] border-b last:border-b-0 [&>td]:px-2 [&>td]:py-2 [&>td]:align-middle">
            <td className="max-w-0">{name(c)}</td><td className="max-w-0"><span className="block truncate text-text-secondary" title={c.label}>{c.label}</span></td><td>{status(c)}</td>
            <td className="text-[11px] text-text-tertiary">{c.lastSyncText}</td><td><Actions {...props} connection={c} /></td>
          </tr>)}</tbody>
        </table>
      </div>
      <div className="overflow-hidden rounded-xl border bg-surface min-[1120px]:hidden">{rows.map(c => <article key={c.id} className="flex min-w-0 flex-col gap-3 border-b p-4 last:border-b-0">
        <div className="flex min-w-0 items-center justify-between gap-3"><div className="flex min-w-0 items-center gap-2"><PlatformLogo platform={c.platformId} size={16} className="shrink-0" />{name(c)}</div><span className="shrink-0">{status(c)}</span></div>
        <p className="text-center text-[11px] text-text-tertiary">{c.label} · 最近同步 {c.lastSyncText}</p><Actions {...props} connection={c} />
      </article>)}</div>
    </>}
    {error && rows.length > 0 && <div role="alert" className="flex items-center justify-between gap-3 text-xs text-danger"><span>{error}</span><Button onClick={onRetry}>重试</Button></div>}
    <p className="text-[11px] leading-relaxed text-text-tertiary">
      同时仅使用一个灵动岛连接（跨平台唯一）。当前使用中：{using ? `${platformConfig(using.platformId).name} · ${using.name}` : "无"}。
    </p>
    <p className="text-[11px] leading-relaxed text-text-tertiary">断开保留配置、凭证与历史；本机 Token 按平台统计，无法区分具体账号或 Key。</p>
  </div>
}
