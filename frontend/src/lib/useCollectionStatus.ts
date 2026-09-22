import { useEffect, useState } from "react"
import { api, isTauri, listenEvent, type CollectionStatusDto } from "./api"

export function useCollectionStatus() {
  const [status, setStatus] = useState<CollectionStatusDto | null>(null)
  useEffect(() => {
    if (!isTauri) return
    let stopped = false
    let off: (() => void) | undefined
    void listenEvent<CollectionStatusDto>("collection-status", (next) => {
      if (!stopped) setStatus(next)
    }).then((unlisten) => {
      if (stopped) unlisten()
      else {
        off = unlisten
        void api.collectionStatus().then((current) => { if (!stopped) setStatus(current) }).catch(() => {})
      }
    })
    return () => { stopped = true; off?.() }
  }, [])
  return {
    status,
    retry: async () => { await api.scanLocalSessions() },
  }
}
