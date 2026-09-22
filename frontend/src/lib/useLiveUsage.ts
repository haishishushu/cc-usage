import { useEffect, useState } from "react"
import { api, compactTokens, isTauri, listenLiveUsage, type LiveUsageDto } from "./api"
import { useRef } from "react"
import { LiveTokenCounter } from "./liveTokenCounter"
import type { DeltaPhase } from "@/components/island/TokenDelta"
import type { SessionActivity } from "@/types"

export interface LiveUsage {
  deltaText: string | null
  /** 已经插值的显示值，所有布局共用，不在展示组件中再次补间。 */
  deltaTokens: number | null
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
  const [counts, setCounts] = useState<Record<string, number>>({})
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
    setCounts({})
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
      setCounts(snapshot.values)
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
          setCounts({})
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
  }, [platform, enabled, dnd])

  const deltaTokens = counts.total ?? 0
  return {
    deltaText: deltaTokens > 0 ? `+${compactTokens(deltaTokens)} Token` : null,
    deltaTokens: phase === "idle" ? null : deltaTokens,
    deltaPhase: phase,
    sessions: sessions.map((session) => {
      const tokens = counts[`session:${session.id}`]
      return { ...session, deltaText: tokens == null ? "—" : `+${compactTokens(tokens)} Token` }
    }),
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
