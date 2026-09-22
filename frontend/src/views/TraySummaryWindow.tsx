import { useEffect, useRef, useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { TrayTooltip } from "@/components/tray/TrayMenu"
import { isTauri } from "@/lib/api"

interface Summary {
  connection: string
  quotas: { key: string; value: string }[]
  updated_at: number | null
  message: string
}

export function TraySummaryWindow() {
  const root = useRef<HTMLDivElement>(null)
  const [summary, setSummary] = useState<Summary>({ connection: "未选择连接", quotas: [], updated_at: null, message: "尚无额度数据" })
  useEffect(() => {
    if (!isTauri) return
    let active = true
    const load = () => invoke<Summary>("tray_summary_get").then((value) => { if (active) setSummary(value) }).catch(() => {
      if (active) setSummary({ connection: "摘要暂不可用", quotas: [], updated_at: null, message: "请在主面板查看连接状态" })
    })
    void load()
    const timer = window.setInterval(load, 5000)
    return () => { active = false; window.clearInterval(timer) }
  }, [])
  useEffect(() => {
    if (!isTauri || !root.current) return
    const observer = new ResizeObserver(([entry]) => {
      void invoke("tray_summary_fit", { width: entry.borderBoxSize[0].inlineSize, height: entry.borderBoxSize[0].blockSize }).catch(() => {})
    })
    observer.observe(root.current)
    return () => observer.disconnect()
  }, [])
  const age = summary.updated_at == null ? null : Math.max(0, Math.floor((Date.now() - summary.updated_at) / 60000))
  const updated = age == null ? summary.message : `最近更新 ${age === 0 ? "刚刚" : `${age} 分钟前`}${summary.message ? ` · ${summary.message}` : ""}`
  return <div ref={root} className="w-fit p-3"><TrayTooltip app="CC Usage" connection={summary.connection} quotas={summary.quotas} updated={updated} /></div>
}
