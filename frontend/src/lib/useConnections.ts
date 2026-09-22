import { useCallback, useEffect, useState } from "react"
import { api, isTauri, listenEvent, type Candidate, type ConnectionDto } from "./api"
import type { Connection, ConnectionKind, PlatformId } from "@/types"

/**
 * 连接管理（§6.3）
 *
 * 连接只有官方订阅（auth）与 API Key（api）两种；本地会话记录是统计来源，
 * 不在此列。凭证原值永不进入前端——后端只给 masked。
 */

/** 「更新」按钮只剩忙碌态；结果文案交给主面板的 Toast 播报，按钮不再自己闪一下再复位 */
export type FetchState = "idle" | "loading"

/** 一次「更新」的结局，调用方据此决定弹什么 */
export type FetchOutcome =
  | { kind: "updated" }
  | { kind: "same" }
  | { kind: "notfound"; message: string }
  | { kind: "mismatch"; message: string }
  | { kind: "error"; message: string }

function toConnection(d: ConnectionDto): Connection {
  return {
    id: d.id,
    platformId: d.platform as PlatformId,
    kind: d.kind as ConnectionKind,
    // API 连接把脱敏标识并进名称，便于区分同平台多个 Key
    name: d.kind === "api" && d.masked ? `${d.name} · ${d.masked}` : d.name,
    baseName: d.name,
    label: d.kind === "api" ? "API Key" : d.label,
    masked: d.masked,
    status: d.status,
    lastSyncText: d.last_sync_text ?? "从未同步",
    baseUrl: d.base_url,
    model: d.model,
    effort: d.effort,
    context1m: d.context_1m,
  }
}

export function useConnections() {
  const [connections, setConnections] = useState<Connection[]>([])
  const [candidates, setCandidates] = useState<Candidate[]>([])
  const [loading, setLoading] = useState(isTauri)
  const [candidateLoading, setCandidateLoading] = useState(false)
  const [live, setLive] = useState(false)
  const [error, setError] = useState<string | null>(null)
  /** 每行「获取」的独立状态，按连接 id 索引 */
  const [fetchState, setFetchState] = useState<Record<string, FetchState>>({})
  /** 最近一次「获取」扫描过的位置，界面据此说明读取范围 */
  const [scanned, setScanned] = useState<string[]>([])

  const reload = useCallback(async () => {
    if (!isTauri) return
    setLoading(true)
    try {
      const list = await api.listConnections()
      setConnections(list.map(toConnection))
      setLive(true)
      setError(null)
    } catch (reason) {
      setLive(false)
      setError(String(reason))
    } finally {
      setLoading(false)
    }
  }, [])

  const refreshCandidates = useCallback(async () => {
    if (!isTauri) return
    setCandidateLoading(true)
    try {
      setCandidates(await api.discoverConnections())
    } finally {
      setCandidateLoading(false)
    }
  }, [])

  useEffect(() => {
    void reload()
    let disposed = false
    let off: (() => void) | undefined
    void listenEvent("connections-changed", () => void reload()).then((un) => {
      if (disposed) un()
      else off = un
    })
    return () => {
      disposed = true
      off?.()
    }
  }, [reload, refreshCandidates])

  /** 行内「更新」：重读本机配置刷新已有连接的 Key 与地址。失败不破坏现有连接 */
  const fetchCreds = useCallback(
    async (id: string): Promise<FetchOutcome> => {
      setFetchState((s) => ({ ...s, [id]: "loading" }))
      try {
        const r = await api.fetchCredentials(id)
        setScanned(r.scanned)
        if (r.ok) {
          await reload()
          return r.changed ? { kind: "updated" } : { kind: "same" }
        }
        // 后端把「本机配置指向别的服务」和「压根没找到」都塞在 message 里，按关键词分流
        return r.message.includes("不同")
          ? { kind: "mismatch", message: r.message }
          : { kind: "notfound", message: r.message }
      } catch (reason) {
        return { kind: "error", message: String(reason) }
      } finally {
        setFetchState((s) => ({ ...s, [id]: "idle" }))
      }
    },
    [reload],
  )

  const add = useCallback(
    async (input: {
      platform: string
      kind: "auth" | "api"
      name: string
      secret?: string | null
      base_url?: string | null
      /** 手动添加的可选默认参数（画布 18） */
      model?: string | null
      effort?: string | null
      context_1m?: boolean | null
    }) => {
      await api.addConnection(input)
      await reload()
    },
    [reload],
  )

  /** 移除只断开连接，历史统计保留 */
  const remove = useCallback(
    async (id: string) => {
      await api.removeConnection(id)
      await reload()
    },
    [reload],
  )

  const replaceApiKey = useCallback(async (id: string, secret: string) => {
    await api.replaceApiKey(id, secret)
    await reload()
  }, [reload])

  /** 改名只动显示名称；后端会同步刷新灵动岛的名称快照 */
  const rename = useCallback(async (id: string, name: string) => {
    await api.renameConnection(id, name)
    await reload()
  }, [reload])

  const setPaused = useCallback(async (id: string, paused: boolean) => {
    await api.setConnectionPaused(id, paused)
    await reload()
  }, [reload])

  const readLocal = useCallback(async (platform: string, kind: "auth" | "api", name?: string) => {
    const summary = await api.readLocalConnections(platform, kind, name)
    await reload()
    return summary
  }, [reload])

  return { readLocal, setPaused, connections, candidates, loading, candidateLoading, live, error, reload, refreshCandidates, fetchState, scanned, fetchCreds, add, remove, replaceApiKey, rename }
}
