async(page)=>{
 await page.setViewportSize({width:1440,height:1000});await page.goto('http://localhost:5173/')
 const results=[]
 for(const theme of ['light','dark']){
  await page.evaluate(theme=>document.documentElement.classList.toggle('dark',theme==='dark'),theme)
  await page.getByRole('button',{name:'总览',exact:true}).click()
  const apiButton=page.getByRole('button',{name:'API 连接',exact:true});if(await apiButton.count())await apiButton.click()
  await page.screenshot({path:`output/acceptance/panel-auth-${theme}.png`,fullPage:true})
  await page.getByRole('button',{name:'官方账号',exact:true}).click()
  await page.getByRole('heading',{name:'今日 API 用量',exact:true}).waitFor()
  await page.screenshot({path:`output/acceptance/panel-api-${theme}.png`,fullPage:true})
  await page.getByRole('button',{name:'自定义时间',exact:true}).click()
  await page.getByRole('button',{name:'确定',exact:true}).waitFor()
  await page.screenshot({path:`output/acceptance/date-${theme}.png`})
  await page.getByRole('button',{name:'取消',exact:true}).click()
  await page.getByRole('button',{name:'设置',exact:true}).click()
  await page.getByRole('button',{name:'灵动岛',exact:true}).last().click()
  await page.screenshot({path:`output/acceptance/island-settings-${theme}.png`,fullPage:true})
  await page.getByRole('navigation',{name:'设置分区'}).getByRole('button',{name:'常规',exact:true}).click()
  await page.screenshot({path:`output/acceptance/general-settings-${theme}.png`,fullPage:true})
  results.push(theme)
 }
 return{pass:true,themes:results,pages:['Auth overview','API overview','date dialog','island settings','general settings']}
}
