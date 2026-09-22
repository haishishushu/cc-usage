async page => {
  const probe = await page.context().newPage()
  const errors = []
  probe.on('pageerror', error => errors.push(error.message))
  try {
    await probe.route('**/src/main.tsx*', route => route.fulfill({contentType:'application/javascript',body:`
      import React from '/node_modules/.vite/deps/react.js';
      import Client from '/node_modules/.vite/deps/react-dom_client.js';
      import {SettingsView} from '/src/views/SettingsView.tsx';
      import {api} from '/src/lib/api.ts';
      import '/src/index.css';
      api.dataInfo=async()=>({path:'test',size_bytes:0});
      api.autostartStatus=async()=>true;
      window.saveCalls=0;
      api.setDnd=()=>{window.saveCalls++;return new Promise((resolve,reject)=>{window.rejectSave=()=>reject(new Error('test failure'));window.resolveSave=()=>resolve({silent_startup:true,island_platform:'claude',island_kind:'api',island_connection_id:null,island_connection_name:null,island_source_id:'local:claude',always_on_top:true,theme:'light',island_opacity:100,island_scale:100,refresh_minutes:5,balance_alert_threshold:null,balance_alert_currency:'USD',dock_enabled:true,retention_days:null,dock:{edge:null,offset:0,monitor:null},dnd:true,island_visible:true})})};
      Client.createRoot(document.getElementById('root')).render(React.createElement(SettingsView,{section:'island',islandPlatform:'claude'}));
    `}))
    await probe.goto('http://localhost:5173')
    const toggle = probe.getByRole('switch',{name:'免打扰',exact:true})
    await toggle.waitFor()
    await probe.waitForTimeout(250)
    const snapshot = () => probe.evaluate(() => ['透明度','大小','自动刷新间隔','静默启动'].map(label=>{
      const e=document.querySelector('[aria-label="'+label+'"]')
      const s=getComputedStyle(e),r=e.getBoundingClientRect()
      return {label,opacity:s.opacity,color:s.color,background:s.backgroundColor,width:r.width,height:r.height}
    }))
    const before=await snapshot()
    await toggle.click()
    await probe.waitForFunction(()=>window.saveCalls===1)
    const during=await snapshot()
    if(JSON.stringify(before)!==JSON.stringify(during))throw Error('保存时无关控件闪变: '+JSON.stringify({before,during}))
    if(!await toggle.isDisabled())throw Error('保存期间未阻止重复提交')
    await toggle.evaluate(e=>e.click())
    if(await probe.evaluate(()=>window.saveCalls)!==1)throw Error('产生重复请求')
    await probe.evaluate(()=>window.rejectSave())
    await probe.getByRole('alert').filter({hasText:'设置保存失败'}).waitFor()
    if(await toggle.getAttribute('aria-checked')!=='false')throw Error('保存失败后显示了未保存的值')
    if(await toggle.isDisabled())throw Error('失败后控件没有恢复')
    if(JSON.stringify(before)!==JSON.stringify(await snapshot()))throw Error('保存结束后无关样式变化')
    await probe.evaluate(()=>document.documentElement.classList.add('dark'))
    await probe.waitForTimeout(200)
    const darkBefore=await snapshot()
    await toggle.click()
    await probe.waitForFunction(()=>window.saveCalls===2)
    if(JSON.stringify(darkBefore)!==JSON.stringify(await snapshot()))throw Error('深色主题保存闪变')
    await probe.evaluate(()=>window.resolveSave())
    await probe.waitForFunction(()=>document.querySelector('[aria-label="免打扰"]').getAttribute('aria-checked')==='true')
    if(await toggle.isDisabled())throw Error('成功后控件没有恢复')
    if(JSON.stringify(darkBefore)!==JSON.stringify(await snapshot()))throw Error('成功后无关样式变化')
    if(errors.length)throw Error(errors.join('\n'))
    return {pass:true,unrelatedControlsStable:true,duplicateBlocked:true,failureRecovered:true,successApplied:true,darkThemeStable:true}
  } finally { await probe.close() }
}
