async page => {
  const probe=await page.context().newPage()
  try {
    await probe.route('**/src/main.tsx*',route=>route.fulfill({contentType:'application/javascript',body:`
      import React from '/node_modules/.vite/deps/react.js';
      import Client from '/node_modules/.vite/deps/react-dom_client.js';
      import {SessionList} from '/src/components/island/SessionList.tsx';
      import '/src/index.css';
      window.parentDrags=0;window.parentDoubles=0;
      function Fixture(){
        const [count,setCount]=React.useState(8),[tick,setTick]=React.useState(0);
        window.setCount=setCount;window.updateTokens=()=>setTick(t=>t+1);
        return React.createElement('div',{style:{width:400},onPointerDown:()=>window.parentDrags++,onDoubleClick:()=>window.parentDoubles++},
          React.createElement('div',{id:'header'},'固定额度区域'),
          React.createElement(SessionList,{sessions:Array.from({length:count},(_,i)=>({id:String(i),title:'会话 '+(i+1),state:'running',deltaText:(i+tick)+'K',startedAtMs:Date.now()-10000}))}),
          React.createElement('div',{id:'footer'},'固定底部区域'));
      }
      Client.createRoot(document.getElementById('root')).render(React.createElement(Fixture));
    `}))
    await probe.goto('http://localhost:5173')
    const list=probe.getByRole('region',{name:'会话列表，可滚动查看全部会话'})
    await list.waitFor()
    const heights=[]
    for(const count of [1,3,8,50]) {
      await probe.evaluate(n=>window.setCount(n),count)
      await probe.waitForFunction(n=>document.querySelector('[role=region]').children.length===n,count)
      const height=await list.evaluate(list=>{
        const current=list.parentElement
        const clone=current.cloneNode(true)
        const viewport=clone.querySelector('[role=region]')
        const rows=[...viewport.children].slice(0,3)
        rows.forEach(row=>clone.insertBefore(row,viewport))
        viewport.remove()
        clone.style.width=current.getBoundingClientRect().width+'px'
        current.parentElement.appendChild(clone)
        const result={current:current.getBoundingClientRect().height,original:clone.getBoundingClientRect().height}
        clone.remove()
        return result
      })
      if(Math.abs(height.current-height.original)>1)throw Error('展开高度变化: '+JSON.stringify({count,...height}))
      heights.push({count,...height})
    }
    const footerBefore=await probe.locator('#footer').boundingBox()
    await list.evaluate(e=>{e.scrollTop=e.scrollHeight})
    await probe.getByText('已到列表底部 · 共 50 个会话').waitFor()
    const top=await list.evaluate(e=>e.scrollTop)
    await probe.evaluate(()=>window.updateTokens())
    await probe.waitForTimeout(50)
    if(await list.evaluate(e=>e.scrollTop)!==top)throw Error('Token 更新重置滚动位置')
    const lastVisible=await list.evaluate(e=>e.lastElementChild.getBoundingClientRect().bottom<=e.getBoundingClientRect().bottom+1)
    if(!lastVisible)throw Error('无法查看最后一个会话')
    if(JSON.stringify(footerBefore)!==JSON.stringify(await probe.locator('#footer').boundingBox()))throw Error('底部随滚动移动')
    await list.locator(':scope > div').last().dblclick()
    if(await probe.evaluate(()=>window.parentDrags+window.parentDoubles)!==0)throw Error('列表操作触发拖动或收起')
    return {pass:true,heights,allSessionsReachable:true,scrollRetained:true,footerStable:true,noAccidentalDrag:true}
  } finally {await probe.close()}
}
