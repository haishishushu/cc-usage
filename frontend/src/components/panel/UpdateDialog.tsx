import { Check, LoaderCircle } from "lucide-react"
import { useEffect, useId, useRef, useState } from "react"
import { formatBytes, progressPercent } from "@/lib/updateState"
import { useUpdate } from "./UpdateContext"

/**
 * 更新进度对话框（画布 17 · B）。
 *
 * 点击标题栏绿色按钮即弹出：下载中显示实时进度且不可关闭，
 * 完成校验后进入「校验并安装」，就绪态（Tauri 下由后端自动重启）
 * 显示完成提示；预览模式提供「立即重启」模拟重启。
 */
export function UpdateDialog() {
  const { phase, info, progress, error, restartPreview } = useUpdate()
  const [closed, setClosed] = useState(false)
  const titleId = useId()
  const closeRef = useRef(() => {})
  closeRef.current = () => setClosed(true)

  // 重新触发安装（预览里重复演示）时恢复显示
  useEffect(() => {
    if (phase === "downloading") setClosed(false)
  }, [phase])

  // 就绪态允许 Escape 关闭；下载与安装中不可关闭（画布标注）
  useEffect(() => {
    if (phase !== "ready" || closed) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeRef.current()
    }
    document.addEventListener("keydown", onKeyDown)
    return () => document.removeEventListener("keydown", onKeyDown)
  }, [phase, closed])

  if (!info) return null
  if (phase !== "downloading" && phase !== "installing" && phase !== "ready") return null
  if (closed) return null

  const percent = progressPercent(progress.downloaded, progress.total)
  const installing = phase === "installing"
  const ready = phase === "ready"
  const bytes = progress.total === null
    ? formatBytes(progress.downloaded)
    : `${formatBytes(progress.downloaded)} / ${formatBytes(progress.total)}`

  return (
    <div role="dialog" aria-modal="true" aria-labelledby={titleId} className="motion-overlay absolute inset-0 z-20 grid place-items-center bg-black/25 p-8">
      <div className="motion-dialog flex w-[360px] max-w-full flex-col gap-3.5 rounded-xl border bg-surface p-5 shadow-dialog">
        <div className="flex items-center gap-3">
          <span className={`grid size-10 place-items-center rounded-full bg-success-soft ${installing && !ready ? "animate-pulse" : ""}`}>
            {ready ? (
              <Check className="size-5 text-success-text" aria-hidden />
            ) : (
              <LoaderCircle className={`size-5 text-success-text ${ready ? "" : "animate-spin"}`} aria-hidden />
            )}
          </span>
          <span className="flex flex-col gap-0.5">
            <span id={titleId} className="text-sm font-semibold text-text-primary">
              {ready ? "安装完成" : installing ? "校验并安装" : `正在下载 v${info.availableVersion}`}
            </span>
            <span className="text-[11px] text-text-secondary">
              {ready
                ? `v${info.availableVersion} 已就绪，应用即将自动重启`
                : installing
                  ? `已下载 ${bytes} · 正在校验签名并替换应用文件`
                  : "下载完成后自动安装，无需操作"}
            </span>
          </span>
        </div>

        <div className="tnum flex items-center gap-2 rounded-lg bg-surface-2 px-3 py-2.5 font-mono text-xs">
          <span className="text-text-tertiary">v{info.currentVersion}</span>
          <span aria-hidden className="text-text-tertiary">→</span>
          <span className="font-bold text-success-text">v{info.availableVersion}</span>
          <span className="flex-1" />
          {info.pubDate && <span className="font-sans text-[10px] text-text-tertiary">{info.pubDate.slice(0, 10)} 发布</span>}
        </div>

        {info.notes && (
          <div className="flex flex-col gap-1.5 rounded-lg bg-surface-2 p-3">
            <span className="text-[11px] font-semibold text-text-secondary">更新说明</span>
            <p className="text-[11px] leading-[1.5] text-text-secondary">{info.notes}</p>
          </div>
        )}

        {!ready && (
          <div className="flex flex-col gap-1.5">
            <div
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={percent ?? undefined}
              className="h-1.5 w-full overflow-hidden rounded-full bg-track"
            >
              <div
                className="h-full rounded-full bg-success transition-[width] duration-200"
                style={{ width: `${percent === null ? 40 : percent}%` }}
              />
            </div>
            <div className="tnum flex justify-between font-mono text-[11px]">
              <span className="text-text-secondary">{bytes}</span>
              <span className="font-bold text-success-text">{percent === null ? "…" : `${percent}%`}</span>
            </div>
          </div>
        )}

        {error && <p role="alert" className="text-[11px] text-danger">{error}</p>}

        <p className="text-center text-[11px] text-text-tertiary">
          {ready
            ? "灵动岛与后台采集会一并恢复"
            : installing
              ? "期间用量采集与灵动岛不受影响"
              : "下载期间不会中断用量采集；完成前对话框不可关闭"}
        </p>

        {ready && (
          <div className="flex justify-end gap-2.5">
            <button
              type="button"
              onClick={() => setClosed(true)}
              className="rounded-[8px] bg-neutral-soft px-3.5 py-2 text-xs font-medium text-text-secondary"
            >
              稍后自行重启
            </button>
            <button
              type="button"
              onClick={restartPreview}
              className="rounded-[8px] bg-success px-3.5 py-2 text-xs font-semibold text-white"
            >
              立即重启
            </button>
          </div>
        )}
      </div>
    </div>
  )
}
