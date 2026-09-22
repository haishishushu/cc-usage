async(page)=>{
 const b=await page.context().browser().browserType().connectOverCDP('http://127.0.0.1:9337');const island=b.contexts()[0].pages().find(p=>p.url().includes('window=island'))
 return island.evaluate(async()=>{const{api}=await import('/src/lib/api.ts');if(!(await api.dataInfo()).path.replaceAll('\\','/').includes('/output/acceptance/profile/'))throw new Error('Not isolated');const before=(await api.listConnections()).length;const cases=[];
 for(const key of ['test-unauthorized','test-forbidden','test-limited','test-server-error','test-invalid','test-malformed','test-missing']){let rejected=false;try{await api.addConnection({platform:'claude',kind:'api',name:'HTTP fixture',secret:key,base_url:'http://127.0.0.1:18765'})}catch{rejected=true}if(!rejected)throw new Error(key+' incorrectly accepted');cases.push(key)}
 if((await api.listConnections()).length!==before)throw new Error('Failed request changed database');return{pass:true,rejected:cases}})
}
