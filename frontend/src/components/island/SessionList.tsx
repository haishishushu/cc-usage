import { useEffect, useLayoutEffect, useRef, useState } from "react"
import { sessionElapsed } from "@/lib/sessionElapsed"
import { cn } from "@/lib/utils"
import { MOTION_EASE, reducedMotion } from "@/lib/motion"
import { Chip } from "@/components/ui/primitives"
import type { SessionActivity } from "@/types"

/**
 * C/SessionActivityList —— §2.1.2
 * 保留原有三行高度，超过时只在列表内部滚动；提示占用原有「还有 N 个会话」的位置。
 * 标题过长时单行省略并以…结尾（CSS truncate），悬停查看全名。
 * 会话行依次展示标题、本轮已执行时间与该会话实时 Token；合计独立展示。
 * 会话按最近活跃降序、最新的在最前（2026-09-19 鼠鼠定版）；顺序变化用 FLIP
 * 平滑换位——先记旧位置，DOM 更新后从位移差播放过渡，不做瞬时跳动。
 */
const MAX_VISIBLE = 3

const DOT: Record<SessionActivity["state"], string> = {
  running: "bg-success",
  done: "bg-text-tertiary",
  failed: "bg-danger",
  unknown: "bg-warn",
}

function StateTag({ state, showUnknown }: { state: SessionActivity["state"]; showUnknown?: boolean }) {
  if (state === "done") return <Chip tone="neutral">已完成</Chip>
  if (state === "failed") return <Chip tone="danger">失败</Chip>
  // 「状态未知」只用于来源中断；本地会话记录本就没有运行状态，
  // 此时整个列表已标为「最近活跃」，不必每行再挂标签
  if (state === "unknown" && showUnknown) return <Chip tone="warn">状态未知</Chip>
  return null
}

export function SessionList({
  sessions,
  totalCount,
  showUnknownTag = false,
}: {
  sessions: SessionActivity[]
  /** 当前范围内的会话总数，用于计算「还有 N 个会话」 */
  totalCount?: number
  showUnknownTag?: boolean
}) {
  const [now, setNow] = useState(Date.now)
  const listRef = useRef<HTMLDivElement>(null)
  const [listHeight, setListHeight] = useState(74)
  const [atEnd, setAtEnd] = useState(false)
  const firstRows = JSON.stringify(sessions.slice(0, MAX_VISIBLE).map((session) => session.id))
  // 每行渲染后在列表内容坐标系里的位置。FLIP 的「First」在这里留存，
  // 会话数组变化后的布局效果里与「Last」求差播放过渡；用 offsetTop 而非
  // 视口坐标，列表内部滚动不会伪装成换位。
  const previousTops = useRef<Map<string, number> | null>(null)
  useLayoutEffect(() => {
    const list = listRef.current
    if (!list) return
    const currentTops = new Map<string, number>()
    const rows = Array.from(list.children) as HTMLElement[]
    for (const row of rows) {
      const id = row.dataset.sessionId
      if (id) currentTops.set(id, row.offsetTop)
    }
    const previous = previousTops.current
    previousTops.current = currentTops
    // 首次挂载没有旧位置，不播动画；系统要求减少动态效果时直接落位。
    if (!previous || reducedMotion()) return
    for (const row of rows) {
      const id = row.dataset.sessionId
      if (!id) continue
      const before = previous.get(id)
      const after = currentTops.get(id)
      if (before === undefined || after === undefined || before === after) continue
      row.animate(
        [{ transform: `translateY(${before - after}px)` }, { transform: "translateY(0)" }],
        { duration: 200, easing: MOTION_EASE },
      )
    }
  }, [sessions])
  useLayoutEffect(() => {
    const list = listRef.current
    if (!list) return
    const rows = Array.from(list.children).slice(0, MAX_VISIBLE) as HTMLElement[]
    const measure = () => {
      // 使用原有行高与间距，不把之前的展开高度改成新的固定尺寸。
      const gap = Number.parseFloat(getComputedStyle(list).rowGap) || 0
      setListHeight(rows.reduce((height, row) => height + row.offsetHeight, 0) + Math.max(0, rows.length - 1) * gap)
    }
    measure()
    const observer = new ResizeObserver(measure)
    rows.forEach((row) => observer.observe(row))
    return () => observer.disconnect()
  }, [firstRows])
  useLayoutEffect(() => {
    const list = listRef.current
    if (list) setAtEnd(list.scrollTop + list.clientHeight >= list.scrollHeight - 1)
  }, [sessions.length, listHeight])
  const hasRunningSessions = sessions.some((session) => session.state === "running")
  // 失败会话的计时冻结在失败时刻，不再随秒跳动
  const elapsed = (session: SessionActivity) =>
    sessionElapsed(session.startedAtMs, session.state === "failed" ? (session.updatedAtMs ?? now) : now)
  useEffect(() => {
    if (!hasRunningSessions) return
    setNow(Date.now())
    const timer = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(timer)
  }, [hasRunningSessions])
  const remaining = Math.max(0, sessions.length - MAX_VISIBLE)

  return (
    <div className="flex w-full flex-col gap-[7px]">
      <div ref={listRef} role="region" aria-label="会话列表，可滚动查看全部会话"
        className="relative flex w-full flex-col gap-[7px] overflow-y-auto overscroll-contain [scrollbar-width:thin]"
        style={{ maxHeight: listHeight }}
        onScroll={(event) => { const list = event.currentTarget; setAtEnd(list.scrollTop + list.clientHeight >= list.scrollHeight - 1) }}
        onPointerDown={(event) => event.stopPropagation()}
        onDoubleClick={(event) => event.stopPropagation()}>
      {sessions.map((s) => (
        <div
          key={s.id}
          data-session-id={s.id}
          tabIndex={0}
          aria-label={`${s.title}，本轮已执行 ${elapsed(s)}，${s.deltaText === "—" ? "Token 暂无数据" : s.deltaText}`}
          className={cn(
            "flex w-full shrink-0 items-center gap-2 rounded-[6px] px-1 py-0.5 transition-colors focus:outline-none focus:ring-1 focus:ring-inset focus:ring-accent-blue",
            s.highlighted && "bg-success-soft",
          )}
        >
          <span className={cn("size-1.5 shrink-0 rounded-full", DOT[s.state])} />
          <span className="min-w-0 flex-1 truncate text-xs text-text-primary" title={s.title}>
            {s.title}
          </span>
          <StateTag state={s.state} showUnknown={showUnknownTag} />
          <span className="tnum shrink-0 font-mono text-[10px] text-text-secondary" title="本轮任务已执行时间">
            {elapsed(s)}
          </span>
          <span className={cn("tnum shrink-0 font-mono text-[10px]", s.deltaText === "—" ? "text-text-tertiary" : "text-success-text")}
            title={s.startedAtMs ? "该会话本轮累计 Token" : "该会话实时采集的新增 Token"}>
            {s.deltaText === "—" ? "— Token" : s.deltaText}
          </span>
        </div>
      ))}
      </div>
      {remaining > 0 && (
        <p className="text-[11px] text-text-tertiary">{atEnd ? `已到列表底部 · 共 ${sessions.length} 个会话` : `向下滚动查看其余 ${remaining} 个会话 ↓`}</p>
      )}
      {(totalCount ?? sessions.length) > sessions.length && <p className="text-[11px] text-text-tertiary">另有 {totalCount! - sessions.length} 个会话尚未加载</p>}
    </div>
  )
}
