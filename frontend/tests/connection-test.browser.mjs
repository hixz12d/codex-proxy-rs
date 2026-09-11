/* eslint-disable antfu/no-top-level-await -- Standalone browser verification script. */
import assert from 'node:assert/strict'
import { mkdtemp } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import process from 'node:process'

const require = createRequire(import.meta.url)
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright')
const base = process.env.TEST_BASE_URL || 'http://127.0.0.1:5186'
const screenshots = await mkdtemp(join(tmpdir(), 'cpr-question-'))
const browser = await chromium.launch({ headless: true })
const context = await browser.newContext({ viewport: { width: 1440, height: 1100 }, reducedMotion: 'reduce' })
const page = await context.newPage()
const errors = []
page.on('pageerror', error => errors.push(error.message))
const sent = []
let responseMode = 'success'
let releasePending
let releaseModels
let holdModels = false

const harness = `<!doctype html><html><head><meta name="viewport" content="width=device-width, initial-scale=1"></head><body><div id="app"></div><script type="module">
import {createApp,h,reactive} from '/node_modules/.vite/deps/vue.js';
import {pinia} from '/src/stores/index.ts';
import {useThemeStore} from '/src/stores/modules/theme.ts';
import Modal from '/src/views/accounts/components/AccountConnectionTestModal.vue';
import {useAccountConnectionTest} from '/src/views/accounts/composables/useAccountConnectionTest.ts';
import '/src/styles/index.css';
const account = id => ({id,provider:'openai',email:id+'@example.test',planType:'plus',status:'normal'});
const app=createApp({setup(){
 const t=reactive(useAccountConnectionTest({reload:async()=>{}}));
 window.testState=t;window.openAccount=id=>t.openConnectionTest(account(id));
 t.openConnectionTest(account('acct_demo'));
 return ()=>h(Modal,{
 modelValue:t.showConnectionTestModal,'onUpdate:modelValue':v=>t.showConnectionTestModal=v,
 selectedModel:t.connectionTestSelectedModel,'onUpdate:selectedModel':v=>t.connectionTestSelectedModel=v,
 mode:t.connectionTestMode,'onUpdate:mode':v=>t.connectionTestMode=v,
 question:t.connectionTestQuestion,'onUpdate:question':v=>t.connectionTestQuestion=v,
 reasoningEffort:t.connectionTestReasoningEffort,'onUpdate:reasoningEffort':v=>t.connectionTestReasoningEffort=v,
 account:t.testingAccount,status:t.connectionTestStatus,model:t.connectionTestModel,content:t.connectionTestContent,
 logs:t.connectionTestLogs,error:t.connectionTestError,startedAt:t.connectionTestStartedAt,finishedAt:t.connectionTestFinishedAt,
 durationMs:t.connectionTestDurationMs,loadingModels:t.loadingConnectionTestModels,refreshingModels:t.refreshingConnectionTestModels,
 modelOptions:t.connectionTestModelOptions,statusView:t.connectionTestStatusView,
 onTest:()=>t.handleTestConnection(),onStop:()=>t.abortConnectionTest(),onRefreshModels:()=>t.handleRefreshConnectionTestModels()
 });
}});app.use(pinia);window.theme=useThemeStore(pinia);window.theme.initializeTheme();app.mount('#app');
</script></body></html>`

await page.route('**/__connection_test__', route => route.fulfill({ contentType: 'text/html', body: harness }))
await page.route('**/dev/api/**', async (route) => {
  const url = new URL(route.request().url())
  if (url.pathname.endsWith('/models') || url.pathname.endsWith('/models/refresh')) {
    const id = url.searchParams.get('accountId') || route.request().postDataJSON()?.accountId
    if (holdModels && id === 'acct_slow')
      await new Promise(resolve => releaseModels = resolve)
    await route.fulfill({ json: { code: 0, message: 'OK', data: { models: [{ id: id === 'acct_slow' ? 'stale-model' : 'gpt-5.4', label: 'GPT-5.4' }] } } })
    return
  }
  if (url.pathname.endsWith('/connection-test')) {
    assert.equal(route.request().method(), 'POST')
    assert.equal(url.search, '')
    const body = route.request().postDataJSON()
    sent.push(body)
    const mode = responseMode
    if (mode === 'pending')
      await new Promise(resolve => releasePending = resolve)
    const events = [
      { type: 'test_start', model: body.modelId },
      { type: 'request', payload: { input: [{ content: [{ type: 'input_text', text: body.inputText ?? 'Reply with exactly OK.' }] }], reasoning: body.reasoningEffort ? { effort: body.reasoningEffort } : null } },
    ]
    if (mode === 'failure') {
      events.push({ type: 'error', source: 'upstream', gatewayErrorCode: 'invalid_request', error: 'Unsupported reasoning effort', upstreamStatus: 400 })
    }
    else if (mode !== 'disconnect') {
      events.push({ type: 'content', text: mode === 'pending' ? 'STALE ANSWER' : `这是人工问答的测试回答。\n<script>window.injection = true</script>\n${'long-token-'.repeat(40)}` })
      events.push({ type: 'test_complete', success: true })
    }
    await route.fulfill({ contentType: 'text/event-stream', body: events.map(event => `data: ${JSON.stringify(event)}\n\n`).join('') }).catch(() => {})
    return
  }
  await route.fulfill({ json: { code: 0, data: {}, message: 'OK' } })
})

async function status(expected) {
  await page.waitForFunction(value => window.testState?.connectionTestStatus === value, expected)
}
async function select(name, option) {
  await page.getByRole('combobox', { name, exact: true }).click()
  await page.getByRole('option', { name: option, exact: true }).click()
}
async function layoutCheck() {
  const result = await page.evaluate(() => {
    const dialog = document.querySelector('[role="dialog"]')
    const rect = dialog.getBoundingClientRect()
    const answer = document.querySelector('section[aria-label="测试结果"] pre')
    return {
      width: innerWidth,
      left: rect.left,
      right: rect.right,
      overflow: answer ? answer.scrollWidth > answer.clientWidth + 2 : false,
      horizontal: document.documentElement.scrollWidth > innerWidth + 2,
    }
  })
  assert.ok(result.left >= 0 && result.right <= result.width + 1, JSON.stringify(result))
  assert.equal(result.overflow, false)
  assert.equal(result.horizontal, false)
}

try {
  await page.goto(`${base}/__connection_test__`)
  await page.getByRole('combobox', { name: '测试模型', exact: true }).waitFor()
  await page.waitForFunction(() => window.testState?.connectionTestSelectedModel === 'gpt-5.4')
  await select('测试模式', '人工问答')
  assert.equal(await page.getByRole('button', { name: '开始测试', exact: true }).isDisabled(), true)
  const question = 'don\'t search the internet, who is Thibault Sottiaux on X\n请用中文回答。'
  await page.getByRole('textbox', { name: '检测问题' }).fill(question)
  await select('思考强度', '高 (high)')
  await page.getByRole('button', { name: '开始测试', exact: true }).click()
  await status('success')
  assert.deepEqual(sent[0], { accountId: 'acct_demo', modelId: 'gpt-5.4', inputText: question, reasoningEffort: 'high' })
  assert.equal(await page.evaluate(() => window.injection), undefined)
  await layoutCheck()
  await page.screenshot({ path: join(screenshots, 'desktop.png'), fullPage: true })
  await page.emulateMedia({ colorScheme: 'dark' })
  await page.waitForTimeout(100)
  await page.screenshot({ path: join(screenshots, 'desktop-dark.png'), fullPage: true })
  await page.emulateMedia({ colorScheme: 'light' })

  await page.setViewportSize({ width: 390, height: 844 })
  await layoutCheck()
  await page.screenshot({ path: join(screenshots, 'mobile.png'), fullPage: true })
  await page.setViewportSize({ width: 320, height: 640 })
  await layoutCheck()
  await page.getByRole('button', { name: '重新测试', exact: true }).scrollIntoViewIfNeeded()
  await page.screenshot({ path: join(screenshots, 'small-mobile.png'), fullPage: true })
  await page.setViewportSize({ width: 1440, height: 1100 })

  responseMode = 'pending'
  await page.getByRole('button', { name: '重新测试', exact: true }).click()
  await page.waitForFunction(() => window.testState.connectionTestStatus === 'running')
  assert.equal(await page.getByRole('combobox', { name: '思考强度', exact: true }).isDisabled(), true)
  await page.getByRole('button', { name: '停止测试', exact: true }).click()
  await status('cancelled')
  responseMode = 'success'
  await page.getByRole('button', { name: '重新测试', exact: true }).click()
  await status('success')
  releasePending?.()
  await page.waitForTimeout(100)
  assert.ok(!(await page.evaluate(() => window.testState.connectionTestContent)).includes('STALE'))

  responseMode = 'failure'
  await page.getByRole('button', { name: '重新测试', exact: true }).click()
  await status('error')
  assert.ok((await page.getByRole('alert').textContent()).includes('测试请求不合法'))
  await page.getByText('事件轨迹', { exact: true }).click()
  assert.ok(await page.getByText('Unsupported reasoning effort', { exact: false }).isVisible())

  responseMode = 'disconnect'
  await page.getByRole('button', { name: '重新测试', exact: true }).click()
  await status('error')
  assert.ok((await page.getByRole('alert').textContent()).includes('未收到完成事件'))

  holdModels = true
  await page.evaluate(() => window.openAccount('acct_slow'))
  await page.waitForTimeout(100)
  await page.evaluate(() => window.openAccount('acct_fast'))
  await page.waitForFunction(() => window.testState.connectionTestSelectedModel === 'gpt-5.4')
  releaseModels?.()
  await page.waitForTimeout(100)
  assert.equal(await page.evaluate(() => window.testState.connectionTestSelectedModel), 'gpt-5.4')

  responseMode = 'success'
  await select('测试模式', '连通测试')
  await page.getByRole('button', { name: '开始测试', exact: true }).click()
  await status('success')
  assert.deepEqual(sent.at(-1), { accountId: 'acct_fast', modelId: 'gpt-5.4' })

  responseMode = 'pending'
  await page.getByRole('button', { name: '重新测试', exact: true }).click()
  await status('running')
  await page.getByRole('button', { name: '关闭', exact: true }).last().click()
  await status('cancelled')
  releasePending?.()
  assert.deepEqual(errors, [])
  process.stdout.write(`${JSON.stringify({ passed: true, requests: sent.length, screenshots })}\n`)
}
finally {
  releasePending?.()
  releaseModels?.()
  await browser.close()
}
