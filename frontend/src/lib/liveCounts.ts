import { createContext, useContext, useSyncExternalStore } from "react"

/**
 * 追数动画的逐帧数值，放在 React 状态之外：每帧只通知订阅了具体键的叶子组件，
 * 灵动岛其余部分（额度、费用、会话列表结构）不随每一帧重新渲染。
 * 键约定：合计为 `total`，单个会话为 `session:<id>`。
 */
export class LiveCounts {
  private values: Record<string, number> = {}
  private listeners = new Set<() => void>()

  get = (key: string): number | undefined => this.values[key]

  set(values: Record<string, number>) {
    this.values = values
    this.listeners.forEach((listener) => listener())
  }

  subscribe = (listener: () => void) => {
    this.listeners.add(listener)
    return () => { this.listeners.delete(listener) }
  }
}

export const LiveCountsContext = createContext<LiveCounts | null>(null)

const detached = () => () => {}

/** 订阅某个追数键的当前值；没有提供存储或键时为 undefined。值不变的帧不会触发重渲染。 */
export function useLiveCount(key: string | undefined): number | undefined {
  const store = useContext(LiveCountsContext)
  return useSyncExternalStore(
    store && key ? store.subscribe : detached,
    () => (store && key ? store.get(key) : undefined),
  )
}
