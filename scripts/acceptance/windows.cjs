async (page) => {
 const browser=await page.context().browser().browserType().connectOverCDP('http://127.0.0.1:9337');const context=browser.contexts()[0];context.setDefaultTimeout(12000)
 const island=context.pages().find(p=>p.url().includes('window=island'))
 const call=(method,...args)=>island.evaluate(async({method,args})=>{const{api}=await import('/src/lib/api.ts');return api[method](...args)},{method,args})
 if(!(await call('dataInfo')).path.replaceAll('\\','/').includes('/output/acceptance/profile/'))throw new Error('Not isolated')
 const results=[]
 for(let i=0;i<12;i++) {
  const created=context.waitForEvent('page');await call('menuAction','open_main');const panel=await created
  await panel.getByRole('button',{name:'设置',exact:true}).waitFor()
  await panel.getByRole('button',{name:'设置',exact:true}).click()
  await panel.getByRole('button',{name:'关闭主面板',exact:true}).click()
  await panel.waitForEvent('close').catch(()=>{if(!panel.isClosed())throw new Error('Panel retained')})
  results.push(context.pages().filter(p=>!p.isClosed()).length)
 }
 return {pass:true,cycles:results}
}
