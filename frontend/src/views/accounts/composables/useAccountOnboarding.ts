import type { Account, AccountImportParam, AccountImportTask } from '@/api'
import type { EmailAuthState } from '@/api/modules/email-auth'
import { computed, ref, shallowRef, watch } from 'vue'
import { createAccountImportTask, importAccounts } from '@/api'
import { startEmailAuth } from '@/api/modules/email-auth'
import { ApiError } from '@/api/request'
import { useAsyncAction } from '@/composables/useAsyncAction'
import { t } from '@/i18n'
import { notification as ZNotification } from '@/utils/notification'
import { errorMessage, generateRequestId } from '@/utils/operation'
import { isSupportedProvider } from '@/utils/providers'
import { accountCreateProvider, accountCreateSourceKey, accountImportModes, accountImportSettings, emptyAccountCreateForm, parseEmailAuthInput } from '../utils/accountCreate'
import { accountImportBatches, accountImportDocuments, mixedImportDocuments } from '../utils/accountImport'
import { loadRandomProxyPool, pickRandomProxy } from '../utils/randomProxy'
import { emptyApiKeyAccountForm } from '../utils/upstreamApiKey'
import { useAccountAuthorization } from './useAccountAuthorization'

export function useAccountOnboarding(options: {
  reload: () => Promise<unknown>
  onImportTaskCreated: (task: AccountImportTask, reveal?: boolean) => void
  onEmailAuthCreated: (state: EmailAuthState) => void
  onEmailAuthRefresh?: () => void
}) {
  const createModalOpen = shallowRef(false)
  const reauthorizingAccount = shallowRef<Account | null>(null)
  const creatingAccountAction = useAsyncAction()
  const createForm = ref(emptyAccountCreateForm())
  const emailAuthSubmitError = shallowRef('')
  const authorization = useAccountAuthorization(() => finishCreate(reauthorizingAccount.value
    ? t('accounts.onboarding.reauthorized')
    : t('accounts.onboarding.accountAdded')))
  let submissionIds: string[] = []
  // 随机代理按条目抽取；重试沿用同一批标识时必须提交相同内容，否则后端返回 409。
  let submissionBatches: AccountImportParam[][] | undefined
  watch(createForm, () => {
    submissionIds = []
    submissionBatches = undefined
  }, { deep: true, flush: 'sync' })
  watch(
    () => [createForm.value.mode, createForm.value.importTexts.email],
    () => {
      emailAuthSubmitError.value = ''
    },
    { flush: 'sync' },
  )
  // 只在切到邮箱模式时刷新一次配置与批次状态，输入邮箱时不逐键请求
  watch(
    () => createForm.value.mode,
    (mode) => {
      if (mode === 'email' && createModalOpen.value)
        options.onEmailAuthRefresh?.()
    },
  )

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
    emailAuthSubmitError.value = ''
    createForm.value = emptyAccountCreateForm()
  }

  function requireImportProvider(provider: string | undefined) {
    if (!provider)
      throw new Error(t('accounts.onboarding.providerRequired'))
    if (!isSupportedProvider(provider))
      throw new Error(t('accounts.onboarding.accountPlatformIsNotSupported'))
    return provider
  }

  async function handleCreate() {
    if (createForm.value.mode === 'oauth') {
      await authorization.complete()
      return
    }
    if (createForm.value.mode === 'email') {
      await creatingAccountAction.run(async () => {
        const form = createForm.value
        const provider = accountCreateProvider(form)
        if (requireImportProvider(provider) !== 'openai')
          throw new Error(t('accounts.onboarding.accountPlatformIsNotSupported'))
        const parsed = parseEmailAuthInput(form.importTexts.email)
        if (!parsed.emails.length || parsed.invalidEmails.length > 0 || parsed.tooMany)
          return
        emailAuthSubmitError.value = ''
        const settings = accountImportSettings(form)
        const result = await startEmailAuth({
          emails: parsed.emails,
          settings,
        }, { silent: true })
        showCreateModal.value = false
        options.onEmailAuthCreated(result)
      }, {
        errorText: false,
        onError: (cause) => {
          emailAuthSubmitError.value = cause instanceof ApiError
            ? cause.message
            : errorMessage(cause)
          options.onEmailAuthRefresh?.()
        },
      })
      return
    }
    let submitted = 0
    await creatingAccountAction.run(async () => {
      const form = createForm.value
      const settings = accountImportSettings(form)
      const randomPool = form.proxyMode === 'random'
        ? await loadRandomProxyPool()
        : undefined
      const outboundProxyId = () => randomPool
        ? pickRandomProxy(randomPool)
        : form.proxyMode === 'proxy'
          ? form.proxyId.trim()
          : undefined
      const mode = form.mode
      if (mode === 'oauth')
        return
      const provider = accountCreateProvider(form)
      if (mode === 'api_key') {
        if (requireImportProvider(provider) !== 'openai')
          throw new Error(t('accounts.onboarding.apiKeyUnsupported'))
        await importAccounts({
          provider: 'openai',
          settings,
          outboundProxyId: outboundProxyId(),
          data: {
            provider: 'openai',
            authentication_kind: 'api_key',
            name: form.name.trim(),
            base_url: form.apiKey.base_url.trim(),
            api_key: form.apiKey.apiKey,
            protocol: form.apiKey.protocol,
            transport: form.apiKey.transport,
          },
        })
        await finishCreate(t('accounts.onboarding.apiKeyAccountAdded'))
        return
      }
      const documents = form.source?.kind === 'bundle'
        ? mixedImportDocuments(form.importTexts.json)
        : accountImportDocuments(requireImportProvider(provider), mode, form.importTexts[mode])
      for (const document of documents)
        requireImportProvider(document.provider)
      submissionBatches ??= accountImportBatches(documents.map(entry => ({
        provider: entry.provider,
        data: entry.document,
        settings: documents.length === 1
          ? settings
          : { ...settings, name: undefined },
        outboundProxyId: outboundProxyId(),
      })))
      const batches = submissionBatches
      // 同一输入重试时复用每批标识，恢复丢失响应或部分提交，不重复执行 RT 交换
      const ids = submissionIds
      for (const [index, items] of batches.entries()) {
        const task = await createAccountImportTask({ submissionId: ids[index] ??= generateRequestId(), items })
        submitted += 1
        const finished = index === batches.length - 1
        if (finished)
          showCreateModal.value = false
        options.onImportTaskCreated(task, finished)
      }
      ZNotification.success({
        message: submitted > 1
          ? t('accounts.onboarding.importTasksCreated', { submitted })
          : t('accounts.onboarding.importTaskCreated'),
      })
    }, {
      onError: () => {
        if (submitted) {
          ZNotification.warning({
            message: t('accounts.onboarding.partialSubmission'),
          })
        }
      },
    })
  }

  async function handleAuthorizeOAuth() {
    if (authorization.busy.value)
      return
    try {
      const form = createForm.value
      const provider = accountCreateProvider(form)
      if (!isSupportedProvider(provider))
        throw new Error(t('accounts.onboarding.accountPlatformIsNotSupported'))
      const account = reauthorizingAccount.value
      const startProxyId = account
        ? undefined
        : form.proxyMode === 'random'
          ? pickRandomProxy(await loadRandomProxyPool())
          : form.proxyMode === 'proxy'
            ? form.proxyId.trim()
            : undefined
      await authorization.start({
        start: {
          provider,
          name: account?.name ?? form.name.trim(),
          accountId: account?.id,
          outboundProxyId: startProxyId,
        },
        settings: account
          ? undefined
          : accountImportSettings(form),
      })
    }
    catch (cause) {
      ZNotification.error({
        message: errorMessage(cause, t('accounts.onboarding.failedToStartAuthorization')),
      })
    }
  }

  function openCreateAccount() {
    authorization.reset()
    reauthorizingAccount.value = null
    emailAuthSubmitError.value = ''
    createForm.value = emptyAccountCreateForm()
    showCreateModal.value = true
  }

  function openReauthorizeAccount(account: Account) {
    if (isSupportedProvider(account.provider) && account.authenticationKind !== 'oauth')
      return
    authorization.reset()
    reauthorizingAccount.value = account
    createForm.value = {
      ...emptyAccountCreateForm(),
      source: { kind: 'provider', id: account.provider },
      step: 'import',
    }
    showCreateModal.value = true
    void handleAuthorizeOAuth()
  }

  async function finishCreate(message: string) {
    showCreateModal.value = false
    ZNotification.success({ message })
    await options.reload()
  }

  watch(() => accountCreateSourceKey(createForm.value), () => {
    emailAuthSubmitError.value = ''
    createForm.value = {
      ...createForm.value,
      mode: reauthorizingAccount.value
        ? 'oauth'
        : createForm.value.source?.kind === 'bundle'
          ? 'json'
          : accountImportModes(accountCreateProvider(createForm.value))[0]?.value ?? 'json',
      apiKey: emptyApiKeyAccountForm(),
      importTexts: {
        access_token: '',
        refresh_token: '',
        json: '',
        email: '',
      },
    }
  }, { flush: 'sync' })

  watch([
    () => accountCreateSourceKey(createForm.value),
    () => createForm.value.mode,
    () => createForm.value.step,
    () => createForm.value.proxyMode,
    () => createForm.value.proxyMode === 'proxy'
      ? createForm.value.proxyId.trim()
      : '',
  ], () => {
    authorization.reset()
  }, { flush: 'sync' })

  return {
    showCreateModal,
    reauthorizingAccount,
    creatingAccount: creatingAccountAction.loading,
    authorization: authorization.view,
    authorizationCallback: authorization.callback,
    emailAuthSubmitError,
    createForm,
    handleCreate,
    handleAuthorizeOAuth,
    openCreateAccount,
    openReauthorizeAccount,
    clearCreate,
  }
}
