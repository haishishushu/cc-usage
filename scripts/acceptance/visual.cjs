async(page)=>{
 await page.setViewportSize({width:1440,height:1000});await page.goto('http://localhost:5173/?window=showcase')
 const results=[]
 for(const theme of ['light','dark']){
  if(theme==='dark')await page.getByRole('button',{name:'切换主题',exact:true}).click()
  const sections=page.locator('main section');await sections.first().waitFor();const count=await sections.count()
  for(let i=0;i<count;i++){const section=sections.nth(i);await section.screenshot({path:`output/acceptance/showcase-${theme}-${i+1}.png`});results.push({theme,title:await section.locator('h2').innerText()})}
  const native=await page.locator('select,input[type=date],input[type=time]').count();if(native)throw new Error('Native controls')
 }
 await page.goto('http://localhost:5173/')
 await page.getByRole('button',{name:'设置',exact:true}).click()
 await page.getByRole('button',{name:'连接管理 / 常规 / 外观 / 数据',exact:true}).click()
 await page.screenshot({path:'output/acceptance/settings-preview.png',fullPage:true})
 return{pass:true,sections:results}
}
