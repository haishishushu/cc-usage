import { useEffect, useLayoutEffect, useRef, useState } from "react"

export const MOTION_EASE = "cubic-bezier(0.2, 0.8, 0.2, 1)"
export const reducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches

/**
 * 刷新柔光掠过整卡的周期，与 index.css 的 island-refresh-shimmer 同一数值。
 * 卡片与停靠条共用，保证两种形态节奏一致。
 */
export const SHIMMER_CYCLE_MS = 1100

/**
 * 柔光的最少轮数。额度查询有 5 分钟缓存，命中时几乎瞬时返回，
 * 只掠一轮容易没看清就结束；两轮分量够又不拖沓。
 *
 * 刻意是**下限**而不是定值：超过这个时长仍未查完时柔光继续掠，
 * 否则会出现「动画停了、数据却还没回来」——用户以为刷新完了，
 * 再点一次又会被防重入闸门挡掉，白等。
 */
export const SHIMMER_MIN_CYCLES = 2

/**
 * 还要多久才能收尾：对齐到轮次边界，且不早于 `minCycles` 轮。
 *
 * 起跑后立刻要停时返回完整的最小时长，这正是「至少完整掠 N 轮」的来源：
 * 查询命中缓存秒回时，反馈不会一闪而过。
 */
export function cycleExitDelay(elapsedMs: number, cycleMs: number, minCycles = 1): number {
  const toBoundary = cycleMs - (elapsedMs % cycleMs)
  const minTotal = cycleMs * minCycles
  // 补到边界后若已满最小时长就按边界收；否则等满最小时长（那一刻同样是边界）
  return elapsedMs + toBoundary >= minTotal ? toBoundary : minTotal - elapsedMs
}

/**
 * 循环反馈动画的收尾闸门：`active` 落下后不立刻摘除，而是补齐到轮次边界，
 * 且不早于 `minCycles` 轮。
 *
 * 循环动画中途摘除会从当前帧硬切回起点——柔光跳回卡外重新掠一遍，
 * 读起来就是「卡了一下」。轮次交界处柔光正好在卡外且透明，此刻移除毫无痕迹。
 *
 * 用计时而非 animationiteration 事件：reduced-motion 的全局兜底把
 * animation-iteration-count 压成 1，该事件永不触发，元素会永久留在 DOM，
 * 并且此后每次刷新都不再有动画。
 */
export function useCycleExit(
  startKey: number,
  active: boolean,
  cycleMs: number,
  minCycles = 1,
): boolean {
  const [running, setRunning] = useState(false)
  // 动画真正挂载的时刻；停稳后清零，下一次才重新计时。
  // 动画还在播时不重记起点：CSS 动画并未重启，重置会让轮次边界算错位。
  const startedAt = useRef(0)
  // 只认序号的「变化」，不认它的「值」。挂载时按当前值对齐：双击展开/收起会
  // 重建组件，若按 startKey > 0 起跑，刷新过一次之后每次切换形态都会误播一轮。
  const previousKey = useRef(startKey)

  // 起跑看序号而不是 active：查询命中缓存时 active 的 true→false 可能被
  // React 合并进同一批更新，只看 active 会让整段反馈动画彻底丢失。
  useEffect(() => {
    if (previousKey.current === startKey) return
    previousKey.current = startKey
    if (startedAt.current === 0) startedAt.current = performance.now()
    setRunning(true)
  }, [startKey])

  useEffect(() => {
    if (active || !running) return
    const timer = window.setTimeout(() => {
      startedAt.current = 0
      setRunning(false)
    }, cycleExitDelay(performance.now() - startedAt.current, cycleMs, minCycles))
    return () => window.clearTimeout(timer)
  }, [active, running, cycleMs, minCycles])

  return running
}

/** 退出期间保留上一份内容；重新打开会取消旧计时器，避免误卸载。 */
export function useExitPresence<T>(value: T | null, duration = 140) {
  const [retained, setRetained] = useState(value)
  useLayoutEffect(() => {
    if (value !== null) {
      setRetained(value)
      return
    }
    const timer = window.setTimeout(() => setRetained(null), reducedMotion() ? 0 : duration)
    return () => window.clearTimeout(timer)
  }, [value, duration])
  return { rendered: value ?? retained, exiting: value === null && retained !== null }
}

/** 只在交互标识变化时重播，不重挂载内容，不影响表单、焦点与滚动位置。 */
export function useContentMotion(key: string, duration = 180, distance = 4) {
  const ref = useRef<HTMLDivElement>(null)
  const previousKey = useRef(key)
  const interrupted = useRef<{ opacity: string; transform: string } | null>(null)
  useLayoutEffect(() => {
    if (previousKey.current === key) return
    previousKey.current = key
    const element = ref.current
    if (!element || reducedMotion()) { interrupted.current = null; return }
    const start = interrupted.current
    interrupted.current = null
    const animation = element.animate([
      { opacity: start?.opacity ?? 0.85, ...(distance ? { transform: start?.transform ?? `translateY(${distance}px)` } : {}) },
      { opacity: 1, ...(distance ? { transform: "translateY(0)" } : {}) },
    ], { duration, easing: MOTION_EASE })
    const query = window.matchMedia("(prefers-reduced-motion: reduce)")
    const stop = () => { if (query.matches) animation.cancel() }
    query.addEventListener("change", stop)
    return () => {
      // 连续切换从当前可见帧接续，不能反复跳回固定的淡入起点。
      if (animation.playState === "running" || animation.playState === "paused") {
        const style = getComputedStyle(element)
        interrupted.current = { opacity: style.opacity, transform: style.transform }
      } else interrupted.current = null
      animation.cancel()
      query.removeEventListener("change", stop)
    }
  }, [key, duration, distance])
  return ref
}
