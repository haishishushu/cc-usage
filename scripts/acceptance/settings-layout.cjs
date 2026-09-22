async page => {
  const desktop=await page.context().browser().browserType().connectOverCDP('http://127.0.0.1:9337');
  const context=desktop.contexts()[0];
  const panel=context.pages().find(p=>!p.url().includes('window='));
  context.setDefaultTimeout(10000);
  await panel.getByRole('button',{name:'设置',exact:true}).click();
  const nav=panel.getByRole('navigation',{name:'设置分区'});
  await nav.getByRole('button',{name:'数据',exact:true}).click();
  await panel.screenshot({path:'output/acceptance/approved-settings-data.png'});
  const measures=await panel.evaluate(()=>({viewport:innerHeight,scroll:Array.from(document.querySelectorAll('[data-panel-scroll],.unified-settings,nav,#settings-data')).map(e=>({tag:e.tagName,id:e.id,y:e.getBoundingClientRect().y,h:e.getBoundingClientRect().height,scrollHeight:e.scrollHeight,scrollTop:e.scrollTop}))}));
  if(await panel.locator('input[type=file]').count())throw Error('Browser file picker remains');
  if(await panel.getByRole('button',{name:'还原',exact:true}).count())await panel.getByRole('button',{name:'还原',exact:true}).click();
  await nav.getByRole('button',{name:'连接管理',exact:true}).click();
  await panel.screenshot({path:'output/acceptance/approved-settings-default-size.png'});
  return {pass:true,measures};
}
