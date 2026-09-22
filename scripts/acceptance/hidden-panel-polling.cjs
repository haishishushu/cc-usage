async page => {
  const probe=await page.context().newPage()
  try {
    await probe.addInitScript(()=>{
      window.__TAURI_INTERNALS__={};window.pollTimers=new Set();window.calls=0
      const start=window.setInterval,end=window.clearInterval
      window.setInterval=(fn,ms,...args)=>{const id=start(fn,ms,...args);if(ms>=60000)window.pollTimers.add(id);return id}
      window.clearInterval=id=>{window.pollTimers.delete(id);end(id)}
    })
    await probe.route('**/src/lib/api.ts*',async route=>{
      const response=await route.fetch()
      await route.fulfill({response,body:(await response.text())+`
        listenEvent=async()=>()=>{};
        api.getSettings=async()=>({refresh_minutes:5});
        api.connectionQuota=api.connectionBalance=api.connectionApiUsage=async()=>{window.calls++;return {state:'ok',windows:[],balance:12,currency:'USD'}};
      `})
    })
    await probe.route('**/src/main.tsx*',route=>route.fulfill({contentType:'application/javascript',body:`
      import React from '/node_modules/.vite/deps/react.js';
      import Client from '/node_modules/.vite/deps/react-dom_client.js';
      import {useQuota,useBalance,useApiUsage} from '/src/lib/useQuota.ts';
      function Fixture(){
        const [active,setActive]=React.useState(true);window.setActive=setActive;
        const quota=useQuota('one',false,active),balance=useBalance('one',active),usage=useApiUsage('one',active);
        return React.createElement('pre',{id:'state'},JSON.stringify([quota.state,balance.state,usage.state]));
      }
      Client.createRoot(document.getElementById('root')).render(React.createElement(Fixture));
    `}))
    await probe.goto('http://localhost:5173')
    await probe.waitForFunction(()=>JSON.parse(document.querySelector('#state')?.textContent||'[]').filter(Boolean).length===3)
    const snapshot=await probe.locator('#state').textContent()
    await probe.evaluate(()=>window.setActive(false))
    await probe.waitForTimeout(50)
    if(await probe.evaluate(()=>window.pollTimers.size)!==0)throw Error('隐藏后仍有网络轮询计时器')
    if(await probe.locator('#state').textContent()!==snapshot)throw Error('暂停轮询清空缓存')
    const calls=await probe.evaluate(()=>window.calls)
    await probe.evaluate(()=>window.setActive(true))
    await probe.waitForFunction(n=>window.calls===n+3,calls)
    if(await probe.locator('#state').textContent()!==snapshot)throw Error('恢复时清空缓存')
    return {pass:true,hiddenPollingStopped:true,cachedDataPreserved:true,resumeRefresh:true}
  } finally {await probe.close()}
}
