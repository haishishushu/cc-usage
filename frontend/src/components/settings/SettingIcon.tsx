import {
  ArrowRightLeft, Bell, Blend, Database, EyeOff, Globe, Languages,
  Moon, PanelBottom, PanelLeftClose, Pin, Power, RefreshCw, Scaling,
  Settings2, Trash2, type LucideIcon,
} from "lucide-react"
import { cn } from "@/lib/utils"

const icons: Record<string, [LucideIcon, string]> = {
  开机启动: [Power, "text-warn"],
  静默启动: [EyeOff, "text-success-text"],
  灵动岛置顶: [Pin, "text-accent-blue"],
  系统托盘: [PanelBottom, "text-accent-blue"],
  自动刷新间隔: [RefreshCw, "text-accent-blue"],
  余额提醒: [Bell, "text-warn"],
  语言: [Languages, "text-purple-text"],
  统计时区: [Globe, "text-accent-blue"],
  贴边停靠: [PanelLeftClose, "text-accent-blue"],
  免打扰: [Moon, "text-purple-text"],
  透明度: [Blend, "text-purple-text"],
  大小: [Scaling, "text-accent-blue"],
  数据库位置: [Database, "text-accent-blue"],
  历史清理: [Trash2, "text-warn"],
  "导入 / 导出": [ArrowRightLeft, "text-success-text"],
}

export function SettingIcon({ label }: { label: string }) {
  const [Icon, color] = icons[label] ?? [Settings2, "text-accent-blue"]
  return (
    <span aria-hidden="true" className={cn("grid size-[34px] shrink-0 place-items-center rounded-[10px] border bg-surface-2", color)}>
      <Icon className="size-[17px]" strokeWidth={1.75} />
    </span>
  )
}
