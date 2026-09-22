/**
 * 模型名去掉结尾的发布日期：`claude-sonnet-4-5-20250929` → `claude-sonnet-4-5`。
 *
 * 列宽有限，日期后缀挤掉的正是真正用于区分模型的部分；完整名仍由悬浮提示给出。
 * 只裁结尾恰好八位的数字段，名字中间的数字与版本号一律保留。
 */
export function shortModelName(model: string): string {
  return model.replace(/-\d{8}$/, "")
}
