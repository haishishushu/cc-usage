/**
 * 有连接 ID 的刷新事件只作用于对应连接；没有 ID 的事件表示全局刷新。
 * 连接管理、监听器发现和托盘刷新仍使用全局事件，避免改变既有入口语义。
 */
export function shouldRefreshConnection(
  eventConnectionId: string | null | undefined,
  currentConnectionId: string | null | undefined,
): boolean {
  return eventConnectionId == null || eventConnectionId === currentConnectionId
}
