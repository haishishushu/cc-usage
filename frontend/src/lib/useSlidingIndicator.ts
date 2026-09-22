import { useCallback, useLayoutEffect, useRef } from "react"

/**
 * 选中标记的滑动底板。
 *
 * 原先只有顶部「总览 / 设置」分段控件有这套逻辑（写在 Segmented 里）。设置分区
 * 导航与平台标签也需要同样的观感，与其抄三份，不如把它抽出来共用——包括已经过
 * 性能评审的两条纪律：**先集中读布局再写样式**（避免读写交替强制同步布局），
 * 以及**只在选项结构变化时重建 ResizeObserver**（数据刷新不重新绑定监听）。
 *
 * 底板只动 transform 与尺寸，不参与文档流，因此移动时不会牵动任何兄弟节点。
 */

export type IndicatorMode =
  /** 整块底板：铺满选中项，用于分段控件与导航胶囊 */
  | "plate"
  /** 下划线：只跟宽度与横向位置走，高度由 CSS 定，用于标签页 */
  | "underline"

export function useSlidingIndicator<Root extends HTMLElement, Plate extends HTMLElement>({
  /** 选中项的选择器，各处的选中语义不同（aria-pressed / aria-current / aria-selected） */
  selector,
  mode = "plate",
  /** 选项结构的标识；只有它变化才重建监听 */
  itemsKey,
  /** 选中值；变化时重新测量 */
  value,
}: {
  selector: string
  mode?: IndicatorMode
  itemsKey: string
  value: string
}) {
  const rootRef = useRef<Root>(null)
  const plateRef = useRef<Plate>(null)

  const measure = useCallback(() => {
    const container = rootRef.current
    const plate = plateRef.current
    if (!container || !plate) return
    const selected = container.querySelector<HTMLElement>(selector)
    if (!selected) { plate.hidden = true; return }
    // 先集中读取布局，再写入样式，避免读写交替强制刷新布局。
    const { offsetWidth: width, offsetHeight: height, offsetLeft: left, offsetTop: top } = selected
    plate.hidden = false
    plate.style.width = `${width}px`
    if (mode === "plate") {
      plate.style.height = `${height}px`
      plate.style.transform = `translate(${left}px, ${top}px)`
    } else {
      // 下划线贴着容器底边，只跟横向走；高度交给 CSS，避免这里改动布局属性
      plate.style.transform = `translateX(${left}px)`
    }
  }, [selector, mode])

  useLayoutEffect(measure, [value, itemsKey, measure])

  useLayoutEffect(() => {
    const container = rootRef.current
    if (!container) return
    const observer = new ResizeObserver(measure)
    observer.observe(container)
    container.querySelectorAll("button").forEach((button) => observer.observe(button))
    return () => observer.disconnect()
  }, [itemsKey, measure])

  return { rootRef, plateRef, measure }
}
