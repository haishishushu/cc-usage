export type ConnectionActionStatus = "connected" | "paused" | "expired" | "invalid" | "offline"

export type ConnectionToggleAction = {
  label: "启用" | "断开"
  action: "select" | "clear" | "reconnect" | "unavailable"
  disabled: boolean
}

/** 将灵动岛选择与连接暂停状态收敛为一个互斥按钮。 */
export function connectionToggleAction(status: ConnectionActionStatus, using: boolean): ConnectionToggleAction {
  if (using) return { label: "断开", action: "clear", disabled: false }
  if (status === "paused") return { label: "启用", action: "reconnect", disabled: false }
  if (status !== "connected") return { label: "启用", action: "unavailable", disabled: true }
  return { label: "启用", action: "select", disabled: false }
}
