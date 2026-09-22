import { CircleArrowUp, Check, LoaderCircle } from "lucide-react"
import { useContext } from "react"
import { UpdateContext } from "./UpdateContext"

/**
 * 标题栏绿色更新按钮（画布 17 · A/A2）。
 *
 * 仅在有新版本时渲染：绿圈默认态可点击（点击 = 直接进入安装），
 * 下载 / 安装中转圈不可点，就绪态显示对勾等待后端自动重启。
 * 无 Provider 时（预览外壳等场景）静默不渲染。
 */
export function UpdateBadge() {
  const update = useContext(UpdateContext)
  if (!update) return null
  const { phase, visible, info, startInstall } = update
  const relevant = phase === "idle" || phase === "available" ? visible : true
  if (phase === "checking" || !info || !relevant) return null

  const busy = phase === "downloading" || phase === "installing"
  const ready = phase === "ready"
  const title = ready
    ? `v${info.availableVersion} 安装完成，应用即将重启`
    : busy
      ? `正在下载 v${info.availableVersion}…`
      : `更新到 v${info.availableVersion} · 点击立即安装`

  return (
    <button
      type="button"
      title={title}
      aria-label={title}
      disabled={busy || ready}
      onClick={() => {
        if (!busy && !ready) startInstall()
      }}
      className={`grid size-6 shrink-0 place-items-center rounded-full ${
        busy || ready ? "bg-success-soft" : "bg-success-soft hover:bg-success transition-colors"
      }`}
    >
      {ready ? (
        <Check className="size-3.5 text-success-text" aria-hidden />
      ) : busy ? (
        <LoaderCircle className="size-3.5 animate-spin text-success-text" aria-hidden />
      ) : (
        <CircleArrowUp className="size-3.5 text-success-text" aria-hidden />
      )}
    </button>
  )
}
