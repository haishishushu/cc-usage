import type { DockEdge, IslandMode } from "../types"

export interface CollapsedView {
  mode: IslandMode
  peek: boolean
  edge: DockEdge | null
}

/** 停靠来源仍有效时回到探出态，由鼠标离开计时器完成最终缩回。 */
export function collapseExpandedView(dockEdge: DockEdge | null): CollapsedView {
  if (dockEdge) return { mode: "docked", peek: true, edge: dockEdge }
  return { mode: "collapsed", peek: false, edge: null }
}

/** 吸附色线只由拖动和当前提示决定，停靠探出态也可以直接拖动。 */
export function snapIndicatorVisible(
  dragging: boolean,
  _mode: IslandMode,
  hasHint: boolean,
): boolean {
  return dragging && hasHint
}
