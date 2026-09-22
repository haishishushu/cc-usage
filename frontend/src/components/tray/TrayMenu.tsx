import {
  Bell,
  BellOff,
  Check,
  ChevronRight,
  Eye,
  Info,
  LayoutGrid,
  Loader,
  LogOut,
  Move,
  PanelTopOpen,
  RefreshCw,
  Settings,
} from "lucide-react"
import { cn } from "@/lib/utils"
import { PlatformLogo } from "@/components/brand/PlatformLogo"
import { Radio } from "@/components/ui/primitives"

/**
 * 系统托盘右键菜单 —— §2.6 / 画布 18
 * 宽 268、子菜单 300；菜单项高 32 r6、图标位 16、分隔线上下各 4。
 * 禁用项降低不透明度而非隐藏。
 */
export const TRAY_MENU_WIDTH = 268

type Item =
  | { sep: true }
  | {
      sep?: false
      label: string
      icon?: React.ComponentType<{ className?: string }>
      check?: boolean
      sub?: boolean
      hint?: string
      danger?: boolean
      accent?: boolean
      disabled?: boolean
    }

export interface TrayState {
  refreshing?: boolean
  islandHidden?: boolean
  dnd?: boolean
}

export function trayItems(s: TrayState = {}): Item[] {
  return [
    { label: "打开主面板", icon: PanelTopOpen },
    s.refreshing
      ? { label: "刷新中…", icon: Loader, disabled: true }
      : { label: "立即刷新", icon: RefreshCw },
    { sep: true },
    s.islandHidden
      ? { label: "显示灵动岛", icon: Eye }
      : { label: "显示灵动岛", check: true },
    { label: "切换连接", icon: LayoutGrid, sub: true, disabled: !!s.islandHidden },
    { label: "显示位置", icon: Move, sub: true, disabled: !!s.islandHidden },
    { label: "始终置顶", check: true },
    s.dnd
      ? { label: "免打扰", check: true, accent: true, hint: "已开启" }
      : { label: "免打扰", icon: BellOff },
    { sep: true },
    { label: "重置窗口位置", icon: Move },
    { label: "设置", icon: Settings },
    { label: "关于 CC Usage", icon: Info },
    { sep: true },
    { label: "退出", icon: LogOut, danger: true },
  ]
}

export function TrayMenu({ state = {} }: { state?: TrayState }) {
  return (
    <div
      className="flex flex-col gap-px rounded-[8px] border bg-surface p-1 shadow-popover"
      style={{ width: TRAY_MENU_WIDTH }}
    >
      {trayItems(state).map((it, i) =>
        "sep" in it && it.sep ? (
          <div key={i} className="px-2 py-1">
            <div className="h-px w-full bg-[var(--border)]" />
          </div>
        ) : (
          <div
            key={i}
            className={cn(
              "flex h-8 w-full items-center gap-2.5 rounded-[6px] px-2",
              !it.disabled && "hover:bg-surface-3",
              it.disabled && "pointer-events-none opacity-[0.42]",
            )}
          >
            <span className="grid size-4 shrink-0 place-items-center">
              {it.check ? (
                <Check className={cn("size-3.5", it.accent ? "text-accent-blue" : "text-text-primary")} />
              ) : it.icon ? (
                <it.icon
                  className={cn(
                    "size-3.5",
                    it.danger ? "text-danger" : it.accent ? "text-accent-blue" : "text-text-secondary",
                  )}
                />
              ) : null}
            </span>
            <span
              className={cn(
                "min-w-0 flex-1 truncate text-[13px]",
                it.danger ? "text-danger" : it.accent ? "font-semibold text-accent-blue" : "text-text-primary",
              )}
            >
              {it.label}
            </span>
            {it.hint && <span className="text-[11px] text-accent-blue">{it.hint}</span>}
            {it.sub && <ChevronRight className="size-3.5 shrink-0 text-text-tertiary" />}
          </div>
        ),
      )}
    </div>
  )
}

/** 「切换连接」子菜单：单选，改的就是设置里同一个值 */
export function TrayPlatformSubmenu() {
  const rows = [
    { id: "claude" as const, name: "Claude", conn: "官方订阅（20x）", selected: true },
    { id: "claude" as const, name: "Claude", conn: "个人 API Key", selected: false },
    { id: "codex" as const, name: "Codex", conn: "工作账号", selected: false, status: "连接已过期" },
    { id: "gemini" as const, name: "Gemini", conn: "尚未连接", selected: false, disabled: true },
  ]
  return (
    <div className="flex w-[300px] flex-col gap-px rounded-[8px] border bg-surface p-1 shadow-popover">
      {rows.map((r, i) => (
        <div
          key={i}
          className={cn(
            "flex h-8 w-full items-center gap-[9px] rounded-[6px] px-2",
            r.selected && "bg-surface-3",
            r.disabled && "pointer-events-none opacity-[0.42]",
          )}
        >
          <span className="grid size-3.5 shrink-0 place-items-center">
            <Radio checked={!!r.selected} />
          </span>
          <PlatformLogo platform={r.id} />
          <span className="min-w-0 flex-1 truncate text-xs text-text-primary">
            {r.name} · {r.conn}
          </span>
          {r.status && <span className="text-[10px] text-warn">{r.status}</span>}
        </div>
      ))}
      <div className="px-2 py-1">
        <div className="h-px w-full bg-[var(--border)]" />
      </div>
      <div className="flex h-8 w-full items-center gap-2.5 rounded-[6px] px-2">
        <span className="grid size-3.5 shrink-0 place-items-center">
          <Settings className="size-[13px] text-text-secondary" />
        </span>
        <span className="min-w-0 flex-1 truncate text-xs text-text-secondary">
          在设置中配置连接与统计来源…
        </span>
      </div>
    </div>
  )
}

/** 托盘 tooltip：平台、连接、各额度窗口已用百分比、最近更新时间 */
export function TrayTooltip({
  app,
  connection,
  quotas,
  updated,
}: {
  app: string
  connection: string
  quotas: { key: string; value: string }[]
  updated: string
}) {
  return (
    <div className="flex w-[230px] flex-col gap-1 rounded-[8px] border bg-surface px-3 py-2.5 shadow-popover">
      <span className="text-xs font-semibold text-text-primary">{app}</span>
      <span className="break-words text-[11px] text-text-secondary">{connection}</span>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        {quotas.map((q) => (
          <span key={q.key} className="flex items-center gap-1.5">
            <span className="tnum font-mono text-[11px] text-text-tertiary">{q.key}</span>
            <span className="tnum font-mono text-xs font-semibold text-text-primary">{q.value}</span>
          </span>
        ))}
      </div>
      <span className="text-[10px] text-text-tertiary">{updated}</span>
    </div>
  )
}

/** 托盘图标水位角标：取当前配置下最紧张的窗口；不可用为中性灰，不画成红色 */
export function TrayIconBadgeTone(worstPercent: number | null): string {
  if (worstPercent === null) return "#9AA0AA"
  if (worstPercent >= 90) return "#DC2626"
  if (worstPercent >= 75) return "#D98500"
  return "#06C167"
}

export { Bell }
