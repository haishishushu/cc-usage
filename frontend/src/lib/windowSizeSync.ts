export type WindowSize = { width: number; height: number }

/** 一次只提交一个尺寸；等待期间发生的变化只保留最新值。 */
export function createWindowSizeSync(apply: (size: WindowSize) => Promise<void>) {
  let pending: { size: WindowSize; force: boolean } | undefined
  let running = false
  let last: WindowSize | undefined
  async function flush() {
    if (running) return
    running = true
    try {
      while (pending) {
        const { size, force } = pending
        pending = undefined
        if (!force && last?.width === size.width && last.height === size.height) continue
        try {
          await apply(size)
          last = size
        } catch (error) {
          last = undefined
          console.error("同步灵动岛尺寸失败", error)
        }
      }
    } finally {
      running = false
    }
  }
  return {
    request(size: WindowSize, force = false) {
      if (!Number.isFinite(size.width) || !Number.isFinite(size.height)
        || size.width < 8 || size.height < 8) return
      pending = { size, force }
      void flush()
    },
    cancelPending() { pending = undefined },
  }
}
