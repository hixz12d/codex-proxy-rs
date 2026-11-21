import type { ApiKeyAccountForm } from './upstreamApiKey'
import type { AccountModelAccess } from '@/api'
import { t } from '@/i18n'
import { accountSchedulingValues } from './schedulingForm'
import { emptyApiKeyAccountForm } from './upstreamApiKey'

export type AccountCreateSource = { kind: 'bundle' } | { kind: 'provider', id: string }
export type AccountImportMode = 'oauth' | 'api_key' | 'access_token' | 'refresh_token' | 'json' | 'email'
type AccountImportInputMode = 'access_token' | 'refresh_token' | 'json' | 'email'

export const MAX_EMAIL_AUTH_COUNT = 50
const EMAIL_DOMAIN_LABEL_RE = /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/

export interface ParsedEmailAuthInput {
  emails: string[]
  invalidEmails: string[]
  totalCount: number
  tooMany: boolean
}

function hasForbiddenEmailChar(value: string): boolean {
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index)
    if (code <= 0x20 || code === 0x7F)
      return true
  }
  return /\s/.test(value)
}

export function isValidEmailFormat(value: string): boolean {
  if (!value || value.length > 254 || hasForbiddenEmailChar(value))
    return false
  const atIndex = value.indexOf('@')
  if (atIndex <= 0 || atIndex !== value.lastIndexOf('@'))
    return false
  const local = value.slice(0, atIndex)
  const domain = value.slice(atIndex + 1)
  if (local.length > 64 || local.startsWith('.') || local.endsWith('.') || local.includes('..'))
    return false
  const labels = domain.split('.')
  if (labels.length < 2 || (labels[labels.length - 1]?.length ?? 0) < 2)
    return false
  return labels.every(label => EMAIL_DOMAIN_LABEL_RE.test(label))
}

export function parseEmailAuthInput(raw: string): ParsedEmailAuthInput {
  const unique = new Set<string>()
  for (const line of raw.split(/\r?\n/)) {
    const normalized = line.trim().toLowerCase()
    if (normalized)
      unique.add(normalized)
  }
  const emails: string[] = []
  const invalidEmails: string[] = []
  for (const entry of unique) {
    if (isValidEmailFormat(entry))
      emails.push(entry)
    else
      invalidEmails.push(entry)
  }
  return {
    emails,
    invalidEmails,
    totalCount: unique.size,
    tooMany: unique.size > MAX_EMAIL_AUTH_COUNT,
  }
}

export interface AccountCreateForm {
  name: string
  source: AccountCreateSource | null
  apiKey: ApiKeyAccountForm
  notes: string
  enabled: boolean
  concurrencyLimit: string
  weight: string
  modelAccess?: AccountModelAccess
  groupIds: string[]
  step: 'settings' | 'import'
  mode: AccountImportMode
  importTexts: Record<AccountImportInputMode, string>
  proxyMode: string
  proxyId: string
}

export function emptyAccountCreateForm(): AccountCreateForm {
  return {
    name: '',
    source: null,
    apiKey: emptyApiKeyAccountForm(),
    notes: '',
    enabled: true,
    concurrencyLimit: '',
    weight: '1',
    groupIds: [],
    step: 'settings',
    mode: 'oauth',
    importTexts: {
      access_token: '',
      refresh_token: '',
      json: '',
      email: '',
    },
    proxyMode: 'random',
    proxyId: '',
  }
}

export function accountCreateProvider(form: AccountCreateForm) {
  return form.source?.kind === 'provider'
    ? form.source.id
    : undefined
}

export function accountCreateSourceKey(form: AccountCreateForm) {
  return form.source?.kind === 'provider'
    ? `provider:${form.source.id}`
    : form.source?.kind ?? ''
}

export function accountImportSettings(form: AccountCreateForm) {
  const scheduling = accountSchedulingValues(form.concurrencyLimit, form.weight)
  return {
    name: form.source?.kind === 'provider'
      ? form.name.trim() || undefined
      : undefined,
    modelAccess: form.modelAccess
      ? {
          ...form.modelAccess,
          models: [...form.modelAccess.models],
        }
      : undefined,
    enabled: form.enabled,
    ...scheduling,
    groupIds: [
      ...new Set(form.groupIds),
    ],
    notes: form.notes.trim() || undefined,
  }
}

export function accountImportModes(provider: string | undefined) {
  if (provider === 'openai') {
    return [
      { label: 'OAuth', value: 'oauth' as const },
      {
        label: t('accounts.emailAuth.modeLabel'),
        value: 'email' as const,
      },
      {
        label: t('common.fields.apiKey'),
        value: 'api_key' as const,
      },
      { label: 'AT', value: 'access_token' as const },
      { label: 'RT', value: 'refresh_token' as const },
      {
        label: t('accounts.onboarding.accountFile'),
        value: 'json' as const,
      },
    ]
  }
  if (provider === 'xai') {
    return [
      { label: 'OAuth', value: 'oauth' as const },
      {
        label: t('accounts.onboarding.accountFile'),
        value: 'json' as const,
      },
    ]
  }
  return []
}
