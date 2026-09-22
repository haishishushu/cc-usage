import { useLayoutEffect, useRef, type ReactNode } from "react"
import { MOTION_EASE } from "@/lib/motion"
import { isTauri } from "@/lib/api"

/** 预留较大窗口高度，岛体自行插值；收起完成后才释放原生窗口空间。 */
export function IslandMotion({ expanded, dragging, children }: {
  expanded: boolean
  dragging: boolean
  children: ReactNode
}) {
  const reserveRef = useRef<HTMLDivElement>(null)
  const frameRef = useRef<HTMLDivElement>(null)
  const contentRef = useRef<HTMLDivElement>(null)
  const initialized = useRef(false)
  const previousExpanded = useRef(expanded)

  useLayoutEffect(() => {
    const reserve = reserveRef.current
    const frame = frameRef.current
    const content = contentRef.current
    if (!reserve || !frame || !content) return
    let animateContent = previousExpanded.current !== expanded
    previousExpanded.current = expanded
    const preference = window.matchMedia("(prefers-reduced-motion: reduce)")
    let heightAnimation: Animation | undefined
    let contentAnimations: Animation[] = []
    let waitingForWindow: (() => void) | undefined
    let target = -1
    const stopWaiting = () => {
      if (waitingForWindow) window.removeEventListener("resize", waitingForWindow)
      waitingForWindow = undefined
    }
    const settle = () => {
      stopWaiting()
      heightAnimation?.cancel()
      contentAnimations.forEach((animation) => animation.cancel())
      contentAnimations = []
      heightAnimation = undefined
      frame.style.height = `${content.offsetHeight}px`
      reserve.style.height = `${content.offsetHeight}px`
    }
    const measure = () => {
      const next = content.offsetHeight
      if (next === target) return
      target = next
      // computed height 不受用户设置的 CSS zoom 影响；中途反向从当前帧继续。
      const from = Number.parseFloat(getComputedStyle(frame).height)
      stopWaiting()
      heightAnimation?.cancel()
      contentAnimations.forEach((animation) => animation.cancel())
      contentAnimations = []
      if (!initialized.current || dragging || preference.matches || Math.abs(next - from) < 1) {
        initialized.current = true
        animateContent = false
        settle()
        return
      }
      reserve.style.height = `${Math.max(from, next)}px`
      frame.style.height = `${next}px`
      heightAnimation = frame.animate([{ height: `${from}px` }, { height: `${next}px` }], {
        duration: expanded ? 280 : 220,
        easing: MOTION_EASE,
        fill: "both",
      })
      heightAnimation.onfinish = settle
      // 标题与额度始终可读，不把整座岛清空后再淡入。
      if (expanded && animateContent) {
        animateContent = false
        contentAnimations = Array.from(content.querySelectorAll(".island-shell > :not(:first-child)"))
          .map((element) => element.animate([
            { opacity: 0.75, transform: "translateY(6px)" },
            { opacity: 1, transform: "translateY(0)" },
          ], { duration: 160, delay: 60, easing: MOTION_EASE, fill: "backwards" }))
      }
      // 原生 resize 是异步 IPC，不能假设固定延时后窗口已变大。
      // 从实际 viewport 确认空间足够后再播放，避免低性能机器上被窗口裁切。
      if (isTauri && next > from) {
        heightAnimation.pause()
        contentAnimations.forEach((animation) => animation.pause())
        waitingForWindow = () => {
          if (window.innerHeight + 1 < reserve.getBoundingClientRect().bottom) return
          stopWaiting()
          heightAnimation?.play()
          contentAnimations.forEach((animation) => animation.play())
        }
        window.addEventListener("resize", waitingForWindow)
        waitingForWindow()
      }
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(content)
    const onPreference = () => { if (preference.matches) settle() }
    const onSizeLimit = (event: Event) => {
      const maxHeight = (event as CustomEvent<number | null>).detail
      const bounds = reserve.getBoundingClientRect()
      const zoom = bounds.width / reserve.offsetWidth || 1
      const bottomPadding = reserve.parentElement ? Number.parseFloat(getComputedStyle(reserve.parentElement).paddingBottom) || 0 : 0
      content.style.maxHeight = maxHeight === null ? "" : `${Math.max(1, (maxHeight - bounds.top) / zoom - bottomPadding)}px`
      content.style.overflowY = maxHeight === null ? "" : "auto"
      content.style.overflowX = maxHeight === null ? "" : "hidden"
      // max-height 改变后重新测量，目标可达时沿用已有的 viewport 确认流程。
      measure()
    }
    window.addEventListener("island-size-limit", onSizeLimit)
    preference.addEventListener("change", onPreference)
    return () => {
      observer.disconnect()
      stopWaiting()
      preference.removeEventListener("change", onPreference)
      window.removeEventListener("island-size-limit", onSizeLimit)
      // 清理旧动画前记住可见高度，下一次 effect 才能平滑反向。
      frame.style.height = getComputedStyle(frame).height
      heightAnimation?.cancel()
      contentAnimations.forEach((animation) => animation.cancel())
    }
  }, [expanded, dragging])

  return (
    <div ref={reserveRef} className="w-[400px] shrink-0" data-island-motion={expanded ? "expanded" : "collapsed"}>
      <div ref={frameRef} className="island-morph">
        <div ref={contentRef}>{children}</div>
      </div>
    </div>
  )
}
