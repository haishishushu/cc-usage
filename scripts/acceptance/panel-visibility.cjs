async page => {
  const probe=await page.context().newPage()
  try {
    await probe.route('**/src/views/MainPanelWindow.tsx*',async route=>{
      const response=await route.fetch()
      const body=await response.text()
      const anchor='const [panelActive, setPanelActive] = useState(true);'
      if(!body.includes(anchor))throw Error('找不到面板可见性测试注入点')
      await route.fulfill({response,body:body.replace(anchor,anchor+'window.setPanelActiveForTest=setPanelActive;')})
    })
    await probe.goto('http://localhost:5173')
    const week=probe.getByRole('button',{name:'本周',exact:true})
    await week.click()
    await probe.evaluate(()=>window.setPanelActiveForTest(false))
    await probe.waitForTimeout(50)
    if(await week.count()!==1||await week.getAttribute('aria-pressed')!=='true')throw Error('隐藏窗口卸载或重置了总览')
    await probe.evaluate(()=>window.setPanelActiveForTest(true))
    if(await week.getAttribute('aria-pressed')!=='true')throw Error('恢复窗口丢失筛选')
    return {pass:true,overviewRetained:true,periodPreserved:true}
  } finally {await probe.close()}
}
