import { useEffect, useState } from "react"
import { api, isTauri, type CustomRange, type SourceMetrics } from "./api"

/** `enabled` 为 false 时不查询（该平台不展示积分，或所在卡片当前不可见）。 */
export function useSourceMetrics(platform: string, period: string, custom: CustomRange | null, model: string | null, queryEndMs: number, enabled = true) {
  const [result, setResult] = useState<{ key: string; data: SourceMetrics | null; error: string | null } | null>(null)
  const key = JSON.stringify([platform, period, custom?.start, custom?.end, model, queryEndMs])
  useEffect(() => {
    if (!isTauri || !enabled) return
    let cancelled = false
    void api.sourceMetrics(platform, period, custom, model, queryEndMs)
      .then(data => { if (!cancelled) setResult({ key, data, error: null }) })
      .catch(error => { if (!cancelled) setResult({ key, data: null, error: String(error) }) })
    return () => { cancelled = true }
  }, [key, enabled])
  return result?.key === key ? result : { data: null, error: null }
}
