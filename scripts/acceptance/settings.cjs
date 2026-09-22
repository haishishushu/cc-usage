async page => {
  const desktop = await page.context().browser().browserType().connectOverCDP('http://127.0.0.1:9337');
  const context = desktop.contexts()[0]; context.setDefaultTimeout(12000);
  const island = context.pages().find(p => p.url().includes('window=island'));
  const call = (method,...args) => island.evaluate(async ({method,args}) => Promise.race([(await import('/src/lib/api.ts')).api[method](...args),new Promise((_,reject)=>window.setTimeout(()=>reject(Error(`${method} timed out`)),12000))]),{method,args});
  if (!(await call('dataInfo')).path.replaceAll('\\','/').includes('/output/acceptance/profile/')) throw Error('Refusing real profile');
  await call('setDnd',false); await call('setDockEnabled',true); await call('islandTopmost',true);
  const existing = context.pages().find(p => !p.url().includes('window='));
  const created = existing ? Promise.resolve(existing) : context.waitForEvent('page');
  await call('openMainPanel');
  const panel = await created;
  await panel.getByRole('button',{name:'设置',exact:true}).click();
  const nav = panel.getByRole('navigation',{name:'设置分区'});
  if (await nav.getByRole('button').count() !== 5) throw Error('Missing sections');
  const choose = async (label,option) => {
    await panel.getByRole('button',{name:label,exact:true}).click();
    await panel.getByRole('menuitemradio',{name:option,exact:true}).click();
    await panel.locator('fieldset[aria-busy="false"]').waitFor();
  };
  const toggle = async label => {
    await panel.getByRole('switch',{name:label,exact:true}).click();
    await panel.locator('fieldset[aria-busy="false"]').waitFor();
  };
  await nav.getByRole('button',{name:'灵动岛',exact:true}).click();
  await toggle('灵动岛置顶'); await toggle('贴边停靠'); await toggle('免打扰');
  await choose('透明度','70%'); await choose('大小','大号 · 115%');
  await nav.getByRole('button',{name:'常规',exact:true}).click();
  await choose('自动刷新间隔','15 分钟');
  await panel.getByLabel('余额提醒币种',{exact:true}).fill('CNY');
  await panel.getByLabel('余额提醒阈值',{exact:true}).fill('12.5');
  await panel.getByRole('button',{name:'保存',exact:true}).click();
  await panel.getByText('余额提醒已保存',{exact:true}).waitFor();
  await nav.getByRole('button',{name:'外观',exact:true}).click();
  await panel.getByRole('button',{name:/^深色/}).click();
  await panel.locator('html.dark').waitFor();
  await island.locator('html.dark').waitFor();
  await nav.getByRole('button',{name:'数据',exact:true}).click();
  await choose('历史保留时间','保留 90 天');
  await call('setSilentStartup',false);
  const expected = {always_on_top:false,dock_enabled:false,dnd:true,island_opacity:70,island_scale:115,refresh_minutes:15,theme:'dark',retention_days:90,balance_alert_threshold:12.5,balance_alert_currency:'CNY',silent_startup:false};
  const settings = await call('getSettings');
  for (const [k,v] of Object.entries(expected)) if (settings[k] !== v) throw Error(`${k}: ${settings[k]} != ${v}`);
  await call('menuAction','dnd');
  await panel.getByRole('switch',{name:'免打扰',exact:true}).and(panel.locator('[aria-checked="false"]')).waitFor();
  await call('setDnd',true);
  let rejected=0;
  for (const [method,args] of [['setTheme',['invalid']],['setIslandPlatform',['invalid']],['setIslandSource',['local:invalid']],['setDisplayPreferences',[59,100,5]],['setRetentionDays',[1]],['setBalanceAlert',[-1,'USD']]]) {
    try { await call(method,...args) } catch { rejected++ }
  }
  if (rejected !== 6) throw Error('Invalid setting accepted');
  await panel.screenshot({path:'output/acceptance/settings-complete-dark.png'});
  await panel.reload();
  await panel.getByRole('button',{name:'设置',exact:true}).click();
  await panel.locator('fieldset[aria-busy="false"]').waitFor();
  if (await panel.getByRole('switch',{name:'免打扰',exact:true}).getAttribute('aria-checked') !== 'true') throw Error('Reload lost settings');
  return {pass:true,expected,checks:['five sections','window controls','opacity/scale/refresh','balance alert','theme cross-window','retention','tray sync','invalid input rejection','page reload']};
}
