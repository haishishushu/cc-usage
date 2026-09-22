import { useCallback, useEffect, useRef, useState } from "react"
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
  proxy_enabled: false,
  proxy_port: 12731,
  proxy_fallback_direct: true,
  proxy_claude_upstream: null,
  proxy_codex_upstream: null,
}

export function useSettings() {
  const [settings, setSettings] = useState<AppSettings>(DEFAULTS)
  const [loading, setLoading] = useState(isTauri)
  const [ready, setReady] = useState(!isTauri)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const busy = useRef(false)
  const revision = useRef(0)

  const reload = useCallback(async () => {
    if (!isTauri) return
    const started = revision.current
    setLoading(true)
    try {
      const next = await api.getSettings()
      if (revision.current === started) { setSettings(next); setError(null); setReady(true) }
    } catch (reason) { setError(`设置读取失败：${String(reason)}`) }
    finally { setLoading(false) }
  }, [])

  useEffect(() => {
    if (!isTauri) return
    let stopped = false
    let off: (() => void) | undefined
    // 先订阅再读初值；读取期间收到的新快照不能被较旧的响应覆盖。
    void listenEvent<AppSettings>("settings-changed", (next) => {
      if (!stopped) { revision.current++; setSettings(next); setReady(true) }
    }).then(async (unsubscribe) => {
      if (stopped) { unsubscribe(); return }
      off = unsubscribe
      await reload()
    }).catch((reason) => {
      if (!stopped) { setError(`设置同步失败：${String(reason)}`); setLoading(false) }
    })
    return () => { stopped = true; off?.() }
  }, [reload])

  const save = useCallback(async (action: () => Promise<AppSettings>) => {
    if (busy.current) return false
    busy.current = true
    setSaving(true)
    setError(null)
    const started = revision.current
    try {
      const next = await action()
      if (revision.current === started) setSettings(next)
      return true
    } catch (reason) {
      setError(`设置保存失败：${String(reason)}`)
      return false
    } finally { busy.current = false; setSaving(false) }
  }, [])

  const setIslandPlatform = useCallback((platform: string) => save(() => api.setIslandPlatform(platform)), [save])
  const setIslandConnection = useCallback((id: string | null) => save(() => api.setIslandConnection(id)), [save])
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

  return { settings, ready, loading, saving, error, reload, setIslandPlatform, setIslandConnection, setIslandSource, setDnd, setDockEnabled, setTopmost, setSilentStartup, setProxy, setTheme, setRetentionDays, setDisplayPreferences }
}

