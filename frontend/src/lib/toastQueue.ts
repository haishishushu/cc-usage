/**
 * 主面板 Toast 队列的纯逻辑（§设计 docs/superpowers/plans/2026-09-18-panel-toast-design.md）。
 *
 * 这里只算「队列该变成什么样」，不碰 DOM、不设计时器：
 * 计时交给 ToastProvider 的 effect，退场动画交给 CSS。
 * 好处是入队去重、条数上限、悬停暂停这些容易出错的规则可以直接单测。
 */

export type ToastTone = "success" | "info" | "warn" | "danger"

export type ToastInput = {
  tone: ToastTone
  title: string
  /** 失败原因等补充信息，单独一行小字 */
  detail?: string
  /** 覆盖该 tone 的默认停留时长；0 表示不自动消失，只能手动关 */
  duration?: number
}

export type ToastItem = {
  id: number
  tone: ToastTone
  title: string
  detail?: string
  duration: number
  /** 计时起点。悬停暂停后整体后移，等价于「这段时间没走过」 */
  createdAt: number
  /** 退场动画期间仍留在列表里，动画跑完才真正移除 */
  exiting: boolean
}

/**
 * 失败比成功停留久：成功只是确认「点到了」，扫一眼就够；
 * 失败要读完原因，2 秒关掉等于没说。
 */
export const TOAST_DURATION: Record<ToastTone, number> = {
  success: 2200,
  info: 2200,
  warn: 3600,
  danger: 4800,
}

/** 同屏最多 3 条：再多就挡住内容区，且人也读不过来 */
export const TOAST_LIMIT = 3

/** 退场动画时长，与 index.css 的 .motion-toast[data-state="closed"] 同一数值 */
export const TOAST_EXIT_MS = 140

/** 未进入退场的条目，即视觉上真正占位的那些 */
export function visibleToasts(list: ToastItem[]): ToastItem[] {
  return list.filter((item) => !item.exiting)
}

function sameContent(item: ToastItem, input: ToastInput): boolean {
  return item.tone === input.tone && item.title === input.title && item.detail === input.detail
}

/**
 * 入队。两条规则：
 *
 * 1. 同内容且尚未退场 → 不新增，只把计时拨回起点。连点「更新」时按钮会连发
 *    同一条提示，堆成一摞既吵又挡内容；原地续时既说明「又执行了一次」，
 *    位置也不跳。已在退场的不复用——它正在淡出，复用会看到一条提示诡异地倒放。
 * 2. 超过上限 → 最旧的一条转入退场而不是直接删。直接删会让它从当前帧硬切消失，
 *    读起来像闪了一下。
 */
export function pushToast(
  list: ToastItem[],
  input: ToastInput,
  id: number,
  now: number,
): ToastItem[] {
  const duplicate = list.find((item) => !item.exiting && sameContent(item, input))
  if (duplicate) {
    return list.map((item) => (item === duplicate ? { ...item, createdAt: now } : item))
  }

  const next = [
    ...list,
    {
      id,
      tone: input.tone,
      title: input.title,
      detail: input.detail,
      duration: input.duration ?? TOAST_DURATION[input.tone],
      createdAt: now,
      exiting: false,
    },
  ]

  const overflow = visibleToasts(next).length - TOAST_LIMIT
  if (overflow <= 0) return next
  // 只挤未退场的，退场中的不占名额也不该被重复标记
  const evicted = new Set(visibleToasts(next).slice(0, overflow).map((item) => item.id))
  return next.map((item) => (evicted.has(item.id) ? { ...item, exiting: true } : item))
}

/** 转入退场：超时、手动关闭、被挤出共用这一条路径 */
export function markToastExiting(list: ToastItem[], id: number): ToastItem[] {
  return list.map((item) => (item.id === id && !item.exiting ? { ...item, exiting: true } : item))
}

/** 退场动画结束后真正移除 */
export function removeToast(list: ToastItem[], id: number): ToastItem[] {
  return list.filter((item) => item.id !== id)
}

/** 还要停留多久；duration 为 0 表示不自动消失 */
export function toastRemaining(item: ToastItem, now: number): number {
  if (item.duration <= 0) return Infinity
  return Math.max(0, item.createdAt + item.duration - now)
}

/**
 * 悬停暂停结束后，把所有计时起点整体后移暂停时长。
 * 不这么做的话，悬停期间的时间照算，用户刚把鼠标移开提示就没了。
 */
export function shiftToasts(list: ToastItem[], offsetMs: number): ToastItem[] {
  if (offsetMs <= 0) return list
  return list.map((item) => ({ ...item, createdAt: item.createdAt + offsetMs }))
}
