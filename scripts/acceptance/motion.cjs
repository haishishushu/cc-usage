async page => {
  const check = (condition, message) => { if (!condition) throw new Error(message) }
  const failures = []
  const onError = error => failures.push(error.message)
  page.on('pageerror', onError)
  const results = {}
  try {
    await page.emulateMedia({ reducedMotion: 'no-preference' })
    await page.goto('http://localhost:5173')
    await page.getByRole('button', { name: '本周', exact: true }).waitFor()
    await page.evaluate(() => document.fonts.ready)
    results.filter = await page.evaluate(async () => {
      const button = [...document.querySelectorAll('button')].find(e => e.textContent.replaceAll('本周', '') === '' && e.textContent.includes('本周'))
      const track = button.parentElement
      const indicator = track.querySelector('[data-motion-indicator]')
      if (!indicator) throw Error('缺少滑动底板')
      const siblings = [...track.querySelectorAll('button')]
      const before = siblings.map(e => e.getBoundingClientRect().x)
      const stable = document.querySelector('h2').closest('section')
      let flickers = 0, shifts = 0, frames = 0
      button.click()
      const start = performance.now()
      while (performance.now() - start < 260) {
        await new Promise(requestAnimationFrame)
        frames++
        if (siblings.some((e, i) => Math.abs(e.getBoundingClientRect().x - before[i]) > 0.5)) shifts++
        for (let e = stable; e; e = e.parentElement) {
          if (Number(getComputedStyle(e).opacity) < 0.99 || e.getAnimations().some(a => a.playState === 'running')) flickers++
        }
      }
      const a = indicator.getBoundingClientRect(), b = button.getBoundingClientRect()
      return { frames, flickers, shifts, aligned: Math.abs(a.x-b.x)<1 && Math.abs(a.width-b.width)<1 }
    })
    check(results.filter.aligned && !results.filter.flickers && !results.filter.shifts, '筛选导致无关区域闪烁、邻项位移或底板错位')
    await page.getByRole('button', { name: '切换主题', exact: true }).click()
    await page.waitForFunction(() => !document.documentElement.classList.contains('theme-transition'))
    await page.screenshot({ path: 'output/motion-review/code-panel-dark.png' })
    await page.getByRole('button', { name: '切换主题', exact: true }).click()
    await page.getByRole('button', { name: '灵动岛', exact: true }).click()
    await page.locator('.island-morph').waitFor()
    results.island = await page.evaluate(async () => {
      const frame = document.querySelector('.island-morph')
      const reserve = frame.parentElement
      const start = frame.getBoundingClientRect()
      const samples = []
      const toggle = () => frame.querySelector('.island-shell').dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
      toggle()
      let reversed = false, resumed = false
      const begin = performance.now()
      while (performance.now() - begin < 700) {
        await new Promise(requestAnimationFrame)
        const elapsed = performance.now() - begin
        if (elapsed > 90 && !reversed) { toggle(); reversed = true }
        if (elapsed > 155 && !resumed) { toggle(); resumed = true }
        const box = frame.getBoundingClientRect()
        const content = frame.firstElementChild
        const header = frame.querySelector('.island-shell').firstElementChild
        samples.push({ y: box.y, width: box.width, height: box.height, reserved: reserve.offsetHeight,
          opacity: Number(getComputedStyle(content).opacity) * Number(getComputedStyle(header).opacity) })
      }
      return { start, samples, state: reserve.dataset.islandMotion,
        final: frame.offsetHeight, target: frame.firstElementChild.offsetHeight }
    })
    check(results.island.samples.every(s => Math.abs(s.y-results.island.start.y)<1 && s.width===400 && s.height>0 && s.reserved+1>=s.height && s.opacity===1), '灵动岛顶边跳动、内容消失或预留窗口裁切')
    check(results.island.state==='expanded' && results.island.final===results.island.target, '快速反向后最终高度错误')
    results.island = { samples: results.island.samples.length, stableTop: true, readableHeader: true, rapidReverse: true }
    await page.screenshot({ path: 'output/motion-review/code-island-light.png' })

    // 隔离挂载真实控件：只使用内存状态，不读取或删除用户连接、凭证与设置。
    await page.evaluate(async () => {
      const React = (await import('/node_modules/.vite/deps/react.js')).default
      const { createRoot } = (await import('/node_modules/.vite/deps/react-dom_client.js')).default
      const { Toggle, Button } = await import('/src/components/ui/primitives.tsx')
      const { RemoveConnectionDialog } = await import('/src/components/settings/ConnectionDialogs.tsx')
      const { SelectField } = await import('/src/components/ui/SelectField.tsx')
      const { useExitPresence, useContentMotion } = await import('/src/lib/motion.ts')
      const h = React.createElement
      function Fixture() {
        const [open, setOpen] = React.useState(false)
        window.setMotionDialog = setOpen
        const [on, setOn] = React.useState(false)
        const [count, setCount] = React.useState(0)
        const [selection, setSelection] = React.useState('one')
        const presence = useExitPresence(open ? true : null)
        const motion = useContentMotion(selection)
        return h('div', { style: { padding: 40 } },
          h(Button, { onClick: () => setOpen(true) }, '测试弹窗'),
          h(Button, { disabled: true }, '禁用按钮'),
          h(Toggle, { on, onChange: setOn, label: '测试开关' }),
          h(Button, { onClick: () => setCount(c => c+1) }, '后台数据刷新'),
          h(SelectField, { label: '测试菜单', value: selection, options: [{value:'one',label:'第一项'},{value:'two',label:'第二项'}], onValueChange:setSelection }),
          h('div', { ref: motion, id: 'stable-region' }, '始终可见 '+count),
          presence.rendered && h('div', { className:'motion-overlay fixed inset-0 grid place-items-center bg-black/25', 'data-state':presence.exiting?'closed':'open', inert:presence.exiting },
            h(RemoveConnectionDialog, { connection:{id:'fixture',platformId:'claude',name:'测试连接',label:'Auth',kind:'auth',status:'connected'}, exiting:presence.exiting, onClose:()=>setOpen(false) })))
      }
      document.getElementById('root').style.display = 'none'
      const host = document.createElement('div')
      document.body.append(host)
      createRoot(host).render(h(React.StrictMode, null, h(Fixture)))
    })
    const toggle = page.getByRole('switch', { name:'测试开关' })
    await toggle.click()
    await page.waitForFunction(() => {
      const e = document.querySelector('.motion-toggle-thumb')
      return getComputedStyle(e).transform === 'matrix(1, 0, 0, 1, 14, 0)'
    })
    check(await toggle.getAttribute('aria-checked')==='true', '开关状态错误')
    results.toggle = await toggle.locator('span').evaluate(e => getComputedStyle(e).transitionDuration)
    check(results.toggle==='0.16s', '开关未使用 160ms 平移')
    await page.getByRole('button', {name:'后台数据刷新'}).click()
    check(await page.locator('#stable-region').evaluate(e => e.getAnimations().length===0 && getComputedStyle(e).opacity==='1'), '后台刷新触发无关动画')
    await page.getByRole('button', {name:'测试菜单'}).click()
    check(await page.getByRole('menu').evaluate(e => getComputedStyle(e).transitionDuration)==='0.16s, 0.16s', '菜单缺少入场')
    await page.getByRole('menuitemradio', {name:'第二项'}).click()
    await page.getByRole('menu').waitFor({state:'hidden'})
    check(await page.getByRole('button', {name:'测试菜单'}).evaluate(e=>e===document.activeElement), '菜单焦点未恢复')
    const trigger = page.getByRole('button', {name:'测试弹窗'})
    await trigger.click()
    await page.getByRole('dialog').waitFor()
    check(await page.getByRole('dialog').evaluate(e=>e.contains(document.activeElement)), '弹窗焦点未进入')
    await page.keyboard.press('Escape')
    check(await page.locator('.motion-overlay').getAttribute('data-state')==='closed', '弹窗没有退出阶段')
    check(await page.locator('.motion-overlay').getAttribute('inert')!==null, '退出期间仍可操作')
    await page.locator('.motion-overlay').waitFor({state:'detached'})
    check(await trigger.evaluate(e=>e===document.activeElement), '弹窗关闭后焦点未恢复')
    results.presence = true
    results.rapidDialog = await page.evaluate(async () => {
      const {flushSync} = (await import('/node_modules/.vite/deps/react-dom.js')).default
      const wait = ms => new Promise(resolve => setTimeout(resolve, ms))
      const sample = () => {
        const overlay = document.querySelector('.motion-overlay')
        const dialog = overlay.querySelector('.motion-dialog')
        return {opacity:Number(getComputedStyle(overlay).opacity), y:new DOMMatrix(getComputedStyle(dialog).transform).m42}
      }
      flushSync(()=>window.setMotionDialog(true))
      await wait(45)
      const beforeClose=sample()
      flushSync(()=>window.setMotionDialog(false))
      const afterClose=sample()
      await wait(20)
      const beforeReopen=sample()
      flushSync(()=>window.setMotionDialog(true))
      const afterReopen=sample()
      await wait(250)
      const mounted=Boolean(document.querySelector('.motion-overlay[data-state="open"]'))
      const focused=document.querySelector('.motion-dialog')?.contains(document.activeElement)
      flushSync(()=>window.setMotionDialog(false))
      return {beforeClose,afterClose,beforeReopen,afterReopen,mounted,focused}
    })
    const rapid=results.rapidDialog
    check(Math.abs(rapid.beforeClose.opacity-rapid.afterClose.opacity)<0.03 && Math.abs(rapid.beforeReopen.opacity-rapid.afterReopen.opacity)<0.03, '快速开关弹窗发生透明度跳变')
    check(Math.abs(rapid.beforeClose.y-rapid.afterClose.y)<0.1 && Math.abs(rapid.beforeReopen.y-rapid.afterReopen.y)<0.1 && rapid.mounted && rapid.focused, '快速重开弹窗跳位、误卸载或丢失焦点')
    await page.locator('.motion-overlay').waitFor({state:'detached'})
    await page.emulateMedia({reducedMotion:'reduce'})
    await toggle.click()
    check(await toggle.locator('span').evaluate(e=>getComputedStyle(e).transitionDuration)==='0s', '减少动态效果仍有开关过渡')
    await trigger.click()
    check(await page.getByRole('dialog').evaluate(e=>parseFloat(getComputedStyle(e).transitionDuration))===0, '减少动态效果仍有弹窗动画')
    await page.keyboard.press('Escape')
    await page.locator('.motion-overlay').waitFor({state:'detached'})
    results.reducedMotion = true
    check(failures.length===0, failures.join('\n'))
    return {pass:true, ...results}
  } finally {
    page.off('pageerror', onError)
    await page.emulateMedia({reducedMotion:'no-preference'})
    await page.goto('http://localhost:5173')
  }
}
