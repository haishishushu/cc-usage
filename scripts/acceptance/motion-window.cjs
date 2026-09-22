async page => {
  const probe = await page.context().newPage()
  const errors = []
  probe.on('pageerror', error => errors.push(error.message))
  try {
    await probe.setViewportSize({width:700,height:150})
    await probe.addInitScript(() => { window.__TAURI_INTERNALS__ = {} })
    await probe.route('**/src/main.tsx*', route => route.fulfill({contentType:'application/javascript',body:`
      import React from '/node_modules/.vite/deps/react.js';
      import Client from '/node_modules/.vite/deps/react-dom_client.js';
      import {IslandMotion} from '/src/components/island/IslandMotion.tsx';
      import '/src/index.css';
      const h=React.createElement;
      function Fixture(){
        const [expanded,setExpanded]=React.useState(false);
        const [extra,setExtra]=React.useState(0);
        window.toggleMotion=()=>setExpanded(v=>!v);
        window.growContent=()=>setExtra(v=>v+20);
        return h(IslandMotion,{expanded,dragging:false},h('div',{className:'island-shell',style:{height:(expanded?300:100)+extra}},h('div',null,'标题始终可见'),h('div',null,'额度内容')));
      }
      Client.createRoot(document.getElementById('root')).render(h(React.StrictMode,null,h(Fixture)));
    `}))
    await probe.goto('http://localhost:5173')
    await probe.locator('.island-morph').waitFor({timeout:5000}).catch(error => { throw Error(errors.join('\n') || error.message) })
    await probe.evaluate(() => window.toggleMotion())
    await probe.waitForTimeout(350)
    const waiting=await probe.locator('.island-morph').evaluate(e=>({height:e.offsetHeight,state:e.getAnimations()[0]?.playState,reserved:e.parentElement.offsetHeight}))
    if(waiting.height!==100 || waiting.state!=='paused' || waiting.reserved!==300) throw Error('窗口空间不足时动画没有等待尺寸确认: '+JSON.stringify(waiting))
    await probe.setViewportSize({width:700,height:400})
    await probe.waitForFunction(()=>document.querySelector('.island-morph').offsetHeight===300)
    await probe.waitForFunction(()=>document.querySelector('.island-morph').getAnimations().length===0)
    await probe.evaluate(()=>window.growContent())
    const background=await probe.locator('.island-shell').evaluate(e=>[...e.children].every(child=>child.getAnimations().length===0))
    if(!background)throw Error('数据高度变化导致无关内容闪烁')
    await probe.waitForFunction(()=>document.querySelector('.island-morph').offsetHeight===320)
    await probe.evaluate(()=>window.toggleMotion())
    const reserve=await probe.locator('[data-island-motion]').evaluate(e=>e.offsetHeight)
    if(reserve!==320)throw Error('收起期间过早缩小预留窗口')
    await probe.waitForFunction(()=>document.querySelector('[data-island-motion]').offsetHeight===120)
    await probe.setViewportSize({width:700,height:150})
    await probe.evaluate(()=>{document.getElementById('root').style.padding='14px 0'})
    await probe.evaluate(()=>window.toggleMotion())
    await probe.evaluate(()=>window.dispatchEvent(new CustomEvent('island-size-limit',{detail:150})))
    await probe.waitForFunction(()=>document.querySelector('.island-morph').offsetHeight===122 && document.querySelector('.island-morph').getAnimations().length===0)
    const scrollable=await probe.locator('.island-morph > div').evaluate(e=>getComputedStyle(e).overflowY==='auto' && e.scrollHeight>e.clientHeight)
    if(!scrollable)throw Error('工作区高度受限后无法滚动访问内容')
    await probe.setViewportSize({width:700,height:400})
    await probe.evaluate(()=>window.dispatchEvent(new CustomEvent('island-size-limit',{detail:null})))
    await probe.waitForFunction(()=>document.querySelector('.island-morph').offsetHeight===320)
    return {pass:true,resizeAcknowledgement:true,backgroundRefreshNoFlash:true,shrinkAfterAnimation:true,limitedWorkArea:true}
  } finally { await probe.close() }
}
