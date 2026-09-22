async(page)=>{
 await page.goto('http://localhost:5173/__acceptance.html');await page.locator('[data-case]').first().waitFor()
 const cases=await page.locator('[data-case]').getAttribute('data-case').catch(()=>null)
 for(const theme of ['light','dark']){await page.evaluate(theme=>document.documentElement.classList.toggle('dark',theme==='dark'),theme);const list=page.locator('[data-case]');for(let i=0;i<await list.count();i++){const item=list.nth(i),name=await item.getAttribute('data-case');await item.screenshot({path:`output/acceptance/state-${theme}-${name}.png`})}}
 return {pass:true,cases:await page.locator('[data-case]').count(),themes:2}
}
