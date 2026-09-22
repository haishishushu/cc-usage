/**
 * 更新功能的纯状态逻辑。
 *
 * 检查与安装走 Tauri updater 插件（后端命令封装，见 useUpdate.ts），
 * 这里只放界面推导所需的纯函数：忽略版本的记忆、进度换算与字节数格式化，
 * 便于 node --test 直接验证。
 */

export type UpdatePhase = "idle" | "checking" | "available" | "downloading" | "installing" | "ready"

export interface UpdateInfo {
  currentVersion: string
  availableVersion: string
  notes?: string | null
  pubDate?: string | null
}

export interface UpdateProgress {
  downloaded: number
  /** 总量未知时为 null（分块下载早期拿不到 content-length） */
  total: number | null
}

/** 忽略的版本号存在 localStorage；同版本不再亮绿灯，更新版本仍会提示 */
export const DISMISSED_KEY = "ccusage:update:dismissedVersion"

type StorageLike = Pick<Storage, "getItem" | "setItem" | "removeItem">

export function readDismissedVersion(storage: StorageLike = localStorage): string | null {
  try {
    return storage.getItem(DISMISSED_KEY)
  } catch {
    return null
  }
}

export function writeDismissedVersion(version: string, storage: StorageLike = localStorage): void {
  try {
    storage.setItem(DISMISSED_KEY, version)
  } catch {
    // 存不进去就存不进去：大不了下次启动再提示一次
  }
}

export function clearDismissedVersion(storage: StorageLike = localStorage): void {
  try {
    storage.removeItem(DISMISSED_KEY)
  } catch {
    // 同上，清除失败不影响主流程
  }
}

/** 绿灯是否亮起：有可用更新，且不是用户忽略过的那个版本 */
export function isUpdateRelevant(availableVersion: string | null, dismissedVersion: string | null): boolean {
  if (!availableVersion) return false
  return availableVersion !== dismissedVersion
}

/** 下载进度 0–100 取整；总量未知时返回 null，界面显示不定长进度 */
export function progressPercent(downloaded: number, total: number | null): number | null {
  if (total === null || total <= 0) return null
  const pct = Math.round((downloaded / total) * 100)
  return Math.min(100, Math.max(0, pct))
}

/** 面板与对话框共用的字节数展示：按量级选单位，一位小数 */
export function formatBytes(n: number): string {
  if (n < 1024) return `${Math.max(0, Math.round(n))} B`
  const kb = n / 1024
  if (kb < 1024) return `${kb.toFixed(1)} KB`
  const mb = kb / 1024
  if (mb < 1024) return `${mb.toFixed(1)} MB`
  return `${(mb / 1024).toFixed(1)} GB`
}
