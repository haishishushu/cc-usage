/**
 * 编辑连接弹窗的保存计划：把「名称输入 + Key 输入」算成要执行的动作。
 *
 * 名称校验与后端 `connections::validate_name` 同一口径：
 * 去首尾空白、1–80 个字符。脱敏标识由后端从 Key 派生，不可在此编辑。
 */
export interface ConnectionEditPlan {
  /** 与原名称不同时才有：校验过的新名称 */
  rename?: string
  /** 填了新 Key 时才有：去空白后的 Key */
  replaceKey?: string
  /** 校验失败原因，直接作为界面提示 */
  error?: string
}

export function planConnectionEdit(input: {
  originalName: string
  name: string
  secret?: string
}): ConnectionEditPlan {
  const name = input.name.trim()
  // 与后端 chars().count() 对齐：按 Unicode 字符数而不是 UTF-16 长度
  if (!name || [...name].length > 80) {
    return { error: "连接名称应为 1 到 80 个字符" }
  }
  const replaceKey = input.secret?.trim() || undefined
  const rename = name !== input.originalName ? name : undefined
  const plan: ConnectionEditPlan = {}
  if (rename) plan.rename = rename
  if (replaceKey) plan.replaceKey = replaceKey
  return plan
}
