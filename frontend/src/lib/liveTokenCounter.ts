/** 只追赶已采集的真实增量；重新定向时从当前值续计，不回退或超出目标。 */
export class LiveTokenCounter {
  private entries = new Map<string, { from: number; target: number; start: number }>()
  readonly duration = 900

  private value(entry: { from: number; target: number; start: number }, now: number) {
    const progress = Math.max(0, Math.min(1, (now - entry.start) / this.duration))
    return entry.from + (entry.target - entry.from) * (1 - (1 - progress) ** 2)
  }

  add(key: string, delta: number, now: number) {
    if (!Number.isFinite(delta) || delta <= 0) return
    const previous = this.entries.get(key)
    this.entries.set(key, {
      from: previous ? this.value(previous, now) : 0,
      target: (previous?.target ?? 0) + delta,
      start: now,
    })
  }

  /** 本轮累计快照：重连可恢复，相同快照不重复计数，新一轮允许归零。 */
  setTarget(key: string, target: number, now: number, immediate = false) {
    if (!Number.isFinite(target) || target < 0) return
    const previous = this.entries.get(key)
    if (previous?.target === target && !immediate) return
    this.entries.set(key, {
      from: immediate || !previous || target < previous.target
        ? target : this.value(previous, now),
      target,
      start: now,
    })
  }

  sample(now: number, immediate = false) {
    const values: Record<string, number> = Object.create(null)
    let settled = true
    for (const [key, entry] of this.entries) {
      if (immediate) entry.from = entry.target
      const value = immediate ? entry.target : this.value(entry, now)
      values[key] = Math.round(value)
      if (value < entry.target) settled = false
    }
    return { values, settled }
  }

  clear() {
    this.entries.clear()
  }
}
