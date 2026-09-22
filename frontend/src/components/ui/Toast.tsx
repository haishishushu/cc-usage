import { AlertTriangle, CheckCircle2, Info, X, XCircle } from "lucide-react"
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react"
import { cn } from "@/lib/utils"
import {
  TOAST_EXIT_MS,
  markToastExiting,
  pushToast,
  removeToast,
  shiftToasts,
  toastRemaining,
  type ToastInput,
  type ToastItem,
  type ToastTone,
} from "@/lib/toastQueue"

/**
 * 主面板的轻量 Toast（§设计 docs/superpowers/plans/2026-09-18-panel-toast-design.md）。
 *
 * 队列规则全在 lib/toastQueue.ts 里，这里只负责计时、暂停与渲染。
 * 刻意不引第三方库：配色与动效直接用项目既有的 token 和 .motion-* 过渡，
 * 深浅色、reduced-motion 都不需要额外桥接。
 */

export type ToastApi = {
  show: (input: ToastInput) => void
  success: (title: string, detail?: string) => void
  info: (title: string, detail?: string) => void
  warn: (title: string, detail?: string) => void
  danger: (title: string, detail?: string) => void
  dismiss: (id: number) => void
}

type ToastState = {
  items: ToastItem[]
  /** 悬停或聚焦时暂停倒计时，移开后从暂停处续上 */
  setPaused: (paused: boolean) => void
  dismiss: (id: number) => void
}

/** 拆两个 context：调用方只订阅稳定的 api，不被队列变化牵连重渲染 */
const ToastApiContext = createContext<ToastApi | null>(null)
const ToastStateContext = createContext<ToastState | null>(null)

/** 没有 Provider 时的空操作。浏览器预览的 Showcase 也会渲染设置页，不能因此崩 */
const NOOP: ToastApi = {
  show: () => {},
  success: () => {},
  info: () => {},
  warn: () => {},
  danger: () => {},
  dismiss: () => {},
}

export function useToast(): ToastApi {
  return useContext(ToastApiContext) ?? NOOP
}

export function ToastProvider({ children }: { children: React.ReactNode }) {
  const [items, setItems] = useState<ToastItem[]>([])
  const [paused, setPausedState] = useState(false)
  const nextId = useRef(1)
  /**
   * 退场卸载的定时器，按 id 记账。
   *
   * 刻意**不**放进 effect 的 cleanup 里清：cleanup 会在 items 每次变化时跑，
   * 一条新提示入队就会取消上一条已排期的卸载，那条便永远留在 DOM 里——
   * 透明但仍占位，把还看得见的提示一路往下顶。只在组件卸载时统一清。
   */
  const exitTimers = useRef(new Map<number, number>())
  const pausedAt = useRef(0)

  const dismiss = useCallback((id: number) => {
    setItems((list) => markToastExiting(list, id))
  }, [])

  const api = useMemo<ToastApi>(() => {
    const show = (input: ToastInput) => {
      // id 与时间戳在 updater 之外取：updater 必须是纯函数，
      // StrictMode 下它会被调用两次，副作用留在里面会让号跳、时间错
      const id = nextId.current++
      const now = Date.now()
      setItems((list) => pushToast(list, input, id, now))
    }
    const shorthand = (tone: ToastTone) => (title: string, detail?: string) =>
      show({ tone, title, detail })
    return {
      show,
      success: shorthand("success"),
      info: shorthand("info"),
      warn: shorthand("warn"),
      danger: shorthand("danger"),
      dismiss,
    }
  }, [dismiss])

  const setPaused = useCallback((next: boolean) => {
    setPausedState((current) => {
      if (current === next) return current
      if (next) pausedAt.current = Date.now()
      else if (pausedAt.current) {
        setItems((list) => shiftToasts(list, Date.now() - pausedAt.current))
        pausedAt.current = 0
      }
      return next
    })
  }, [])

  // 倒计时：暂停期间整个停掉，恢复时 createdAt 已被平移，按剩余量重新排期
  useEffect(() => {
    if (paused) return
    const now = Date.now()
    const timers = items
      .filter((item) => !item.exiting && Number.isFinite(toastRemaining(item, now)))
      .map((item) =>
        window.setTimeout(
          () => setItems((list) => markToastExiting(list, item.id)),
          toastRemaining(item, now),
        ),
      )
    return () => timers.forEach(window.clearTimeout)
  }, [items, paused])

  // 退场动画跑完再从列表里摘掉；每条只排期一次，且排了就不撤
  useEffect(() => {
    for (const item of items) {
      if (!item.exiting || exitTimers.current.has(item.id)) continue
      exitTimers.current.set(item.id, window.setTimeout(() => {
        exitTimers.current.delete(item.id)
        setItems((list) => removeToast(list, item.id))
      }, TOAST_EXIT_MS))
    }
  }, [items])

  useEffect(() => {
    const timers = exitTimers.current
    return () => {
      timers.forEach(window.clearTimeout)
      timers.clear()
    }
  }, [])

  const state = useMemo<ToastState>(() => ({ items, setPaused, dismiss }), [items, setPaused, dismiss])

  return (
    <ToastApiContext.Provider value={api}>
      <ToastStateContext.Provider value={state}>{children}</ToastStateContext.Provider>
    </ToastApiContext.Provider>
  )
}

const TONE: Record<ToastTone, { icon: typeof CheckCircle2; className: string; iconClass: string }> = {
  success: { icon: CheckCircle2, className: "border-success bg-success-soft text-success-text", iconClass: "text-success-text" },
  info: { icon: Info, className: "border-accent-blue bg-accent-blue-soft text-accent-blue", iconClass: "text-accent-blue" },
  warn: { icon: AlertTriangle, className: "border-warn bg-warn-soft text-warn", iconClass: "text-warn" },
  danger: { icon: XCircle, className: "border-danger bg-danger-soft text-danger", iconClass: "text-danger" },
}

/**
 * 视口。放在主面板根节点内用 absolute 定位：真窗口铺满、浏览器预览是带边距的卡片，
 * 两种模式都能正确落在工具栏下方居中。z-60 高于连接对话框的 z-50——
 * 「换 Key 失败」这类提示必须能盖在对话框之上，否则用户看不见。
 */
export function ToastViewport({ className }: { className?: string }) {
  const state = useContext(ToastStateContext)
  if (!state || state.items.length === 0) return null
  return (
    <div
      aria-live="polite"
      className={cn(
        "pointer-events-none absolute inset-x-0 top-24 z-[60] flex flex-col items-center gap-2 px-4 pt-3",
        className,
      )}
      onMouseEnter={() => state.setPaused(true)}
      onMouseLeave={() => state.setPaused(false)}
      onFocusCapture={() => state.setPaused(true)}
      onBlurCapture={() => state.setPaused(false)}
    >
      {state.items.map((item) => {
        const tone = TONE[item.tone]
        const Icon = tone.icon
        return (
          <div
            key={item.id}
            data-state={item.exiting ? "closed" : "open"}
            role={item.tone === "danger" ? "alert" : undefined}
            className={cn(
              "motion-toast pointer-events-auto flex w-fit max-w-[min(560px,100%)] items-start gap-2.5 rounded-[10px] border px-3.5 py-2.5 shadow-dialog",
              tone.className,
            )}
          >
            <Icon aria-hidden className={cn("mt-px size-4 shrink-0", tone.iconClass)} strokeWidth={2} />
            <div className="flex min-w-0 flex-col gap-0.5">
              <span className="text-xs font-medium leading-[1.5]">{item.title}</span>
              {item.detail && (
                <span className="break-words text-[11px] leading-[1.5] opacity-80">{item.detail}</span>
              )}
            </div>
            <button
              type="button"
              aria-label="关闭提示"
              onClick={() => state.dismiss(item.id)}
              className="-mr-1 -mt-0.5 grid size-5 shrink-0 place-items-center rounded-md opacity-60 transition-opacity hover:opacity-100 focus-visible:outline-2 focus-visible:outline-current"
            >
              <X aria-hidden className="size-3.5" />
            </button>
          </div>
        )
      })}
    </div>
  )
}
