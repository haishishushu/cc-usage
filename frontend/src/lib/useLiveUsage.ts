import { useEffect, useState } from "react"
import { api, compactTokens, isTauri, listenLiveUsage, type LiveUsageDto } from "./api"
import { useRef } from "react"
import { LiveTokenCounter } from "./liveTokenCounter"
import { LiveCounts } from "./liveCounts"
import type { DeltaPhase } from "@/components/island/TokenDelta"
import type { SessionActivity } from "@/types"

export interface LiveUsage {
  /**
   * 已经插值的逐帧显示值（`total` 与 `session:<id>`），所有布局共用、不在展示组件中再次补间。
   * 放在 React 状态之外：通过 LiveCountsContext 交给显示数字的叶子组件订阅。
   */
  counts: LiveCounts
  /** 追数合计当前是否大于 0（只在跨越 0 时变化，不随每帧变化） */
  deltaPositive: boolean
  deltaPhase: DeltaPhase
  sessions: SessionActivity[]
  activityCountText: string | null
  activityKind: "running" | "recent"
  todayTokenText: string | null
  activeWindowSeconds: number
  pulseSerial: number
  live: boolean
}

export function useConnectionKind(platform: string) {
  const [info, setInfo] = useState<{ kind: "auth" | "api" | null; label: string | null }>({
    kind: null, label: null,
  })
  useEffect(() => {
    if (!isTauri) return
    let stopped = false
    void api.connectionKind(platform).then((r) => !stopped && setInfo(r)).catch(() => {})
    return () => { stopped = true }
  }, [platform])
  return info
}

/** 日志事件驱动真实目标值；合计与各会话在同一个帧循环中追数。 */
export function useLiveUsage(platform: string, enabled = true, dnd = false, paused = false): LiveUsage {
  const [counts] = useState(() => new LiveCounts())
  const [deltaPositive, setDeltaPositive] = useState(false)
  const [phase, setPhase] = useState<DeltaPhase>("idle")
  const [sessions, setSessions] = useState<SessionActivity[]>([])
  const [today, setToday] = useState<number | null>(null)
  const [windowSec, setWindowSec] = useState(90)
  const [live, setLive] = useState(false)
  const [pulseSerial, setPulseSerial] = useState(0)
  const pausedRef = useRef(paused)
  const resumeRef = useRef<() => void>(() => {})

  useEffect(() => {
    pausedRef.current = paused
    if (!paused) resumeRef.current()
  }, [paused])

  useEffect(() => {
    counts.set({})
    setDeltaPositive(false)
    setPhase("idle")
    setSessions([])
    setToday(null)
    setLive(false)
    setPulseSerial(0)
    if (!isTauri || !enabled) return
    let stopped = false
    let cursor: number | null = null
    let frame: number | null = null
    let retirement: number | null = null
    let entry: number | null = null
    let highlightTimer: number | null = null
    let showing = false
    let hadSessions = false
    const counter = new LiveTokenCounter()
    const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)")

    const tick = (now: number) => {
      frame = null
      if (stopped) return
      if (pausedRef.current) return
      const snapshot = counter.sample(now, reducedMotion.matches)
      counts.set(snapshot.values)
      // 只有跨越 0 时才更新 React 状态；同值 setState 不会触发重渲染。
      setDeltaPositive((snapshot.values.total ?? 0) > 0)
      if (!snapshot.settled) frame = requestAnimationFrame(tick)
    }
    const animate = () => {
      // 不取消正在运行的帧；连续事件不会推迟显示。
      if (!pausedRef.current && frame === null) frame = requestAnimationFrame(tick)
    }
    resumeRef.current = animate
    const cancelRetirement = () => {
      if (retirement !== null) window.clearTimeout(retirement)
      retirement = null
    }
    const retire = () => {
      if (!showing) return
      cancelRetirement()
      if (entry !== null) window.clearTimeout(entry)
      entry = null
      setPhase("hold")
      retirement = window.setTimeout(() => {
        setPhase("leave")
        retirement = window.setTimeout(() => {
          retirement = null
          if (frame !== null) cancelAnimationFrame(frame)
          frame = null
          counter.clear()
          counts.set({})
          setDeltaPositive(false)
          setPhase("idle")
          showing = false
        }, reducedMotion.matches ? 0 : 300)
      }, 2000)
    }

    const apply = (u: LiveUsageDto) => {
      if (stopped || u.platform !== platform || (cursor !== null && u.cursor < cursor)) return
      const advanced = cursor !== null && u.cursor > cursor
      cursor = u.cursor
      setLive(true)
      setToday(u.today_tokens)
      setWindowSec(u.active_window_seconds)
      const hasRunningSnapshot = u.running_tokens != null
      if (!dnd && hasRunningSnapshot) {
        counter.setTarget("total", u.running_tokens!, performance.now(), !showing)
        showing = true
        setPhase("hold")
        animate()
      }
      // 首次事件仅建立基线；相同游标的心跳不会重复累加。
      if (!dnd && advanced && !u.initial && u.realtime_delta && u.delta_tokens > 0) {
        setPulseSerial((value) => value + 1)
        const now = performance.now()
        if (!hasRunningSnapshot) counter.add("total", u.delta_tokens, now)
        for (const s of u.delta_sessions) {
          if (!u.active_sessions.some((active) => active.session_id === s.session_id && active.running_tokens != null)) {
            counter.add(`session:${s.session_id}`, s.delta_tokens, now)
          }
        }
        if (!showing) {
          showing = true
          setPhase(reducedMotion.matches ? "hold" : "enter")
          if (!reducedMotion.matches) entry = window.setTimeout(() => {
            entry = null
            setPhase("hold")
          }, 20)
        }
        animate()
      }
      // 失败会话由后端保证只在 90 秒窗口内出现，这里放行展示「失败」标签。
      const visibleSessions = u.active_sessions.filter((session) =>
        (platform === "codex" || platform === "claude")
          ? session.state === "running" || session.state === "failed"
          : session.state === "recent",
      )
      if (!dnd) {
        for (const session of visibleSessions) {
          if (session.running_tokens != null) {
            counter.setTarget(`session:${session.session_id}`, session.running_tokens, performance.now())
          }
        }
        animate()
      }
      const updatedIds = new Set(advanced ? u.delta_sessions.map((session) => session.session_id) : [])
      if (visibleSessions.length > 0) {
        if (retirement !== null) {
          cancelRetirement()
          setPhase("hold")
        }
        hadSessions = true
      } else if ((hadSessions || showing) && retirement === null) {
        hadSessions = false
        retire()
      }
      // 会话顺序直接采用后端的最近活跃降序——最新的会话在最前面（2026-09-19 鼠鼠定版）。
      // 顺序变化由 SessionList 的 FLIP 过渡平滑呈现，不再为防跳动冻结既有次序。
      setSessions(visibleSessions.map((session) => ({
        id: session.session_id,
        title: session.title,
        deltaText: "—",
        countKey: `session:${session.session_id}`,
        state: session.state !== "recent" ? session.state : "unknown",
        updatedAtMs: session.last_seen_ms,
        startedAtMs: session.started_at_ms ?? undefined,
        highlighted: updatedIds.has(session.session_id),
      } as SessionActivity)))
      if (updatedIds.size > 0) {
        if (highlightTimer !== null) window.clearTimeout(highlightTimer)
        highlightTimer = window.setTimeout(() => {
          highlightTimer = null
          setSessions((current) => current.map((session) => ({ ...session, highlighted: false })))
        }, reducedMotion.matches ? 0 : 800)
      }
    }

    let unlisten: (() => void) | null = null
    void listenLiveUsage(apply).then((un) => {
      if (stopped) un()
      else {
        unlisten = un
        // 先完成订阅，再拉一次所属平台的当前快照，避免后端首次推送早于订阅。
        void api.liveUsage(platform, null).then(apply).catch(() => {
          if (!stopped) setLive(false)
        })
      }
    }).catch(() => { if (!stopped) setLive(false) })
    reducedMotion.addEventListener("change", animate)
    return () => {
      stopped = true
      unlisten?.()
      if (frame !== null) cancelAnimationFrame(frame)
      if (entry !== null) window.clearTimeout(entry)
      if (highlightTimer !== null) window.clearTimeout(highlightTimer)
      cancelRetirement()
      reducedMotion.removeEventListener("change", animate)
      resumeRef.current = () => {}
    }
  }, [platform, enabled, dnd, counts])

  return {
    counts,
    deltaPositive,
    deltaPhase: phase,
    // 会话数组只在事件到达时换新引用，逐帧数值由各行按 countKey 自行订阅。
    sessions,
    // 「运行中」只数 running 会话；失败会话单独提示，不混入运行数。
    activityCountText: (() => {
      if (sessions.length === 0) return null
      if (platform !== "codex" && platform !== "claude") return `${sessions.length} 个最近活跃会话`
      const running = sessions.filter((session) => session.state === "running").length
      const failed = sessions.filter((session) => session.state === "failed").length
      if (running === 0 && failed === 0) return null
      if (running === 0) return `${failed} 个会话失败`
      return failed > 0 ? `${running} 个会话运行中 · ${failed} 个失败` : `${running} 个会话运行中`
    })(),
    activityKind: (platform === "codex" || platform === "claude") ? "running" : "recent",
    todayTokenText: today === null ? null : compactTokens(today),
    activeWindowSeconds: windowSec,
    pulseSerial,
    live,
  }
}

/**
 * 只取「本机今日 Token」：不跑追数动画、不维护会话列表（连接预览卡片用）。
 * `enabled` 为 false（面板隐藏）时停止订阅并保留已有值；切换平台才清空。
 */
export function useLiveToday(platform: string, enabled = true): string | null {
  const [today, setToday] = useState<{ platform: string; tokens: number | null } | null>(null)
  useEffect(() => {
    if (!isTauri || !enabled) return
    let stopped = false
    let cursor: number | null = null
    let unlisten: (() => void) | null = null
    const apply = (usage: LiveUsageDto) => {
      if (stopped || usage.platform !== platform || (cursor !== null && usage.cursor < cursor)) return
      cursor = usage.cursor
      setToday({ platform, tokens: usage.today_tokens })
    }
    void listenLiveUsage(apply).then((un) => {
      if (stopped) { un(); return }
      unlisten = un
      void api.liveUsage(platform, null).then(apply).catch(() => {})
    }).catch(() => {})
    return () => { stopped = true; unlisten?.() }
  }, [platform, enabled])
  return today?.platform === platform && today.tokens !== null ? compactTokens(today.tokens) : null
}
