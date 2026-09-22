async page => {
  const desktop = await page.context().browser().browserType().connectOverCDP('http://127.0.0.1:9337');
  const context = desktop.contexts()[0];
  const island = context.pages().find(p => p.url().includes('window=island'));
  const call = (method,...args) => island.evaluate(async ({method,args}) => (await import('/src/lib/api.ts')).api[method](...args),{method,args});
  if (!(await call('dataInfo')).path.replaceAll('\\','/').includes('/output/acceptance/profile/')) throw Error('Refusing real profile');
  const id = await call('addConnection',{platform:'claude',kind:'api',name:'Cost refresh fixture',secret:'test-valid',base_url:'http://127.0.0.1:18765'});
  await call('setIslandConnection',id);
  const insert = async key => {
    const now=Date.now();
    await call('importData',JSON.stringify({schema_version:1,exported_at_ms:now,platform:null,requests:[{platform:'claude',source:'local',dedup_key:key,session_id:'cost-refresh',ts:now,model:'claude-sonnet-4',input_tokens:1000000,output_tokens:0,cache_read_tokens:0,cache_write_tokens:0,total_tokens:1000000}],sessions:[]}));
  };
  await insert('cost-first');
  await island.getByText('≈ $3.00',{exact:true}).waitFor({timeout:12000});
  await insert('cost-second');
  await island.getByText('≈ $6.00',{exact:true}).waitFor({timeout:12000});
  await call('setConnectionPaused',id,true);
  await island.getByText('连接已断开，当前仅显示本机统计',{exact:true}).waitFor({timeout:12000});
  if ((await call('getSettings')).island_connection_id!==id) throw Error('Pause lost selection');
  console.log(JSON.stringify({costRefresh:'3.00 → 6.00 without remount',pause:'preserves selection',profile:'isolated'}));
}
