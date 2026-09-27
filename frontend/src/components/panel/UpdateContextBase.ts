import { createContext } from "react"
import type { UpdateInfo, UpdatePhase, UpdateProgress } from "@/lib/updateState"

/**
 * 更新功能的 React Context 对象，独立于 Provider 组件存放。
 *
 * 单独成文件是刻意的：HMR 更新 Provider 的检查/安装逻辑时不会重新执行
 * createContext，避免 dev 长会话中出现「Provider 与消费组件各持一个
 * context 实例」导致消费方读到 null 的分裂问题。
 */
export interface UpdateContextValue {
  phase: UpdatePhase
  info: UpdateInfo | null
  progress: UpdateProgress
  /** 左上角更新提示是否显示：有新版本且未被忽略 */
  visible: boolean
  error: string | null
  /** 手动检查（设置页）。返回是否有更新；失败抛错由调用方 toast */
  checkNow: () => Promise<boolean>
  /** 点击版本入口：下载并校验，随后进入安装界面 */
  startDownload: () => void
  /** 点击安装：使用已下载的安装包安装并重启 */
  installDownloaded: () => void
  /** 忽略此版本：同版本不再亮灯，更新版本仍会提示 */
  dismiss: () => void
}

export const UpdateContext = createContext<UpdateContextValue | null>(null)
