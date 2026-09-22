async page => {
  const probe = await page.context().newPage()
  try {
    await probe.route('**/src/lib/api.ts*', async route => {
      const response=await route.fetch()
      await route.fulfill({response,body:(await response.text())+`
        api.menuRefreshing=async()=>new URLSearchParams(location.search).has('refreshing');
        api.menuAction=async id=>{window.commands.push(id)};
        api.menuClose=async()=>{};
      `})
    })
    await probe.route('**/src/main.tsx*', route => route.fulfill({contentType:'application/javascript',body:`
      import React from '/node_modules/.vite/deps/react.js';
      import Client from '/node_modules/.vite/deps/react-dom_client.js';
      import {ContextMenuWindow} from '/src/views/ContextMenuWindow.tsx';
      import {api} from '/src/lib/api.ts';
      import '/src/index.css';
      window.commands=[];
      api.menuRefreshing=async()=>new URLSearchParams(location.search).has('refreshing');
      api.menuAction=async id=>{window.commands.push(id)};
      api.menuClose=async()=>{};
      Client.createRoot(document.getElementById('root')).render(React.createElement(ContextMenuWindow));
    `}))
    const rootLabels=['打开主面板','立即刷新','切换连接','显示位置','始终置顶']
    const open=async query=>{await probe.goto('http://localhost:5173/?window=menu&'+query);await probe.getByRole('menuitem',{name:'打开主面板',exact:true}).waitFor()}
    await open('source=island')
    const labels=await probe.getByRole('menu',{name:'灵动岛菜单',exact:true}).locator('button').allTextContents()
    if(JSON.stringify(labels)!==JSON.stringify(rootLabels))throw Error('灵动岛菜单顺序错误: '+labels)
    const topmost=probe.getByRole('menuitemcheckbox',{name:'始终置顶',exact:true})
    if(await topmost.getAttribute('aria-checked')!=='true')throw Error('置顶状态未同步')
    await topmost.click()
    if(await probe.evaluate(()=>window.commands[0])!=='topmost')throw Error('置顶命令错误')
    await open('source=island')
    await probe.getByRole('menuitem',{name:'显示位置',exact:true}).hover()
    await probe.getByRole('menuitemradio',{name:'自由悬浮'}).waitFor({timeout:1500})
    if(await probe.getByRole('menuitemradio').count()!==5)throw Error('位置选项丢失')
    if(await probe.getByRole('menuitemradio',{name:'自由悬浮'}).getAttribute('aria-checked')!=='true')throw Error('当前位置未标记')
    await probe.keyboard.press('Escape')
    await probe.getByRole('menuitem',{name:'切换连接',exact:true}).hover()
    await probe.getByText('暂无连接，请在主面板添加').waitFor()
    await probe.keyboard.press('Escape')
    await probe.getByRole('menuitem',{name:'打开主面板',exact:true}).click()
    if(await probe.evaluate(()=>window.commands[0])!=='open_main')throw Error('主面板命令错误: '+JSON.stringify(await probe.evaluate(()=>({commands:window.commands,html:document.body.innerHTML}))))
    await open('source=island')
    await probe.getByRole('menuitem',{name:'立即刷新',exact:true}).click()
    if(await probe.evaluate(()=>window.commands[0])!=='refresh')throw Error('刷新命令错误')
    await open('source=island&refreshing=1')
    if(!await probe.getByRole('menuitem',{name:'刷新中…',exact:true}).isDisabled())throw Error('刷新未锁定')
    await open('source=tray')
    await probe.getByRole('menuitem',{name:'退出',exact:true}).waitFor()
    return {pass:true,rootLabels,positionOptions:5,emptyConnections:true,commands:true,refreshLock:true,trayPreserved:true}
  } finally {await probe.close()}
}
