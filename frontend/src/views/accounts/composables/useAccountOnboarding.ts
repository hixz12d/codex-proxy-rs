import type { AccountImportItem, AccountImportTask, getAccounts } from '@/api'
import { toast } from '@codex-proxy/ui'
import { computed, ref, shallowRef, watch } from 'vue'
import { createAccountImportTask, importAccounts } from '@/api'
import { useAsyncAction } from '@/composables/useAsyncAction'
import { errorMessage, generateRequestId } from '@/utils/operation'
import { formatProviderLabel, isSupportedProvider } from '@/utils/providers'
import { accountCreateProvider, accountCreateSourceKey, accountImportSettings, accountProxyError, emptyAccountCreateForm } from '../components/AccountCreateModal/model'
import { accountImportModes } from '../components/AccountCreateModal/presenter'
import { accountImportDocuments, MAX_ACCOUNT_IMPORT_COUNT, mixedImportDocuments } from '../utils/accountImport'
import { loadRandomProxyPool, pickRandomProxy } from '../utils/randomProxy'
import { apiKeyAccountError, emptyApiKeyAccountForm } from '../utils/upstreamApiKey'
import { useAccountAuthorization } from './useAccountAuthorization'

type AccountRow = Awaited<ReturnType<typeof getAccounts>>['items'][number]

export function useAccountOnboarding(options: {
  reload: () => Promise<unknown>
  onImportTaskCreated: (task: AccountImportTask) => void
}) {
  const createModalOpen = shallowRef(false)
  const reauthorizingAccount = shallowRef<AccountRow | null>(null)
  const creatingAccountAction = useAsyncAction()
  const createForm = ref(emptyAccountCreateForm())
  const authorization = useAccountAuthorization(() => finishCreate(reauthorizingAccount.value ? '账号重新授权成功' : '账号已添加'))
  let submissionId: string | undefined
  // 随机代理按条目抽取；重试沿用同一 submissionId 时必须提交相同内容，否则后端返回 409。
  let submissionItems: AccountImportItem[] | undefined
  watch(createForm, () => {
    submissionId = undefined
    submissionItems = undefined
  }, { deep: true, flush: 'sync' })

  const showCreateModal = computed({
    get: () => createModalOpen.value,
    set: (value: boolean) => {
      createModalOpen.value = value
      if (!value)
        authorization.cancel()
    },
  })

  function clearCreate() {
    authorization.reset()
    reauthorizingAccount.value = null
    createForm.value = emptyAccountCreateForm()
  }

  function requireImportProvider(provider: string | undefined) {
    if (!provider)
      throw new Error('请选择账号平台')
    if (!isSupportedProvider(provider))
      throw new Error('账号平台不受支持')
    return provider
  }

  async function handleCreate() {
    if (createForm.value.mode === 'oauth') {
      await authorization.complete()
      return
    }
    await creatingAccountAction.run(async () => {
      const form = createForm.value
      const proxyError = accountProxyError(form)
      if (proxyError)
        throw new Error(proxyError)
      const settings = accountImportSettings(form)
      const randomPool = form.proxyMode === 'random' ? await loadRandomProxyPool() : undefined
      const outboundProxyId = () => randomPool ? pickRandomProxy(randomPool) : form.proxyMode === 'proxy' ? form.proxyId.trim() : undefined
      const mode = form.mode
      if (mode === 'oauth')
        return
      const provider = accountCreateProvider(form)
      if (mode === 'api_key') {
        if (requireImportProvider(provider) !== 'openai')
          throw new Error('当前平台不支持 API Key 账号')
        const error = apiKeyAccountError(form.apiKey)
        if (error)
          throw new Error(error)
        await importAccounts({
          provider: 'openai',
          settings,
          outboundProxyId: outboundProxyId(),
          data: { provider: 'openai', authentication_kind: 'api_key', name: form.apiKey.name.trim(), base_url: form.apiKey.base_url.trim(), api_key: form.apiKey.apiKey, transport: form.apiKey.transport },
        })
        await finishCreate('API Key 账号已添加')
        return
      }
      const documents = form.source?.kind === 'bundle'
        ? mixedImportDocuments(form.importTexts.json)
        : accountImportDocuments(requireImportProvider(provider), mode, form.importTexts[mode])
      if (documents.length > MAX_ACCOUNT_IMPORT_COUNT)
        throw new Error(`单次最多导入 ${MAX_ACCOUNT_IMPORT_COUNT} 个条目`)
      for (const document of documents)
        requireImportProvider(document.provider)
      submissionId ??= generateRequestId()
      submissionItems ??= documents.map(entry => ({ provider: entry.provider, data: entry.document, settings, outboundProxyId: outboundProxyId() }))
      const task = await createAccountImportTask({
        submissionId,
        items: submissionItems,
      })
      showCreateModal.value = false
      options.onImportTaskCreated(task)
      toast.success('导入任务已创建')
    })
  }

  async function handleAuthorizeOAuth() {
    if (authorization.busy.value)
      return
    try {
      const form = createForm.value
      const provider = accountCreateProvider(form)
      if (!isSupportedProvider(provider))
        throw new Error('账号平台不受支持')
      const account = reauthorizingAccount.value
      const proxyError = accountProxyError(form)
      if (proxyError)
        throw new Error(proxyError)
      const startProxyId = account
        ? undefined
        : form.proxyMode === 'random'
          ? pickRandomProxy(await loadRandomProxyPool())
          : form.proxyMode === 'proxy' ? form.proxyId.trim() : undefined
      await authorization.start({
        start: {
          provider,
          name: account?.name || account?.email || `${formatProviderLabel(provider)} 账号`,
          accountId: account?.id,
          outboundProxyId: startProxyId,
        },
        settings: account ? undefined : accountImportSettings(form),
      })
    }
    catch (cause) {
      toast.error(errorMessage(cause, '启动授权失败'))
    }
  }

  function openCreateAccount() {
    authorization.reset()
    reauthorizingAccount.value = null
    createForm.value = emptyAccountCreateForm()
    showCreateModal.value = true
  }

  function openReauthorizeAccount(account: AccountRow) {
    if (isSupportedProvider(account.provider) && account.authenticationKind !== 'oauth')
      return
    authorization.reset()
    reauthorizingAccount.value = account
    createForm.value = { ...emptyAccountCreateForm(), source: { kind: 'provider', id: account.provider }, step: 'import' }
    showCreateModal.value = true
    void handleAuthorizeOAuth()
  }

  async function finishCreate(message: string) {
    showCreateModal.value = false
    toast.success(message)
    await options.reload()
  }

  watch(() => accountCreateSourceKey(createForm.value), () => {
    createForm.value = {
      ...createForm.value,
      mode: reauthorizingAccount.value ? 'oauth' : createForm.value.source?.kind === 'bundle' ? 'json' : accountImportModes(accountCreateProvider(createForm.value))[0]?.value ?? 'json',
      apiKey: emptyApiKeyAccountForm(),
      importTexts: { access_token: '', refresh_token: '', json: '' },
    }
  }, { flush: 'sync' })

  watch([
    () => accountCreateSourceKey(createForm.value),
    () => createForm.value.mode,
    () => createForm.value.step,
    () => createForm.value.proxyMode,
    () => createForm.value.proxyMode === 'proxy' ? createForm.value.proxyId.trim() : '',
  ], () => {
    authorization.reset()
  }, { flush: 'sync' })

  return {
    showCreateModal,
    reauthorizingAccount,
    creatingAccount: creatingAccountAction.loading,
    authorizingOAuth: authorization.busy,
    authorization: authorization.view,
    authorizationCallback: authorization.callback,
    createForm,
    handleCreate,
    handleAuthorizeOAuth,
    openCreateAccount,
    openReauthorizeAccount,
    clearCreate,
  }
}
