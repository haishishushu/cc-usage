import { ChevronLeft, ChevronRight } from "lucide-react"
import { useState } from "react"
import { cn } from "@/lib/utils"
import { parsePageInput } from "@/lib/paginationState"

/**
 * Pagination —— §2.4 分页条
 *
 * 左侧总条数「共 161 条记录」；右侧 上一页箭头 → 页码按钮组 → 下一页箭头 → 页码输入 → 跳转。
 * 默认每页 10 条，该文案**不在界面上显示**，也不提供页大小切换控件。
 * 页码按钮 32×32 r8 间距 6；页码输入 76×32；组间距 16。
 */

/**
 * 页码省略规则（§2.4）：最多占 7 个位置，超出用不可点击的「…」省略。
 *  - 总页数 <= 7：全部列出
 *  - 当前页靠前（c <= 4）：1 2 3 4 5 … N
 *  - 当前页靠后（c >= N-3）：1 … N-4 N-3 N-2 N-1 N
 *  - 当前页居中：1 … c-1 c c+1 … N
 */
export function pageItems(current: number, total: number): (number | "ellipsis")[] {
  if (total <= 7) return Array.from({ length: total }, (_, i) => i + 1)
  if (current <= 4) return [1, 2, 3, 4, 5, "ellipsis", total]
  if (current >= total - 3)
    return [1, "ellipsis", total - 4, total - 3, total - 2, total - 1, total]
  return [1, "ellipsis", current - 1, current, current + 1, "ellipsis", total]
}

const BOX = "motion-button grid size-8 shrink-0 place-items-center rounded-[8px] border bg-surface hover:bg-hover"

export function Pagination({
  current,
  pageCount,
  totalCount,
  onChange,
  hasNext,
}: {
  current: number
  pageCount: number
  /** 来源无法提供可靠计数时传 null —— 降级为上一页 / 下一页，不显示页码按钮 */
  totalCount: number | null
  onChange?: (page: number) => void
  /** 总数未知时由来源明确是否还有下一页；未知则保持可点。 */
  hasNext?: boolean
}) {
  const [jump, setJump] = useState("")
  const [error, setError] = useState<string | null>(null)
  if (totalCount === null) {
    return (
      <div className="flex w-full items-center justify-between">
        <span className="text-xs text-text-tertiary">共 — 条记录（该来源不提供总条数）</span>
        <div className="flex items-center gap-2">
          <button
            type="button"
            disabled={current <= 1}
            onClick={() => onChange?.(current - 1)}
            className="h-8 rounded-[8px] border bg-surface px-3.5 text-xs text-text-primary disabled:opacity-40"
          >
            上一页
          </button>
          <button
            type="button"
            disabled={hasNext === false}
            onClick={() => onChange?.(current + 1)}
            className="h-8 rounded-[8px] border bg-surface px-3.5 text-xs text-text-primary disabled:opacity-40"
          >
            下一页
          </button>
        </div>
      </div>
    )
  }

  return (
    <div className="flex w-full items-center justify-between">
      <span className="tnum text-xs text-text-secondary">共 {totalCount} 条记录</span>

      <div className="flex items-center gap-4">
        <div className="flex items-center gap-1.5">
          <button
            type="button"
            disabled={current === 1}
            onClick={() => onChange?.(current - 1)}
            className={cn(BOX, "disabled:opacity-40")}
            aria-label="上一页"
          >
            <ChevronLeft className="size-3.5 text-text-secondary" />
          </button>

          {pageItems(current, pageCount).map((it, i) =>
            it === "ellipsis" ? (
              /* 「…」不可点击 */
              <span
                key={`e${i}`}
                className="grid size-8 shrink-0 place-items-center text-[13px] text-text-tertiary"
              >
                …
              </span>
            ) : (
              <button
                key={it}
                type="button"
                onClick={() => onChange?.(it)}
                className={cn(
                  BOX,
                  "tnum font-mono text-xs",
                  it === current
                    ? "border-accent-blue bg-accent-blue font-semibold text-white"
                    : "text-text-primary",
                )}
              >
                {it}
              </button>
            ),
          )}

          <button
            type="button"
            disabled={current === pageCount}
            onClick={() => onChange?.(current + 1)}
            className={cn(BOX, "disabled:opacity-40")}
            aria-label="下一页"
          >
            <ChevronRight className="size-3.5 text-text-secondary" />
          </button>
        </div>

        <div className="flex items-center gap-2">
          {/* 占位文案「页码」，不预填当前页 */}
          <input
            type="text"
            inputMode="numeric"
            value={jump}
            onChange={(event) => { setJump(event.target.value); setError(null) }}
            onKeyDown={(event) => {
              if (event.key !== "Enter") return
              const parsed = parsePageInput(jump, pageCount)
              setError(parsed.error)
              if (parsed.page !== null) { onChange?.(parsed.page); setJump("") }
            }}
            placeholder="页码"
            className="h-8 w-[76px] rounded-[8px] border bg-surface px-2.5 text-xs text-text-primary outline-none placeholder:text-text-tertiary"
          />
          <button
            type="button"
            onClick={() => {
              const parsed = parsePageInput(jump, pageCount)
              setError(parsed.error)
              if (parsed.page !== null) { onChange?.(parsed.page); setJump("") }
            }}
            className="h-8 rounded-[8px] border bg-surface px-3.5 text-xs font-medium text-text-primary"
          >
            跳转
          </button>
          {error && <span className="text-[11px] text-danger">{error}</span>}
        </div>
      </div>
    </div>
  )
}
