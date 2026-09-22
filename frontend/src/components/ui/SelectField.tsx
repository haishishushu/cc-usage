import * as Menu from "@radix-ui/react-dropdown-menu"
import { Check, ChevronDown } from "lucide-react"
import type { ReactNode } from "react"
import { cn } from "@/lib/utils"

export interface SelectOption {
  value: string
  label: string
  description?: string
  icon?: ReactNode
  disabled?: boolean
}

/** 原型统一选择器：菜单、选中、禁用与焦点均由应用主题绘制。 */
export function SelectField({ value, options, onValueChange, label, disabled,
  className, triggerLabel, onOpenChange, readOnlySingle = false,
}: {
  value: string
  options: SelectOption[]
  onValueChange: (value: string) => void
  label: string
  disabled?: boolean
  className?: string
  triggerLabel?: ReactNode
  onOpenChange?: (open: boolean) => void
  readOnlySingle?: boolean
}) {
  const selected = options.find((option) => option.value === value)
  if (readOnlySingle && value && selected && options.filter((option) => option.value && !option.disabled).length === 1) {
    return <span aria-label={label} title={selected.label} className={cn("inline-flex h-8 min-w-0 items-center text-xs text-text-primary", className)}><span className="truncate">{selected.label}</span></span>
  }
  return (
    <Menu.Root modal={false} onOpenChange={onOpenChange}>
      <Menu.Trigger disabled={disabled}
        aria-label={label}
        className={cn("inline-flex h-8 min-w-0 shrink-0 items-center justify-between gap-2 rounded-[8px] border bg-surface px-2.5 text-left text-xs text-text-primary outline-none transition-colors hover:bg-hover focus-visible:border-accent-blue focus-visible:ring-2 focus-visible:ring-accent-blue-soft data-[state=open]:border-border-strong disabled:cursor-not-allowed disabled:opacity-45", className)}
      >
        <span className="min-w-0 truncate">{triggerLabel ?? selected?.label ?? "请选择"}</span>
        <ChevronDown aria-hidden className="motion-select-chevron size-3 shrink-0 text-text-secondary" />
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Content loop align="end" sideOffset={6} collisionPadding={12}
          className="motion-popover z-[100] min-w-[var(--radix-dropdown-menu-trigger-width)] max-w-[min(360px,calc(100vw-24px))] overflow-y-auto overscroll-contain rounded-[8px] border bg-surface text-text-primary shadow-popover"
          style={{ maxHeight: "min(248px, var(--radix-dropdown-menu-content-available-height))" }}
        >
          <Menu.RadioGroup value={value} onValueChange={onValueChange} className="p-1">
            {options.map((option) => (
              <Menu.RadioItem key={option.value} value={option.value} disabled={option.disabled}
                textValue={option.label}
                className="relative flex min-h-8 cursor-default select-none items-center gap-2 rounded-[6px] py-1.5 pl-7 pr-3 text-xs outline-none data-[highlighted]:bg-surface-3 data-[state=checked]:bg-surface-3 data-[disabled]:opacity-40"
              >
                <Menu.ItemIndicator className="absolute left-2"><Check aria-hidden className="size-3" /></Menu.ItemIndicator>
                {option.icon && <span aria-hidden className="grid size-4 shrink-0 place-items-center">{option.icon}</span>}
                <span className="min-w-0">
                  <span>{option.label}</span>
                  {option.description && <span className="mt-0.5 block max-w-[290px] truncate text-[10px] text-text-secondary" title={option.description}>{option.description}</span>}
                </span>
              </Menu.RadioItem>
            ))}
          </Menu.RadioGroup>
        </Menu.Content>
      </Menu.Portal>
    </Menu.Root>
  )
}
