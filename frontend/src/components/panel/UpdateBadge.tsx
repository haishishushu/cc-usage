import { CircleArrowUp, Check, LoaderCircle } from "lucide-react"
import { useContext } from "react"
import { UpdateContext } from "./UpdateContext"

/**
 * 主面板左上角的新版本提示；两个入口共用下载和安装状态。
 * 无 Provider 时（预览外壳等场景）静默不渲染。
 */
export function UpdateBadge() {
  const update = useContext(UpdateContext)
  if (!update) return null
  const { phase, visible, info, startDownload } = update
  const relevant = phase === "idle" || phase === "available" ? visible : true
  if (phase === "checking" || !info || !relevant) return null

  const busy = phase === "downloading" || phase === "installing"
  const waiting = phase === "downloaded" || phase === "downloadError"
  const ready = phase === "ready"
  const title = ready
    ? `v${info.availableVersion} 安装完成，应用即将重启`
    : busy
      ? `正在更新 v${info.availableVersion}…`
      : waiting ? `v${info.availableVersion} 安装界面已打开` : `发现新版本 v${info.availableVersion}，点击下载`

  return (
    <button
      type="button"
      title={title}
      aria-label={title}
      disabled={busy || ready || waiting}
      onClick={() => {
        if (!busy && !ready && !waiting) startDownload()
      }}
      className={`flex h-6 shrink-0 items-center gap-1 rounded-full px-2 text-[11px] font-semibold ${
        busy || ready || waiting ? "bg-success-soft text-success-text" : "bg-success-soft text-success-text hover:bg-success-text hover:text-white dark:hover:bg-success dark:hover:text-[#0f2a1a] transition-colors"
      }`}
    >
      {ready ? (
        <Check className="size-3.5" aria-hidden />
      ) : busy ? (
        <LoaderCircle className="size-3.5 animate-spin" aria-hidden />
      ) : (
        <CircleArrowUp className="size-3.5" aria-hidden />
      )}
      <span>{busy ? "更新中" : waiting ? "等待安装" : ready ? "已安装" : `发现 v${info.availableVersion}`}</span>
    </button>
  )
}
