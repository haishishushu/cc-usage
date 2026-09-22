export type PageParseResult = { page: number; error: null } | { page: null; error: string }

export function parsePageInput(value: string, pageCount: number): PageParseResult {
  const trimmed = value.trim()
  if (!/^\d+$/.test(trimmed)) return { page: null, error: "请输入整数页码" }
  const page = Number(trimmed)
  if (!Number.isSafeInteger(page) || page < 1 || page > pageCount) {
    return { page: null, error: `页码范围为 1–${pageCount}` }
  }
  return { page, error: null }
}
