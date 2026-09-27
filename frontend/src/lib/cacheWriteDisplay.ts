/** Codex 订阅的会话记录可能把未报告的缓存创建量固定写成 0。 */
export function cacheWriteDisplayValue(
  platform: string,
  kind: "auth" | "api",
  value: number | null,
  hasUsage: boolean,
): number | null {
  return platform === "codex" && kind === "auth" && hasUsage && value === 0 ? null : value
}
