import { useEffect, useMemo, useState } from "react"
import { CalendarRange, ChevronLeft, ChevronRight, CircleAlert } from "lucide-react"
import { cn } from "@/lib/utils"
import { Checkbox } from "@/components/ui/primitives"
import type { CustomRange } from "@/lib/api"

/**
 * 自定义时间查询面板 —— §2.3 / 画布 10
 * 宽 620（左侧 fill + 分隔 1 + 日历 308），日期格高 32。
 *
 * 蓝色仅用于该筛选控件的选中态与确定按钮，不替换绿色额度进度条。
 * 时间范围只筛选历史 Token 查询，不改变官方账号 5h/7d 额度窗口，
 * 也不改变灵动岛显示平台的配置。
 *
 * 交互：点击起止区域切换右侧日历编辑的端点，当前编辑区域高亮边框；
 * 勾选「跟随当前时刻」后结束时间禁用，查询时取当时的现在。
 */
const WEEKDAYS = ["日", "一", "二", "三", "四", "五", "六"]

type Endpoint = "start" | "end"

const pad = (n: number) => String(n).padStart(2, "0")
const fmtDate = (d: Date) => `${d.getFullYear()}/${pad(d.getMonth() + 1)}/${pad(d.getDate())}`
const inputDate = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
const fmtTime = (d: Date) => `${pad(d.getHours())}:${pad(d.getMinutes())}`

function parseDateInput(value: string): Date | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value)
  if (!match) return null
  const date = new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3]))
  return date.getFullYear() === Number(match[1])
    && date.getMonth() === Number(match[2]) - 1
    && date.getDate() === Number(match[3]) ? date : null
}

/** 解析 `HH:MM`，非法输入返回 null 交由调用方保留原值 */
function parseTime(v: string): { h: number; m: number } | null {
  const m = /^(\d{1,2}):(\d{1,2})$/.exec(v.trim())
  if (!m) return null
  const h = Number(m[1])
  const min = Number(m[2])
  if (h > 23 || min > 59) return null
  return { h, m: min }
}

/** 某月的 6×7 日历网格，含上下月补格 */
function monthGrid(year: number, month: number) {
  const first = new Date(year, month, 1)
  const startDow = first.getDay()
  const cells: Date[] = []
  for (let i = 0; i < 42; i++) {
    cells.push(new Date(year, month, 1 - startDow + i))
  }
  // 末行整周都属于下个月时裁掉，避免多出一整行空白
  const weeks: Date[][] = []
  for (let i = 0; i < 42; i += 7) weeks.push(cells.slice(i, i + 7))
  while (weeks.length > 4 && weeks[weeks.length - 1].every((d) => d.getMonth() !== month)) {
    weeks.pop()
  }
  return weeks
}

function TimeField({
  label,
  date,
  time,
  focused,
  disabled,
  onFocus,
  onTimeChange,
  onDateChange,
  onDateValidityChange,
}: {
  label: string
  date: Date
  time: string
  focused?: boolean
  disabled?: boolean
  onFocus?: () => void
  onTimeChange?: (v: string) => void
  onDateChange?: (date: Date) => void
  onDateValidityChange?: (valid: boolean) => void
}) {
  const [dateText, setDateText] = useState(() => inputDate(date))
  useEffect(() => { setDateText(inputDate(date)) }, [date])
  const invalidDate = !parseDateInput(dateText)
  return (
    <div className="flex w-full flex-col gap-1.5">
      <span
        className={cn(
          "text-xs",
          focused ? "font-semibold text-accent-blue" : "text-text-secondary",
          disabled && "text-text-tertiary",
        )}
      >
        {label}
      </span>
      <div className="flex w-full gap-2">
        <input
          type="text"
          inputMode="numeric"
          aria-label={`${label}日期`}
          aria-invalid={!disabled && invalidDate}
          placeholder="YYYY-MM-DD"
          disabled={disabled}
          value={dateText}
          onFocus={onFocus}
          onChange={(event) => {
            const next = parseDateInput(event.target.value)
            setDateText(event.target.value)
            onDateValidityChange?.(!!next)
            if (next) onDateChange?.(next)
          }}
          style={{ width: 124 }}
          className={cn(
            "rounded-[8px] px-2.5 py-2 font-mono text-[13px] text-text-primary outline-none",
            disabled ? "border bg-surface-3 opacity-55" : "bg-surface",
            focused ? "border-[1.5px] border-accent-blue" : "border",
          )}
        />
        <input
          value={time}
          aria-label={`${label}时分`}
          inputMode="numeric"
          disabled={disabled}
          onFocus={onFocus}
          onChange={(e) => onTimeChange?.(e.target.value)}
          style={{ width: 74 }}
          className={cn(
            "rounded-[8px] px-2.5 py-2 font-mono text-[13px] text-text-primary outline-none",
            disabled ? "border bg-surface-3 opacity-55" : "bg-surface",
            focused ? "border-[1.5px] border-accent-blue" : "border",
          )}
        />
      </div>
    </div>
  )
}

export function DateRangePicker({
  followNow = false,
  initial,
  onApply,
  onCancel,
  onQuick,
}: {
  followNow?: boolean
  /** 已生效的范围，用于打开面板时回填 */
  initial?: CustomRange | null
  onApply?: (r: CustomRange) => void
  onCancel?: () => void
  /** 快捷范围复用已确认的四个周期，不新增 1d/7d/14d/30d */
  onQuick?: (period: "today" | "week" | "month" | "total") => void
}) {
  const now = new Date()
  const [follow, setFollow] = useState(initial ? initial.end === null : followNow)
  const [start, setStart] = useState<Date>(
    initial ? new Date(initial.start) : new Date(now.getFullYear(), now.getMonth(), now.getDate()),
  )
  const [end, setEnd] = useState<Date>(initial?.end ? new Date(initial.end) : now)
  const [editing, setEditing] = useState<Endpoint>("start")
  // 时分输入允许中间态（用户正打字），因此单独存字符串
  const [startTime, setStartTime] = useState(fmtTime(start))
  const [endTime, setEndTime] = useState(fmtTime(end))
  const [startDateValid, setStartDateValid] = useState(true)
  const [endDateValid, setEndDateValid] = useState(true)
  const [view, setView] = useState(() => new Date(start.getFullYear(), start.getMonth(), 1))

  const weeks = useMemo(() => monthGrid(view.getFullYear(), view.getMonth()), [view])

  /** 把时分输入合并到日期上，得到最终时间点 */
  const compose = (d: Date, timeStr: string) => {
    const t = parseTime(timeStr)
    const out = new Date(d)
    if (t) out.setHours(t.h, t.m, 0, 0)
    return out
  }

  const startAt = compose(start, startTime)
  const endAt = follow ? now : compose(end, endTime)

  // 「确定」前校验开始早于结束；无效范围就近提示且不能提交
  const error =
    !startDateValid || (!follow && !endDateValid)
      ? "请输入有效日期，格式为 YYYY-MM-DD"
      : !parseTime(startTime) || (!follow && !parseTime(endTime))
      ? "时间格式需为 HH:MM"
      : startAt.getTime() >= endAt.getTime()
        ? "开始时间必须早于结束时间"
        : null

  const pickDate = (d: Date) => {
    if (editing === "start") { setStart(d); setStartDateValid(true) }
    else if (!follow) { setEnd(d); setEndDateValid(true) }
  }

  const target = editing === "start" ? start : end
  const shiftMonth = (delta: number) =>
    setView((v) => new Date(v.getFullYear(), v.getMonth() + delta, 1))

  return (
    <div className="w-[620px] overflow-hidden rounded-xl border bg-surface shadow-dialog">
      {/* 快捷范围复用已确认的今日 / 本周 / 本月 / 累计，不新增 1d/7d/14d/30d */}
      <div className="flex w-full items-center gap-2 border-b px-4 py-3">
        <span className="text-xs text-text-secondary">快捷范围</span>
        {(
          [
            ["今日", "today"],
            ["本周", "week"],
            ["本月", "month"],
            ["累计", "total"],
          ] as const
        ).map(([label, key]) => (
          <button
            key={key}
            type="button"
            onClick={() => onQuick?.(key)}
            className="rounded-[6px] bg-surface-3 px-2.5 py-1 text-xs text-text-primary"
          >
            {label}
          </button>
        ))}
      </div>

      <div className="flex w-full items-start">
        <div className="flex flex-1 flex-col gap-3.5 p-4">
          <TimeField
            label={editing === "start" ? "开始时间（当前编辑）" : "开始时间"}
            date={start}
            time={startTime}
            focused={editing === "start"}
            onFocus={() => {
              setEditing("start")
              setView(new Date(start.getFullYear(), start.getMonth(), 1))
            }}
            onTimeChange={setStartTime}
            onDateValidityChange={setStartDateValid}
            onDateChange={(date) => {
              setStart(date)
              setView(new Date(date.getFullYear(), date.getMonth(), 1))
            }}
          />
          <TimeField
            label={
              follow
                ? "结束时间（跟随当前时刻 · 已禁用）"
                : editing === "end"
                  ? "结束时间（当前编辑）"
                  : "结束时间"
            }
            date={end}
            time={follow ? fmtTime(now) : endTime}
            focused={!follow && editing === "end"}
            disabled={follow}
            onFocus={() => {
              setEditing("end")
              setView(new Date(end.getFullYear(), end.getMonth(), 1))
            }}
            onTimeChange={setEndTime}
            onDateValidityChange={setEndDateValid}
            onDateChange={(date) => {
              setEnd(date)
              setView(new Date(date.getFullYear(), date.getMonth(), 1))
            }}
          />

          <label className="flex w-full cursor-pointer items-center gap-2 rounded-[6px] focus-within:ring-2 focus-within:ring-accent-blue-soft">
            <input
              type="checkbox"
              className="sr-only"
              checked={follow}
              onChange={() => {
                const next = !follow
                setFollow(next)
                // 勾选后结束端点不可编辑，把编辑焦点移回开始时间
                if (next) setEditing("start")
              }}
            />
            <span aria-hidden="true">
              <Checkbox checked={follow} />
            </span>
            <span className="text-xs text-text-primary">结束时间跟随当前时刻</span>
          </label>

          {error && (
            <div className="flex w-full items-center gap-1.5 rounded-[8px] bg-danger-soft px-2.5 py-2">
              <CircleAlert className="size-3 shrink-0 text-danger" />
              <span className="text-[11px] leading-[1.5] text-danger">{error}</span>
            </div>
          )}

          <div className="flex w-full justify-end gap-2">
            {/* 取消放弃本次编辑，保留之前生效的查询范围 */}
            <button
              type="button"
              onClick={onCancel}
              className="rounded-[8px] border bg-surface px-4 py-2 text-xs text-text-primary"
            >
              取消
            </button>
            <button
              type="button"
              disabled={!!error}
              onClick={() =>
                onApply?.({
                  start: startAt.getTime(),
                  // 跟随当前时刻时不固化结束点，由后端每次查询取现在
                  end: follow ? null : endAt.getTime(),
                })
              }
              className="rounded-[8px] bg-accent-blue px-4 py-2 text-xs font-semibold text-white disabled:opacity-50"
            >
              确定
            </button>
          </div>
        </div>

        <div className="h-[296px] w-px shrink-0 bg-[var(--border)]" />

        <div className="flex w-[308px] shrink-0 flex-col gap-2.5 p-4">
          <div className="flex w-full items-center justify-between">
            <button type="button" onClick={() => shiftMonth(-1)} aria-label="上个月">
              <ChevronLeft className="size-4 text-text-secondary" />
            </button>
            <span className="text-[13px] font-semibold text-text-primary">
              {view.getFullYear()} 年 {view.getMonth() + 1} 月
            </span>
            <button type="button" onClick={() => shiftMonth(1)} aria-label="下个月">
              <ChevronRight className="size-4 text-text-secondary" />
            </button>
          </div>
          <div className="flex w-full gap-0.5">
            {WEEKDAYS.map((w) => (
              <span key={w} className="flex-1 text-center text-[11px] text-text-tertiary">
                {w}
              </span>
            ))}
          </div>
          {weeks.map((week, wi) => (
            <div key={wi} className="flex w-full gap-0.5">
              {week.map((d, di) => {
                const outside = d.getMonth() !== view.getMonth()
                const selected =
                  d.getFullYear() === target.getFullYear() &&
                  d.getMonth() === target.getMonth() &&
                  d.getDate() === target.getDate()
                return (
                  <button
                    key={di}
                    type="button"
                    onClick={() => pickDate(d)}
                    className={cn(
                      "motion-button tnum grid h-8 flex-1 place-items-center rounded-[7px] font-mono text-xs",
                      !selected && "hover:bg-hover",
                      selected && "bg-accent-blue font-semibold text-white",
                      !selected && outside && "text-text-tertiary",
                      !selected && !outside && "text-text-primary",
                    )}
                  >
                    {d.getDate()}
                  </button>
                )
              })}
            </div>
          ))}
        </div>
      </div>

      <div className="flex w-full items-center gap-2 border-t bg-surface-2 px-4 py-2.5">
        <CalendarRange className="size-3 shrink-0 text-text-secondary" />
        <span className="text-[11px] leading-[1.5] text-text-tertiary">
          {follow
            ? `范围 ${fmtDate(start)} ${startTime} 起，结束时间跟随当前时刻`
            : `范围 ${fmtDate(start)} ${startTime} — ${fmtDate(end)} ${endTime}`}
          ，仅筛选历史 Token 查询，不改变额度窗口
        </span>
      </div>
    </div>
  )
}
