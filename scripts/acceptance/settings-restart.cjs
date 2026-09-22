async page => {
 const browser=await page.context().browser().browserType().connectOverCDP('http://127.0.0.1:9337');
 const context=browser.contexts()[0]; context.setDefaultTimeout(12000);
 const island=context.pages().find(p=>p.url().includes('window=island'));
 const call=(method,...args)=>island.evaluate(async({method,args})=>(await import('/src/lib/api.ts')).api[method](...args),{method,args});
 if(!(await call('dataInfo')).path.replaceAll('\\','/').includes('/output/acceptance/profile/'))throw Error('Refusing real profile');
 const expected={always_on_top:false,dock_enabled:false,dnd:true,island_opacity:70,island_scale:115,refresh_minutes:15,theme:'dark',retention_days:90,balance_alert_threshold:12.5,balance_alert_currency:'CNY',silent_startup:false};
 const settings=await call('getSettings');
 for(const [k,v] of Object.entries(expected))if(settings[k]!==v)throw Error(`${k} lost on restart`);
 // 窗口销毁后反复打开，覆盖之前同步 IPC 创建窗口导致的 Windows 死锁。
 for(let i=0;i<2;i++){
   const existing=context.pages().find(p=>!p.url().includes('window='));
   const created=existing?Promise.resolve(existing):context.waitForEvent('page');
   await island.evaluate(async()=>Promise.race([(await import('/src/lib/api.ts')).api.openMainPanel(),new Promise((_,reject)=>setTimeout(()=>reject(Error('open main deadlock')),10000))]));
   const panel=await created;
   await panel.getByRole('button',{name:'设置',exact:true}).click();
   await panel.locator('fieldset[aria-busy="false"]').waitFor();
   if(await panel.getByRole('switch',{name:'免打扰',exact:true}).getAttribute('aria-checked')!=='true')throw Error('UI did not restore');
   const closed=panel.waitForEvent('close');
   await panel.getByRole('button',{name:'关闭主面板',exact:true}).click();
   await closed;
 }
 return {pass:true,checks:['11 preferences survive process restart','UI restored','destroy/recreate panel twice']};
}
