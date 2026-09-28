import { lazy, Suspense, type ComponentProps } from "react"
import type { IslandConnectionSwitcher as Switcher } from "./IslandConnectionSwitcher"

// 连接切换器带 Radix 下拉菜单，只在展开态出现：按需加载，收缩与停靠态不背这份脚本。
const IslandConnectionSwitcher = lazy(() => import("./IslandConnectionSwitcher")
  .then((module) => ({ default: module.IslandConnectionSwitcher })))

export function LazyConnectionSwitcher(props: ComponentProps<typeof Switcher>) {
  return (
    <Suspense fallback={null}>
      <IslandConnectionSwitcher {...props} />
    </Suspense>
  )
}
