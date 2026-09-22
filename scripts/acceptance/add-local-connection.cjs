async page => {
  const probe=await page.context().newPage()
  try {
    await probe.route('**/src/lib/api.ts*',async route=>{
      const response=await route.fetch()
      await route.fulfill({response,body:(await response.text())+`
        api.dataInfo=async()=>({path:'test',size_bytes:0});api.autostartStatus=async()=>false;
        api.readLocalConnections=async(platform,kind,name)=>{window.reads.push({platform,kind,name});return new Promise((resolve,reject)=>{window.finishRead=resolve;window.failRead=()=>reject(new Error('读取失败，请检查所选配置'))})};
      `})
    })
    await probe.route('**/src/main.tsx*',route=>route.fulfill({contentType:'application/javascript',body:`
      import React from '/node_modules/.vite/deps/react.js';
      import Client from '/node_modules/.vite/deps/react-dom_client.js';
      import {SettingsView} from '/src/views/SettingsView.tsx';
      import '/src/index.css';
      window.reads=[];
      Client.createRoot(document.getElementById('root')).render(React.createElement(SettingsView,{section:'connections',islandPlatform:'claude'}));
    `}))
    for(const platform of ['claude','codex'])for(const kind of ['auth','api']) {
      await probe.goto('http://localhost:5173')
      await probe.getByRole('button',{name:'添加连接',exact:true}).click()
      const dialog=probe.getByRole('dialog',{name:'添加连接'})
      if(!await dialog.getByRole('button',{name:/Gemini/}).isDisabled())throw Error('未接入平台应禁用')
      await dialog.getByRole('button',{name:platform==='claude'?/^Claude/:/^Codex/}).click()
      await dialog.getByRole('button',{name:'下一步',exact:true}).click()
      await dialog.getByRole('button',{name:kind==='auth'?/^官方订阅/:/^API Key/}).click()
      await dialog.getByRole('button',{name:'下一步',exact:true}).click()
      // 第 3 步只允许连接名称一个输入框，不接受手动凭证
      if(await dialog.locator('input,textarea,select').count()!==1)throw Error('名称外不应有其他表单')
      const customName='验收-'+platform+'-'+kind
      await dialog.getByLabel('连接名称（选填）',{exact:true}).fill(customName)
      await dialog.getByRole('button',{name:'读取并添加',exact:true}).click()
      await probe.waitForFunction(()=>window.reads.length===1)
      if(JSON.stringify(await probe.evaluate(()=>window.reads[0]))!==JSON.stringify({platform,kind,name:customName}))throw Error('读取范围或名称错误')
      if(!await dialog.getByRole('button',{name:'正在读取…',exact:true}).isDisabled())throw Error('读取期间可重复提交')
      await probe.evaluate(()=>window.failRead())
      await dialog.getByRole('alert').waitFor()
      await dialog.getByRole('button',{name:'读取并添加',exact:true}).click()
      await probe.waitForFunction(()=>window.reads.length===2)
      await probe.evaluate(()=>window.finishRead({added:0,existing:0}))
      await dialog.getByRole('alert').filter({hasText:'未找到'}).waitFor()
      await dialog.getByRole('button',{name:'读取并添加',exact:true}).click()
      await probe.waitForFunction(()=>window.reads.length===3)
      await probe.evaluate(k=>window.finishRead(k==='auth'?{added:1,existing:0}:{added:0,existing:1}),kind)
      await dialog.getByRole('status').waitFor()
      await dialog.getByRole('button',{name:'完成',exact:true}).click()
      await dialog.waitFor({state:'detached'})
    }
    return {pass:true,selectionCombinations:4,nameOnlyForm:true,customNamePassthrough:true,scopedRead:true,retry:true,emptyState:true,dedup:true}
  } finally {await probe.close()}
}
