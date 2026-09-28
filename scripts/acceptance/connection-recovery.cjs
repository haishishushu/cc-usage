// 浏览器隔离夹具：真实 useConnections，模拟数据库启动延迟，不读取用户数据。
async page => {
  await page.route('**/src/lib/api.ts', route => route.fulfill({
    contentType: 'application/javascript', body: `
      export const isTauri = true;
      window.recovery = { requests: [], listener: null };
      export const api = { listConnections: () => new Promise((resolve, reject) => {
        if (!window.recovery.listener) throw Error('read before subscription');
        window.recovery.requests.push({resolve, reject});
      }) };
      export const listenEvent = async (_name, callback) => {
        window.recovery.listener = callback;
        return () => { window.recovery.listener = null; };
      };
    `,
  }));
  await page.route('**/__connection-recovery.html', route => route.fulfill({
    contentType: 'text/html', body: `<div id="root"></div><script type="module">
      import React from '/node_modules/.vite/deps/react.js';
      import ReactDOM from '/node_modules/.vite/deps/react-dom_client.js';
      import { useConnections } from '/src/lib/useConnections.ts';
      function Probe() {
        const state = useConnections();
        return React.createElement('pre', {}, JSON.stringify({
          ids: state.connections.map(c => c.id), live: state.live,
          loading: state.loading, error: state.error
        }));
      }
      ReactDOM.createRoot(document.getElementById('root')).render(React.createElement(Probe));
    </script>`,
  }));
  await page.goto('http://localhost:5173/__connection-recovery.html');
  await page.waitForFunction(() => window.recovery?.requests.length === 1);
  await page.evaluate(() => window.recovery.requests[0].reject(Error('database not ready')));
  await page.waitForFunction(() => document.querySelector('pre').textContent.includes('database not ready'));
  await page.evaluate(() => window.recovery.listener());
  await page.waitForFunction(() => window.recovery.requests.length === 2);
  const connection = {id:'saved-account', platform:'codex', kind:'auth', name:'Saved account', status:'connected'};
  await page.evaluate(c => window.recovery.requests[1].resolve([c]), connection);
  await page.waitForFunction(() => JSON.parse(document.querySelector('pre').textContent).live);
  await page.evaluate(() => { window.recovery.listener(); window.recovery.listener(); });
  await page.waitForFunction(() => window.recovery.requests.length === 4);
  await page.evaluate(c => window.recovery.requests[3].resolve([c]), connection);
  await page.waitForFunction(() => !JSON.parse(document.querySelector('pre').textContent).loading);
  await page.evaluate(() => window.recovery.requests[2].reject(Error('stale startup failure')));
  const state = JSON.parse(await page.locator('pre').innerText());
  if (!state.live || state.error || state.loading || state.ids.join() !== 'saved-account') throw Error(JSON.stringify(state));
  return {pass:true, checks:['subscribe before reading', 'restore saved connection after database readiness', 'ignore stale startup response']};
}
