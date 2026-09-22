async (page) => {
 const browser=await page.context().browser().browserType().connectOverCDP('http://127.0.0.1:9337');const context=browser.contexts()[0];context.setDefaultTimeout(12000)
 const island=context.pages().find(p=>p.url().includes('window=island'))
 const call=(method,...args)=>island.evaluate(async({method,args})=>{const{api}=await import('/src/lib/api.ts');return api[method](...args)},{method,args})
 if(!(await call('dataInfo')).path.replaceAll('\\','/').includes('/output/acceptance/profile/'))throw new Error('Not isolated')
 const results=[]
 const geometry=[]
 for(const edge of ['top','bottom','left','right']) {
  await call('menuAction','pos_'+edge)
  const cfg=await call('getSettings');if(cfg.dock.edge!==edge)throw new Error('Wrong edge')
  const stable=await island.waitForFunction(async edge=>{
   const{getCurrentWindow,currentMonitor}=await import('/node_modules/@tauri-apps/api/window.js');const w=getCurrentWindow(),p=await w.outerPosition(),s=await w.outerSize(),m=await currentMonitor();const a=m.workArea;
   const err=edge==='top'||edge==='bottom'?Math.abs(p.x+s.width/2-(a.position.x+a.size.width/2)):Math.abs(p.y+s.height/2-(a.position.y+a.size.height/2));
   const key=JSON.stringify([edge,p,s]);const previous=window.__acceptanceGeometry;window.__acceptanceGeometry={key,count:previous?.key===key?previous.count+1:0};
   return err<=1&&window.__acceptanceGeometry.count>=5?{err,p,s}:false
  },edge)
  const err=await island.evaluate(async edge=>{const{getCurrentWindow,currentMonitor}=await import('/node_modules/@tauri-apps/api/window.js');const w=getCurrentWindow(),p=await w.outerPosition(),s=await w.outerSize(),m=await currentMonitor(),a=m.workArea;return edge==='top'||edge==='bottom'?Math.abs(p.x+s.width/2-(a.position.x+a.size.width/2)):Math.abs(p.y+s.height/2-(a.position.y+a.size.height/2))},edge)
  if(err>1)throw new Error('Unstable center '+edge+':'+err)

  geometry.push({edge,center_error_px:err})
 }
 await call('menuAction','pos_free')
 return {pass:true,cycles:results,single_monitor_centering:geometry}
}
