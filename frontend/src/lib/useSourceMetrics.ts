import { useEffect, useState } from "react"
import { api, isTauri, type CustomRange, type SourceMetrics } from "./api"

export function useSourceMetrics(platform: string, period: string, custom: CustomRange | null, model: string | null, queryEndMs: number) {
  const [result, setResult] = useState<{ key: string; data: SourceMetrics | null; error: string | null } | null>(null)
  const key = JSON.stringify([platform, period, custom?.start, custom?.end, model, queryEndMs])
  useEffect(() => {
    if (!isTauri) return
    let cancelled = false
    void api.sourceMetrics(platform, period, custom, model, queryEndMs)
      .then(data => { if (!cancelled) setResult({ key, data, error: null }) })
      .catch(error => { if (!cancelled) setResult({ key, data: null, error: String(error) }) })
    return () => { cancelled = true }
  }, [key])
  return result?.key === key ? result : { data: null, error: null }
}
