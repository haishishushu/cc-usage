import { useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react"
import { api, isTauri, listenEvent, type UpdateCheckDto, type UpdateProgressDto } from "@/lib/api"
import {
  isUpdateRelevant,
  readDismissedVersion,
  writeDismissedVersion,
  type UpdateInfo,
  type UpdatePhase,
  type UpdateProgress,
} from "@/lib/updateState"
import { UpdateContext, type UpdateContextValue } from "./UpdateContextBase"

/**
 * 更新状态（§更新，画布 17）。
 *
 * 点击绿色按钮 = 直接进入安装（无二次确认）：startInstall 立即调后端
 * install_update_and_restart，下载进度经 update-download-progress 事件推送；
 * Windows 上安装由后端清理托盘与单实例锁后替换文件并自动重启。
 * 浏览器预览（非 Tauri）走内置模拟流程，便于对照画布验收交互。
 *
 * Context 对象在 ./UpdateContextBase（独立文件，防 HMR 实例分裂），
 * 这里 re-export 保持既有 import 路径不变。
 */

export { UpdateContext }
export type { UpdateContextValue }


const MOCK_INFO: UpdateInfo = {
  currentVersion: "0.1.0",
  availableVersion: "0.2.0",
  notes: "灵动岛新增会话耗时角标；修复代理转发偶发中断；连接管理支持批量导入。",
  pubDate: "2026-09-18",
}

export function UpdateProvider({ children }: { children: ReactNode }) {
  const [phase, setPhase] = useState<UpdatePhase>("idle")
  const [info, setInfo] = useState<UpdateInfo | null>(null)
  const [progress, setProgress] = useState<UpdateProgress>({ downloaded: 0, total: null })
  const [dismissed, setDismissed] = useState<string | null>(() => readDismissedVersion())
  const [error, setError] = useState<string | null>(null)
  const busy = useRef(false)
  const timers = useRef<number[]>([])

  useEffect(() => () => timers.current.forEach((t) => window.clearTimeout(t)), [])

  const applyCheck = useCallback((dto: UpdateCheckDto) => {
    if (!dto.available) {
      setInfo(null)
      return false
    }
    setInfo({
      currentVersion: dto.current_version,
      availableVersion: dto.available_version,
      notes: dto.notes,
      pubDate: dto.pub_date,
    })
    return true
  }, [])

  const checkNow = useCallback(async () => {
    if (busy.current) return false
    busy.current = true
    setPhase("checking")
    setError(null)
    try {
      if (!isTauri) {
        // 预览：按画布示例固定给出 v0.2.0
        setInfo(MOCK_INFO)
        return true
      }
      return applyCheck(await api.checkAppUpdate())
    } catch (reason) {
      setError(`检查更新失败：${String(reason)}`)
      throw reason
    } finally {
      busy.current = false
      setPhase((current) => (current === "checking" ? "idle" : current))
    }
  }, [applyCheck])

  const startInstall = useCallback(() => {
    if (busy.current) return
    if (!isTauri) {
      // 预览：模拟 下载 → 校验安装 → 就绪 的完整节奏（约 3.4s）
      busy.current = true
      setPhase("downloading")
      setProgress({ downloaded: 0, total: 18.2 * 1024 * 1024 })
      const step = 1024 * 1024
      const tick = (downloaded: number) => {
        setProgress({ downloaded: Math.min(downloaded, 18.2 * 1024 * 1024), total: 18.2 * 1024 * 1024 })
        if (downloaded < 18.2 * 1024 * 1024) {
          timers.current.push(window.setTimeout(() => tick(downloaded + step * 2), 120))
        } else {
          setPhase("installing")
          timers.current.push(window.setTimeout(() => {
            setPhase("ready")
            busy.current = false
          }, 900))
        }
      }
      timers.current.push(window.setTimeout(() => tick(step * 2), 200))
      return
    }
    busy.current = true
    setPhase("downloading")
    setProgress({ downloaded: 0, total: null })
    setError(null)
    let off: (() => void) | undefined
    const unlisten = listenEvent<UpdateProgressDto>("update-download-progress", (p) => {
      setProgress({ downloaded: p.downloaded, total: p.total })
      if (p.total !== null && p.downloaded >= p.total) setPhase("installing")
    })
    void unlisten
      .then((unsubscribe) => {
        off = unsubscribe
        return api.installUpdate()
      })
      .then(() => {
        // Windows 上走到这里通常意味着进程本该已被替换重启；仍存活时按就绪展示
        setPhase("ready")
      })
      .catch((reason) => {
        setError(`更新失败：${String(reason)}`)
        setPhase((current) => (current === "ready" ? current : "available"))
      })
      .finally(() => {
        busy.current = false
        off?.()
      })
  }, [])

  const dismiss = useCallback(() => {
    if (!info) return
    writeDismissedVersion(info.availableVersion)
    setDismissed(info.availableVersion)
  }, [info])

  const restartPreview = useCallback(() => {
    if (!isTauri) window.location.reload()
  }, [])

  // 启动 1 秒后静默检查；失败不打扰（画布标注：静默重试下次启动）
  useEffect(() => {
    const timer = window.setTimeout(() => {
      void checkNow().catch(() => {})
    }, 1000)
    return () => window.clearTimeout(timer)
  }, [checkNow])

  const value = useMemo<UpdateContextValue>(
    () => ({
      phase,
      info,
      progress,
      visible:
        (phase === "idle" || phase === "available") && isUpdateRelevant(info?.availableVersion ?? null, dismissed),
      error,
      checkNow,
      startInstall,
      dismiss,
      restartPreview,
    }),
    [phase, info, progress, dismissed, error, checkNow, startInstall, dismiss, restartPreview],
  )

  return <UpdateContext.Provider value={value}>{children}</UpdateContext.Provider>
}

export function useUpdate(): UpdateContextValue {
  const value = useContext(UpdateContext)
  if (!value) throw new Error("useUpdate 必须在 UpdateProvider 内使用")
  return value
}
