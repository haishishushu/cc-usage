import type { DeltaPhase } from "@/components/island/TokenDelta"

/** 停靠条只在真实新增（追数合计大于 0）且允许动效时脉冲。 */
export function dockPulseActive(
  hasDelta: boolean,
  phase: DeltaPhase,
  dnd: boolean,
  dragging: boolean,
): boolean {
  return !dnd && !dragging && hasDelta && (phase === "enter" || phase === "hold")
}
