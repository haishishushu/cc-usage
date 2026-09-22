import type { DeltaPhase } from "@/components/island/TokenDelta"

export function dockPulseActive(
  deltaTokens: number | null,
  phase: DeltaPhase,
  dnd: boolean,
  dragging: boolean,
): boolean {
  return !dnd && !dragging && (deltaTokens ?? 0) > 0 && (phase === "enter" || phase === "hold")
}
