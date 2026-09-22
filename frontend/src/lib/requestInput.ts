/**
 * 请求日志的「输入」口径归一。
 *
 * 各平台上报的 input_tokens 语义并不一致（见 backend/src/collector.rs）：
 * - Claude：input_tokens **不含**缓存，cache_read / cache_write 单列上报，
 *   实际喂进模型的输入要把三者相加；命中率高时 input_tokens 常年只有个位数。
 * - Codex：input_tokens **已含** cached 部分，直接采用，再加一次就重复计算了。
 *
 * 口径与 collector 计算 total_tokens 的方式保持一致，因此归一后满足
 * 「输入 + 输出 = 总计」。字段缺失时返回 null，由界面显示「—」，不补造数值。
 */
export type RequestInputTokens = {
  input_semantics?: "includes_cache" | "excludes_cache" | "unknown"
  platform: string
  input: number | null
  cache_read: number | null
  cache_write: number | null
}

export function totalInputTokens({ platform, input, cache_read, cache_write, input_semantics }: RequestInputTokens): number | null {
  if (input === null) return null
  const semantics = input_semantics ?? (platform === "claude" ? "excludes_cache" : platform === "codex" ? "includes_cache" : "unknown")
  if (semantics === "unknown") return null
  if (semantics === "includes_cache") return input
  if (cache_read === null || cache_write === null) return null
  return input + cache_read + cache_write
}
