async page => {
  const probe=await page.context().newPage()
  try {
    await probe.addInitScript(()=>{
      window.observerStarts=0
      const Original=window.ResizeObserver
      window.ResizeObserver=class extends Original {constructor(fn){super(fn);window.observerStarts++}}
    })
    await probe.route('**/src/main.tsx*',route=>route.fulfill({contentType:'application/javascript',body:`
      import React from '/node_modules/.vite/deps/react.js';
      import Client from '/node_modules/.vite/deps/react-dom_client.js';
      import DOM from '/node_modules/.vite/deps/react-dom.js';
      import {useContentMotion} from '/src/lib/motion.ts';
      import {IslandMotion} from '/src/components/island/IslandMotion.tsx';
      import {Segmented} from '/src/components/ui/primitives.tsx';
      import '/src/index.css';
      const h=React.createElement;
      function Fixture(){
        const [key,setKey]=React.useState(0),[tick,setTick]=React.useState(0),[expanded,setExpanded]=React.useState(false);
        const ref=useContentMotion(String(key),180,0);
        window.step=()=>DOM.flushSync(()=>setKey(k=>k+1));
        window.background=()=>DOM.flushSync(()=>setTick(k=>k+1));
        window.toggle=()=>DOM.flushSync(()=>setExpanded(v=>!v));
        return h('div',null,h('div',{id:'content',ref},'页面 '+key),h(Segmented,{items:[{value:'a',label:'总览'},{value:'b',label:'设置'}],value:'a'}),
          h(IslandMotion,{expanded,dragging:false},h('div',{className:'island-shell',style:{height:expanded?360:100}},h('div',null,'稳定标题'),...Array.from({length:80},(_,i)=>h('div',{key:i,style:{height:3}},'数据 '+i+' '+tick)))));
      }
      Client.createRoot(document.getElementById('root')).render(h(Fixture));
    `}))
    await probe.goto('http://localhost:5173')
    await probe.locator('#content').waitFor()
    await probe.waitForTimeout(80)
    const result=await probe.evaluate(async()=>{
      const content=document.querySelector('#content')
      window.step()
      await new Promise(resolve=>setTimeout(resolve,70))
      const before=Number(getComputedStyle(content).opacity)
      window.step()
      const after=Number(getComputedStyle(content).opacity)
      await new Promise(resolve=>setTimeout(resolve,220))
      const starts=window.observerStarts
      for(let i=0;i<20;i++)window.background()
      return {rapidOpacity:{before,after,jump:Math.abs(after-before)},backgroundObserverStarts:window.observerStarts-starts}
    })
    const cdp=await probe.context().newCDPSession(probe)
    await cdp.send('Performance.enable')
    const metrics=async()=>Object.fromEntries((await cdp.send('Performance.getMetrics')).metrics.map(m=>[m.name,m.value]))
    const before=await metrics()
    result.frames=await probe.evaluate(async()=>{
      const gaps=[];let previous=performance.now()
      window.toggle()
      for(let i=0;i<28;i++){await new Promise(requestAnimationFrame);const now=performance.now();gaps.push(now-previous);previous=now}
      return {count:gaps.length,over34ms:gaps.filter(x=>x>34).length,maxMs:Math.max(...gaps)}
    })
    const after=await metrics()
    result.layout={count:after.LayoutCount-before.LayoutCount,durationMs:(after.LayoutDuration-before.LayoutDuration)*1000,recalcMs:(after.RecalcStyleDuration-before.RecalcStyleDuration)*1000}
    result.themeKeepsMotion=await probe.evaluate(()=>{
      document.documentElement.classList.add('theme-transition')
      return getComputedStyle(document.querySelector('[data-motion-indicator]')).transitionProperty.includes('transform')
    })
    if(result.rapidOpacity.jump>0.01)throw Error('快速切换透明度跳变: '+JSON.stringify(result))
    if(result.backgroundObserverStarts!==0)throw Error('数据刷新重建监听: '+JSON.stringify(result))
    if(!result.themeKeepsMotion)throw Error('主题切换覆盖运动过渡')
    return result
  } finally {await probe.close()}
}
