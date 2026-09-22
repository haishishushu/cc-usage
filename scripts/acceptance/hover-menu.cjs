async page => {
  const probe=await page.context().newPage()
  const errors=[]
  probe.on('pageerror',e=>errors.push(e.message))
  try {
    await probe.setViewportSize({width:586,height:620})
    await probe.addInitScript(()=>{window.__TAURI_INTERNALS__={};window.commands=[];window.fits=0;window.shown=0})
    await probe.route('**/src/lib/api.ts*',async route=>{
      const response=await route.fetch()
      await route.fulfill({response,body:(await response.text())+`
        const testSettings={silent_startup:true,island_platform:'claude',island_kind:'api',island_connection_id:'one',always_on_top:true,theme:'light',island_opacity:100,island_scale:100,refresh_minutes:5,dock_enabled:true,dock:{edge:null},dnd:false,island_visible:true};
        listenEvent=async()=>()=>{};
        api.getSettings=async()=>testSettings;
        api.listConnections=async()=>[{id:'one',platform:'claude',kind:'api',name:'当前连接',status:'connected'},{id:'two',platform:'codex',kind:'api',name:'备用连接',status:'connected'},{id:'three',platform:'claude',kind:'auth',name:'失效连接',status:'expired'}];
        api.menuRefreshing=async()=>false;
        api.menuFit=async height=>{window.fits++;return {root_x:location.search.includes('left')?312:6,root_y:Math.max(6,614-height),root_width:268,sub_width:300,side:location.search.includes('left')?'left':'right'}};
        api.menuShow=async()=>{window.shown++};
        api.menuClose=async()=>{};
        api.menuAction=async id=>{window.commands.push(id)};
        api.setIslandConnection=async id=>{window.commands.push(id);return testSettings};
      `})
    })
    await probe.route('**/src/main.tsx*',route=>route.fulfill({contentType:'application/javascript',body:`
      import React from '/node_modules/.vite/deps/react.js';
      import Client from '/node_modules/.vite/deps/react-dom_client.js';
      import {ContextMenuWindow} from '/src/views/ContextMenuWindow.tsx';
      import '/src/index.css';
      Client.createRoot(document.getElementById('root')).render(React.createElement(ContextMenuWindow));
    `}))
    for(const source of ['island','tray'])for(const side of ['left','right']) {
      await probe.goto('http://localhost:5173/?window=menu&source='+source+'&side='+side)
      await probe.waitForFunction(()=>window.shown>0)
      const root=probe.getByRole('menu',{name:source==='island'?'灵动岛菜单':'托盘菜单',exact:true})
      const initial=await root.boundingBox()
      const fits=await probe.evaluate(()=>window.fits)
      const trigger=probe.getByRole('menuitem',{name:'切换连接',exact:true})
      await trigger.hover()
      const sub=probe.getByRole('menu',{name:'切换连接',exact:true})
      await sub.waitFor()
      if(await trigger.getAttribute('aria-expanded')!=='true')throw Error('未展开')
      const box=await sub.boundingBox()
      if(box.x<0||box.y<0||box.x+box.width>586||box.y+box.height>620)throw Error('子菜单越界')
      if(side==='left'?box.x+box.width>initial.x:box.x<initial.x+initial.width)throw Error('子菜单没有侧边展开')
      const current=sub.getByRole('menuitemradio',{name:/当前连接/})
      if(await current.getAttribute('aria-checked')!=='true')throw Error('当前连接标记丢失')
      if(!await sub.getByRole('menuitemradio',{name:/失效连接/}).isDisabled())throw Error('失效连接可点击')
      // 从触发项穿过间隙进入侧边，停留超过关闭延迟后仍可操作。
      const target=await current.boundingBox()
      await probe.mouse.move(target.x+target.width/2,target.y+target.height/2,{steps:12})
      await probe.waitForTimeout(250)
      if(!await sub.isVisible())throw Error('跨菜单移动时意外关闭')
      if(JSON.stringify(initial)!==JSON.stringify(await root.boundingBox()))throw Error('主菜单跳动')
      if(await probe.evaluate(()=>window.fits)!==fits)throw Error('悬浮触发窗口重新布局')
      await sub.getByRole('menuitemradio',{name:/备用连接/}).click()
      if(await probe.evaluate(()=>window.commands[0])!=='two')throw Error('切换连接命令错误')
      await probe.goto('http://localhost:5173/?window=menu&source='+source+'&side='+side)
      await probe.waitForFunction(()=>window.shown>0)
      const position=probe.getByRole('menuitem',{name:'显示位置',exact:true})
      await position.focus()
      await probe.keyboard.press('ArrowRight')
      await probe.waitForFunction(()=>document.activeElement?.getAttribute('role')==='menuitemradio')
      await probe.keyboard.press('ArrowLeft')
      if(await position.getAttribute('aria-expanded')!=='false')throw Error('键盘返回失败')
      await position.hover()
      const positionSub=probe.getByRole('menu',{name:'显示位置',exact:true})
      await positionSub.getByRole('menuitemradio',{name:'上边居中'}).click()
      if(await probe.evaluate(()=>window.commands[0])!=='pos_top')throw Error('位置命令错误')
    }
    if(errors.length)throw Error(errors.join('\n'))
    return {pass:true,sources:2,directions:2,hoverBridge:true,rootStable:true,keyboard:true,selection:true}
  } finally {await probe.close()}
}
