import * as React from "react"
import { Check } from "lucide-react"
import { cn } from "@/lib/utils"
import { useSlidingIndicator } from "@/lib/useSlidingIndicator"

/* ---------------------------------------------------------------------------
   基础原语 —— 尺寸与配色严格对齐 pencil-new.pen，勿套用 shadcn 默认值
--------------------------------------------------------------------------- */

/** 次级按钮：padding [7,12] / [8,16]，圆角 r-sm，描边 */
export function Button({
  variant = "ghost",
  className,
  disabled,
  children,
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "ghost" | "primary" | "danger" | "outline-accent"
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      className={cn(
        "motion-button inline-flex items-center gap-1.5 rounded-[8px] text-xs font-normal",
        "px-3 py-[7px]",
        variant === "ghost" && "border bg-surface text-text-primary hover:bg-hover",
        variant === "primary" && "bg-accent-blue font-semibold text-white",
        variant === "danger" && "bg-danger font-semibold text-white",
        variant === "outline-accent" &&
          "border border-accent-blue bg-surface font-medium text-accent-blue",
        disabled && "pointer-events-none opacity-45",
        className,
      )}
      {...props}
    >
      {children}
    </button>
  )
}

/** 徽标：padding [2,6~8]，圆角 6，字号 10~11 */
export function Chip({
  tone = "neutral",
  mono,
  className,
  children,
}: {
  tone?: "neutral" | "success" | "warn" | "danger" | "accent" | "purple"
  mono?: boolean
  className?: string
  children: React.ReactNode
}) {
  const tones: Record<string, string> = {
    neutral: "bg-neutral-soft text-text-secondary",
    success: "bg-success-soft text-success-text",
    warn: "bg-warn-soft text-warn",
    danger: "bg-danger-soft text-danger",
    accent: "bg-accent-blue-soft text-accent-blue",
    purple: "bg-purple-soft text-purple-text",
  }
  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center rounded-[6px] px-1.5 py-0.5 text-[10px] font-medium leading-normal",
        mono && "font-mono tnum",
        tones[tone],
        className,
      )}
    >
      {children}
    </span>
  )
}

/** 接入类型徽标（§3）：官方订阅绿 Auth、API Key 蓝 API */
export function ConnectionKindChip({ kind, local = false }: { kind: "auth" | "api"; local?: boolean }) {
  if (local) return <Chip tone="neutral">本机</Chip>
  if (kind === "auth") return <Chip tone="success" mono>Auth</Chip>
  return <Chip tone="accent" mono>API</Chip>
}

/** 状态徽标：图标 + 文字，不只依赖颜色区分 */
export function StatusBadge({
  tone,
  icon: Icon,
  children,
}: {
  tone: "success" | "warn" | "danger" | "neutral" | "accent"
  icon?: React.ComponentType<{ className?: string }>
  children: React.ReactNode
}) {
  const tones: Record<string, string> = {
    success: "bg-success-soft text-success-text",
    warn: "bg-warn-soft text-warn",
    danger: "bg-danger-soft text-danger",
    neutral: "bg-neutral-soft text-text-secondary",
    accent: "bg-accent-blue-soft text-accent-blue",
  }
  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center gap-1 rounded-[8px] px-2 py-[3px] text-[11px] font-medium",
        tones[tone],
      )}
    >
      {Icon ? <Icon className="size-3" /> : null}
      {children}
    </span>
  )
}

/** 连接状态点 + 文字 */
export function StatusDot({ tone }: { tone: "success" | "warn" | "danger" | "neutral" }) {
  const tones: Record<string, string> = {
    success: "bg-success",
    warn: "bg-warn",
    danger: "bg-danger",
    neutral: "bg-text-tertiary",
  }
  return <span className={cn("size-1.5 shrink-0 rounded-full", tones[tone])} />
}

/** 只读选项值：当前没有可选项的业务不能伪装成可点击下拉框。 */
export function Dropdown({
  label,
  width,
  className,
}: {
  label: string
  width?: number
  className?: string
}) {
  return (
    <span
      aria-disabled="true"
      title="该选项当前不可配置"
      style={width ? { width } : undefined}
      className={cn(
        "inline-flex cursor-not-allowed items-center rounded-[8px] border bg-surface-2 px-2.5 py-[7px] text-xs text-text-secondary",
        className,
      )}
    >
      <span className="truncate">{label}</span>
    </span>
  )
}

export function Radio({ checked }: { checked: boolean }) {
  return (
    <span
      className={cn(
        "grid size-[13px] shrink-0 place-items-center rounded-full border",
        checked ? "border-accent-blue" : "border-border-strong",
      )}
    >
      {checked && <span className="size-[6px] rounded-full bg-accent-blue" />}
    </span>
  )
}

export function Checkbox({ checked }: { checked: boolean }) {
  return (
    <span
      className={cn(
        "grid size-[14px] shrink-0 place-items-center rounded-[4px] border",
        checked ? "border-accent-blue bg-accent-blue" : "border-border-strong bg-surface",
      )}
    >
      {checked && <Check className="size-2.5 text-white" strokeWidth={3} />}
    </span>
  )
}

/** 开关：34×20，圆角 10，滑块 16 */
export function Toggle({
  on,
  onChange,
  disabled,
  label,
}: {
  on: boolean
  onChange?: (next: boolean) => void
  disabled?: boolean
  /** 无可见文字标签时提供无障碍名称 */
  label?: string
}) {
  // 无回调时保持纯展示（设计示例用），有回调则是真开关
  if (!onChange) {
    return (
      <span
        className={cn(
          "motion-toggle flex h-5 w-[34px] shrink-0 items-center rounded-full px-0.5",
          on ? "bg-success" : "bg-track",
        )}
      >
        <span className="motion-toggle-thumb size-4 rounded-full bg-white" style={{ transform: `translateX(${on ? 14 : 0}px)` }} />
      </span>
    )
  }
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!on)}
      className={cn(
        "motion-toggle flex h-5 w-[34px] shrink-0 items-center rounded-full px-0.5",
        on ? "bg-success" : "bg-track",
        disabled && "opacity-45",
      )}
    >
      <span className="motion-toggle-thumb size-4 rounded-full bg-white" style={{ transform: `translateX(${on ? 14 : 0}px)` }} />
    </button>
  )
}

/** 分段控件：容器 surface-3 圆角 9 内边距 2；选中项 surface + 描边 圆角 7 */
export function Segmented<T extends string>({
  items,
  value,
  onChange,
  className,
}: {
  items: { value: T; label: string; disabled?: boolean }[]
  value: T
  onChange?: (v: T) => void
  className?: string
}) {
  // 滑动底板与设置分区导航、平台标签共用同一套测量逻辑
  const { rootRef: root, plateRef: indicator } = useSlidingIndicator<HTMLDivElement, HTMLSpanElement>({
    selector: 'button[aria-pressed="true"]',
    itemsKey: JSON.stringify(items.map((item) => item.value)),
    value,
  })
  return (
    <div ref={root} className={cn("relative isolate inline-flex gap-0.5 rounded-[9px] bg-surface-3 p-0.5", className)}>
      <span ref={indicator} aria-hidden="true" data-motion-indicator className="motion-indicator pointer-events-none absolute left-0 top-0 -z-10 rounded-[7px] border bg-surface" />
      {items.map((it) => {
        const active = it.value === value
        return (
          <button
            key={it.value}
            type="button"
            disabled={it.disabled}
            aria-pressed={active}
            onClick={() => onChange?.(it.value)}
            className={cn(
              "relative grid rounded-[7px] border border-transparent px-3 py-1.5 text-xs transition-colors",
              active
                ? "font-semibold text-text-primary"
                : "text-text-secondary hover:text-text-primary",
              it.disabled && "pointer-events-none opacity-40",
            )}
          >
            <span aria-hidden="true" className="invisible col-start-1 row-start-1 font-semibold">{it.label}</span>
            <span className="col-start-1 row-start-1">{it.label}</span>
          </button>
        )
      })}
    </div>
  )
}

export function SectionTitle({ children }: { children: React.ReactNode }) {
  return <h2 className="text-sm font-semibold text-text-primary">{children}</h2>
}

export function Hint({ children, className }: { children: React.ReactNode; className?: string }) {
  return (
    <p className={cn("text-[11px] leading-[1.5] text-text-tertiary", className)}>{children}</p>
  )
}

export function Divider({ className }: { className?: string }) {
  return <div className={cn("h-px w-full bg-[var(--border)]", className)} />
}
