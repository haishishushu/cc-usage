async page => {
 const browser=await page.context().browser().browserType().connectOverCDP('http://127.0.0.1:9337');
 const context=browser.contexts()[0]; context.setDefaultTimeout(10000);
 const p=context.pages().find(p=>!p.url().includes('window='));
 await p.getByRole('navigation',{name:'设置分区'}).getByRole('button',{name:'灵动岛',exact:true}).click();
 const control=p.getByRole('switch',{name:'免打扰',exact:true});
 const previous=await control.getAttribute('aria-checked');
 await control.click();
 await p.getByRole('alert').filter({hasText:'设置保存失败'}).waitFor();
 if(await control.getAttribute('aria-checked')!==previous)throw Error('UI retained an unsaved value');
 const value=await p.evaluate(async()=>(await (await import('/src/lib/api.ts')).api.getSettings()).dnd);
 if(String(value)!==previous)throw Error('Backend retained an unsaved value');
 await p.screenshot({path:'output/acceptance/settings-save-failure.png'});
 return {pass:true,checks:['real disk failure reported','UI unchanged','backend unchanged']};
}
