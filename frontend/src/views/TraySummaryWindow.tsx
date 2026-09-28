import { useEffect, useLayoutEffect, useRef, useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { TrayTooltip } from "@/components/tray/TrayMenu"
import { isTauri, listenEvent } from "@/lib/api"

interface Summary {
  connection: string
  quotas: { key: string; value: string }[]
  updated_at: number | null
  message: string
}

/**
 * 托盘悬停摘要。离开图标后窗口由后端隐藏保温、稍后才销毁；
 * 再次悬停时收到 tray-summary-open，重读摘要并按新内容定位显示，隐藏期间不轮询。
 */
export function TraySummaryWindow() {
  const root = useRef<HTMLDivElement>(null)
  const [summary, setSummary] = useState<Summary>({ connection: "未选择连接", quotas: [], updated_at: null, message: "尚无额度数据" })
  /** 每次打开递增：内容提交到 DOM 后再按实际尺寸定位并显示 */
  const [opened, setOpened] = useState(0)
  const fit = () => {
    const box = root.current?.getBoundingClientRect()
    if (!box) return
    void invoke("tray_summary_fit", { width: box.width, height: box.height }).catch(() => {})
  }
  useEffect(() => {
    if (!isTauri) return
    let active = true
    let timer: number | null = null
    const load = () => invoke<Summary>("tray_summary_get").then((value) => { if (active) setSummary(value) }).catch(() => {
      if (active) setSummary({ connection: "摘要暂不可用", quotas: [], updated_at: null, message: "请在主面板查看连接状态" })
    })
    const stop = () => {
      if (timer !== null) window.clearInterval(timer)
      timer = null
    }
    const open = () => {
      stop()
      void load().then(() => { if (active) setOpened((value) => value + 1) })
      timer = window.setInterval(load, 5000)
    }
    open()
    const offs: Array<() => void> = []
    const listen = (promise: Promise<() => void>) => void promise.then((off) => { if (active) offs.push(off); else off() }).catch(() => {})
    listen(listenEvent("tray-summary-open", open))
    listen(listenEvent("tray-summary-close", stop))
    return () => { active = false; stop(); offs.forEach((off) => off()) }
  }, [])
  // 保温窗口重新打开时内容尺寸可能不变，ResizeObserver 不会回调，这里主动定位并显示一次。
  useLayoutEffect(() => { if (opened > 0) fit() }, [opened])
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
