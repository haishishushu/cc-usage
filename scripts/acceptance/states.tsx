import React from "react"
import {createRoot} from "react-dom/client"
import "./src/index.css"
import {BalanceCard} from "./src/components/panel/BalanceCard"
import {IslandCollapsed,IslandExpanded} from "./src/components/island/UsageIsland"
import {BALANCE,CLAUDE_QUOTAS,SESSIONS} from "./src/mock/data"
const data={platform:"claude" as const,platformName:"Claude",kind:"auth" as const,quotas:CLAUDE_QUOTAS,sessions:SESSIONS,deltaText:"+12.4K Token",sessionCountText:"3 个会话运行中"}
createRoot(document.getElementById("root")!).render(<div className="flex flex-col gap-6 bg-bg p-8 text-text-primary">
<h1>仅用于验收的模拟状态</h1>
{["loading","unsupported","forbidden","failed","loaded"].map(state=><div data-case={state} key={state}><BalanceCard balance={BALANCE} queryState={state==="loaded"?undefined:state as any} reason="验收模拟状态"/></div>)}
{["healthy","low","empty","unavailable"].map(level=><div data-case={level} key={level}><BalanceCard balance={{...BALANCE,level:level as any,amountText:level==="empty"?"0.00":level==="unavailable"?null:"12.50"}}/></div>)}
<div className="flex flex-wrap gap-6">{[0,70,90,100].map(used=><div data-case={`quota-${used}`} key={used}><IslandCollapsed data={{...data,quotas:CLAUDE_QUOTAS.map(q=>({...q,usedPercent:used}))}}/></div>)}</div>
<div className="flex flex-wrap gap-6">{[0,1,3].map(count=><div data-case={`sessions-${count}`} key={count}><IslandExpanded data={{...data,sessions:SESSIONS.slice(0,count)}}/></div>)}</div>
</div>)
