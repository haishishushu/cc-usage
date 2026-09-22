async page => {
  const probe=await page.context().newPage()
  try {
    await probe.addInitScript(()=>{window.__TAURI_INTERNALS__={};window.pending=[]})
    await probe.route('**/src/lib/api.ts*',async route=>{
      const response=await route.fetch()
      await route.fulfill({response,body:(await response.text())+`
        const defer=value=>new Promise(resolve=>window.pending.push(()=>resolve(value)));
        api.tokenTotals=()=>defer({today:10,week:20,month:30,total:40,collected_since:null});
        api.usageTrend=()=>defer({points:[{label:'10:00',tokens:10}],bucket:'小时'});
        api.requestLog=()=>defer({rows:[{id:1,input:10,output:20,cost_estimate:null}],total_count:61,page_count:4});
      `})
    })
    await probe.route('**/src/main.tsx*',route=>route.fulfill({contentType:'application/javascript',body:`
      import React from '/node_modules/.vite/deps/react.js';
      import Client from '/node_modules/.vite/deps/react-dom_client.js';
      import {useTokenSummaries,useTrend,useRequestLog} from '/src/lib/useUsage.ts';
      function Fixture(){
        const [key,setKey]=React.useState(0),[platform,setPlatform]=React.useState('claude');
        window.refresh=()=>setKey(k=>k+1);window.switchPlatform=()=>setPlatform('codex');
        const summary=useTokenSummaries(platform,null,key,key),trend=useTrend(platform,'today',null,key,key),log=useRequestLog(platform,'today',2,null,key,key);
        return React.createElement('pre',{id:'state'},JSON.stringify({summary,trend,log}));
      }
      Client.createRoot(document.getElementById('root')).render(React.createElement(Fixture));
    `}))
    await probe.goto('http://localhost:5173')
    await probe.waitForFunction(()=>window.pending.length===3)
    await probe.evaluate(()=>window.pending.splice(0).forEach(resolve=>resolve()))
    await probe.waitForFunction(()=>JSON.parse(document.querySelector('#state').textContent).log.status==='ready')
    const before=await probe.locator('#state').textContent()
    await probe.evaluate(()=>window.refresh())
    await probe.waitForFunction(()=>window.pending.length===3)
    if(await probe.locator('#state').textContent()!==before)throw Error('后台刷新清空了现有数据/分页')
    await probe.evaluate(()=>window.switchPlatform())
    await probe.waitForFunction(()=>window.pending.length===6)
    const changed=JSON.parse(await probe.locator('#state').textContent())
    if(changed.log.rows.length||changed.trend.chart.bars.length||changed.summary.summaries.length)throw Error('切换平台泄漏旧数据')
    await probe.evaluate(()=>window.pending.splice(0,3).forEach(resolve=>resolve()))
    if(JSON.parse(await probe.locator('#state').textContent()).log.status!=='loading')throw Error('过期请求覆盖新筛选')
    await probe.evaluate(()=>window.pending.splice(0).forEach(resolve=>resolve()))
    await probe.waitForFunction(()=>JSON.parse(document.querySelector('#state').textContent).log.status==='ready')
    return {pass:true,backgroundStable:true,queryIsolation:true,staleResponseIgnored:true}
  } finally {await probe.close()}
}
