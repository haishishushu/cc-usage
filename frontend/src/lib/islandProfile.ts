import type { AppSettings, DockEdgeDto, IslandCloneDto } from "./api"

/** 含本体在内的灵动岛总数上限，与后端 island_clones::MAX_ISLANDS 一致。 */
export const MAX_ISLANDS = 8

export interface IslandDock {
  edge: DockEdgeDto | null
  offset: number
  monitor: string | null
}

/** 本体与分身统一成同一形状：岛窗口、右键菜单据此读取本岛的连接与停靠。 */
export interface IslandProfile {
  platform: string
  kind: "auth" | "api"
  connection_id: string | null
  connection_name: string | null
  source_id: string | null
  dock: IslandDock
}

/** 从窗口 URL 读取本岛的分身 id；本体没有 clone 参数，返回 null。 */
export function islandIdFromSearch(search: string): string | null {
  const value = new URLSearchParams(search).get("clone")
  return value ? value : null
}

type ProfileSource = Pick<AppSettings,
  "island_platform" | "island_kind" | "island_connection_id" | "island_connection_name" | "island_source_id" | "dock"
> & { island_clones?: IslandCloneDto[] }

function fromClone(clone: IslandCloneDto): IslandProfile {
  return {
    platform: clone.platform,
    kind: clone.kind,
    connection_id: clone.connection_id,
    connection_name: clone.connection_name,
    source_id: clone.source_id,
    dock: clone.dock,
  }
}

/**
 * 取某个岛的配置。分身记录找不到时（刚被销毁、窗口尚未关闭）回退本体配置，
 * 避免窗口在最后一帧崩溃；调用方不应据此写回设置。
 */
export function islandProfile(settings: ProfileSource, cloneId: string | null): IslandProfile {
  if (cloneId) {
    const clone = settings.island_clones?.find((item) => item.id === cloneId)
    if (clone) return fromClone(clone)
  }
  return {
    platform: settings.island_platform,
    kind: settings.island_kind,
    connection_id: settings.island_connection_id,
    connection_name: settings.island_connection_name,
    source_id: settings.island_source_id,
    dock: settings.dock,
  }
}

/** 灵动岛总数（含本体）。右键菜单右上角显示的就是它。 */
export function islandCount(settings: { island_clones?: IslandCloneDto[] }): number {
  return 1 + (settings.island_clones?.length ?? 0)
}

/** 停靠事件带岛 id；本体为 null。只处理发给自己的，避免别的岛停靠时重置本岛形态。 */
export function isOwnIslandEvent(event: { island?: string | null }, cloneId: string | null): boolean {
  return (event.island ?? null) === cloneId
}

/** 菜单里两项分身操作的可用性：至少保留一个，最多 MAX_ISLANDS 个。 */
export function cloneActions(total: number): { canCreate: boolean; canDestroy: boolean } {
  return { canCreate: total < MAX_ISLANDS, canDestroy: total > 1 }
}
