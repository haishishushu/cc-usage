import React, { useState } from "react"
import { createRoot } from "react-dom/client"
import "./src/index.css"
import { ConnectionPreviews } from "./src/components/settings/ConnectionPreviews"
import { ConnectionTable } from "./src/components/settings/ConnectionTable"
import type { Connection } from "./src/types"

function Fixture() {
  const [count, setCount] = useState(3)
  const [paused, setPaused] = useState(false)
  const [fail, setFail] = useState(false)
  const connections: Connection[] = Array.from({ length: count }, (_, i) => ({
    id: `preview-${i}`, platformId: i % 2 ? "codex" : "claude", kind: i % 2 ? "api" : "auth",
    name: `验收连接 ${i + 1}`, label: i % 2 ? "API Key" : "官方订阅",
    status: i === 0 && paused ? "paused" : "connected", lastSyncText: "刚刚", baseUrl: null,
  }))
  return <main className="mx-auto flex max-w-[1128px] flex-col gap-6 bg-bg p-8 text-text-primary">
    <h1>仅用于验收的合成连接，不读取本机凭证</h1>
    <div className="flex gap-4">{[0, 1, 2, 3, 5].map(n => <button key={n} onClick={() => setCount(n)}>{n} 个连接</button>)}</div>
    <label><input type="checkbox" checked={fail} onChange={e => setFail(e.target.checked)} />模拟操作失败</label>
    <ConnectionTable connections={connections} fetchState={{ 'preview-0': 'success' }} onPause={async (_id, value) => {
      if (fail) throw new Error("模拟失败，状态未改变")
      setPaused(value)
    }} />
    <ConnectionPreviews connections={connections} selectedId="preview-0" />
  </main>
}
createRoot(document.getElementById("root")!).render(<Fixture />)
