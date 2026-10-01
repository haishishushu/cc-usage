import { useCallback, useRef, useState, useSyncExternalStore } from "react"
import { api, isTauri, listenEvent, type AppSettings } from "./api"

/**
 * 持久化设置（§2.6 / §7.3）
 *
 * 灵动岛平台与免打扰在托盘菜单和设置界面里必须是**同一个值**，
 * 因此这里不各存一份：Rust 持有真值，前端读它、改它，并监听变更事件。
 */
const DEFAULTS: AppSettings = {
  silent_startup: true,
  island_platform: "claude",
  island_kind: "api",
  island_connection_id: null,
  island_connection_name: null,
  island_source_id: "local:claude",
  always_on_top: true,
  theme: "light",
  island_opacity: 100,
  island_scale: 100,
  island_shrink_scale: 100,
  refresh_minutes: 5,
  balance_alert_threshold: null,
  balance_alert_currency: "USD",
  dock_enabled: true,
  retention_days: null,
  dock: { edge: null, offset: 0, monitor: null },
  dnd: false,
  island_visible: true,
  island_clones: [],
  proxy_enabled: false,
  proxy_port: 12731,
  proxy_fallback_direct: true,
  proxy_claude_upstream: null,
  proxy_codex_upstream: null,
}

interface SharedSettings {
  settings: AppSettings
  ready: boolean
  loading: boolean
  error: string | null
}

/**
 * 每个窗口只保留一份设置快照：额度、余额、用量等 hook 都要读刷新间隔，
 * 各自读取和监听会让一个灵动岛窗口同时挂着六七份相同的订阅与初始请求。
 */
let shared: SharedSettings = { settings: DEFAULTS, ready: !isTauri, loading: isTauri, error: null }
/** 每收到一次推送递增；读取或保存期间若有更新的推送，较旧的响应不能覆盖它。 */
let revision = 0
let subscribed = false
const listeners = new Set<() => void>()

function publish(patch: Partial<SharedSettings>) {
  shared = { ...shared, ...patch }
  listeners.forEach((listener) => listener())
}

async function reloadShared() {
  if (!isTauri) return
  const started = revision
  publish({ loading: true })
  try {
    const next = await api.getSettings()
    if (revision === started) publish({ settings: next, error: null, ready: true })
  } catch (reason) { publish({ error: `设置读取失败：${String(reason)}` }) }
  finally { publish({ loading: false }) }
}

function ensureSubscribed() {
  if (subscribed || !isTauri) return
  subscribed = true
  // 先订阅再读初值；读取期间收到的新快照不能被较旧的响应覆盖。
  void listenEvent<AppSettings>("settings-changed", (next) => {
    revision++
    publish({ settings: next, ready: true })
  }).then(() => reloadShared()).catch((reason) => {
    subscribed = false
    publish({ error: `设置同步失败：${String(reason)}`, loading: false })
  })
}

function subscribe(listener: () => void) {
  ensureSubscribed()
  listeners.add(listener)
  return () => { listeners.delete(listener) }
}

const snapshot = () => shared

export function useSettings() {
  const state = useSyncExternalStore(subscribe, snapshot)
  const [saving, setSaving] = useState(false)
  const [saveError, setSaveError] = useState<string | null>(null)
  const busy = useRef(false)

  const save = useCallback(async (action: () => Promise<AppSettings>) => {
    if (busy.current) return false
    busy.current = true
    setSaving(true)
    setSaveError(null)
    const started = revision
    try {
      const next = await action()
      if (revision === started) publish({ settings: next })
      return true
    } catch (reason) {
      setSaveError(`设置保存失败：${String(reason)}`)
      return false
    } finally { busy.current = false; setSaving(false) }
  }, [])

  const setIslandPlatform = useCallback((platform: string) => save(() => api.setIslandPlatform(platform)), [save])
  const setIslandConnection = useCallback((id: string | null, island: string | null = null) => save(() => api.setIslandConnection(id, island)), [save])
  const setIslandSource = useCallback((id: string | null) => save(() => api.setIslandSource(id)), [save])
  const setDnd = useCallback((on: boolean) => save(() => api.setDnd(on)), [save])
  const setDockEnabled = useCallback((on: boolean) => save(() => api.setDockEnabled(on)), [save])
  const setTopmost = useCallback((on: boolean) => save(() => api.islandTopmost(on)), [save])
  const setSilentStartup = useCallback((on: boolean) => save(() => api.setSilentStartup(on)), [save])
  const setProxy = useCallback(
    (port: number, enabled: boolean, fallbackDirect: boolean) => save(() => api.setProxy(port, enabled, fallbackDirect)),
    [save],
  )
  const setTheme = useCallback((theme: "light" | "dark" | "system") => save(() => api.setTheme(theme)), [save])
  const setRetentionDays = useCallback((days: number | null) => save(() => api.setRetentionDays(days)), [save])
  const setDisplayPreferences = useCallback((patch: Partial<Pick<AppSettings, "island_opacity" | "island_scale" | "island_shrink_scale" | "refresh_minutes">>) => save(async () => {
    const next = { ...await api.getSettings(), ...patch }
    return api.setDisplayPreferences(next.island_opacity, next.island_scale, next.island_shrink_scale, next.refresh_minutes)
  }), [save])

  const { settings, ready, loading } = state
  const error = saveError ?? state.error
  return { settings, ready, loading, saving, error, reload: reloadShared, setIslandPlatform, setIslandConnection, setIslandSource, setDnd, setDockEnabled, setTopmost, setSilentStartup, setProxy, setTheme, setRetentionDays, setDisplayPreferences }
}

