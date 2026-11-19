<script setup lang="ts">
import type { IqProbeQuestion, IqProbeReasoningEffort, IqProbeSettings, IqProbeSettingsView, IqProbeStatus } from '@/api/modules/iq-probe'
import { ZButton, ZCard, ZFormItem, ZIconButton, ZInput, ZInputTag, ZSelect, ZSwitch, ZTextarea } from '@codex-proxy/ui'

import { Plus, Save, Sparkles, Trash2 } from '@lucide/vue'
import { isEqual } from 'es-toolkit'
import { computed, onMounted, ref, shallowRef } from 'vue'
import { useI18n } from 'vue-i18n'
import { getIqProbeSettings, updateIqProbeSettings } from '@/api/modules/iq-probe'
import { ApiError } from '@/api/request'
import { notification as ZNotification } from '@/utils/notification'
import { generateRequestId } from '@/utils/operation'

const props = defineProps<{ disabled: boolean }>()

const { t } = useI18n({ useScope: 'global' })

interface QuestionDraft {
  key: string
  prompt: string
  model: string
  reasoningEffort: IqProbeReasoningEffort | ''
  passKeywords: string[]
  failKeywords: string[]
  maxAttempts: string
}

interface SettingsDraft {
  enabled: boolean
  intervalMinutes: string
  staggerMinutes: string
  recheckDelayMinutes: string
  breakerThreshold: string
  questions: QuestionDraft[]
}

type QuestionErrors = Partial<Record<'prompt' | 'model' | 'passKeywords' | 'failKeywords' | 'maxAttempts', string>>

// 与后端 IqProbeSettings::validate 保持一致，前端先拦一遍
const limits = {
  intervalMinutes: [1, 1440],
  staggerMinutes: [0, 60],
  recheckDelayMinutes: [1, 60],
  breakerThreshold: [1, 100],
  maxAttempts: [1, 5],
  questions: 10,
  promptChars: 2000,
  modelChars: 128,
  keywords: 10,
  keywordChars: 64,
} as const

const reasoningEfforts: IqProbeReasoningEffort[] = [
  'none',
  'minimal',
  'low',
  'medium',
  'high',
  'xhigh',
]
const reasoningOptions = computed(() => [
  {
    label: t('settings.iqProbe.reasoningUnset'),
    value: '',
  },
  ...reasoningEfforts.map(value => ({ label: value, value })),
])

const loading = shallowRef(false)
const saving = shallowRef(false)
const loaded = shallowRef(false)
const showErrors = shallowRef(false)
const revision = shallowRef(0)
const status = shallowRef<IqProbeStatus | null>(null)
const draft = ref<SettingsDraft>(emptyDraft())
const saved = shallowRef<SettingsDraft>(emptyDraft())

const busy = computed(() => props.disabled || loading.value || saving.value || !loaded.value)
const hasChanges = computed(() => loaded.value && !isEqual(comparable(draft.value), comparable(saved.value)))
const enabling = computed(() => draft.value.enabled && !saved.value.enabled)
const windowHours = computed(() => Math.round((status.value?.windowMinutes ?? 120) / 60))

function emptyDraft(): SettingsDraft {
  return {
    enabled: false,
    intervalMinutes: '',
    staggerMinutes: '',
    recheckDelayMinutes: '',
    breakerThreshold: '',
    questions: [],
  }
}

function toQuestionDraft(question: IqProbeQuestion): QuestionDraft {
  return {
    key: generateRequestId(),
    prompt: question.prompt,
    model: question.model,
    reasoningEffort: question.reasoningEffort ?? '',
    passKeywords: [...question.passKeywords],
    failKeywords: [...question.failKeywords],
    maxAttempts: String(question.maxAttempts),
  }
}

function toDraft(settings: IqProbeSettings): SettingsDraft {
  return {
    enabled: settings.enabled,
    intervalMinutes: String(settings.intervalMinutes),
    staggerMinutes: String(settings.staggerMinutes),
    recheckDelayMinutes: String(settings.recheckDelayMinutes),
    breakerThreshold: String(settings.breakerThreshold),
    questions: settings.questions.map(toQuestionDraft),
  }
}

// 比较改动时忽略仅用于渲染的本地 key
function comparable(value: SettingsDraft) {
  return {
    ...value,
    questions: value.questions.map(({ key: _key, ...question }) => question),
  }
}

function apply(view: IqProbeSettingsView) {
  revision.value = view.revision
  status.value = view.status
  draft.value = toDraft(view.settings)
  saved.value = toDraft(view.settings)
  showErrors.value = false
  loaded.value = true
}

async function load() {
  if (loading.value)
    return
  loading.value = true
  try {
    apply(await getIqProbeSettings())
  }
  catch {}
  finally {
    loading.value = false
  }
}

function charCount(value: string) {
  return [...value].length
}

function normalizedKeywords(values: string[]) {
  return values.map(value => value.trim()).filter(Boolean)
}

function integerError(value: string, [min, max]: readonly [number, number]) {
  const number = Number(value)
  return value.trim() && Number.isInteger(number) && number >= min && number <= max
    ? ''
    : t('settings.iqProbe.errors.integerRange', { min, max })
}

function keywordsError(values: string[], min: number) {
  if (values.some(value => !value.trim()))
    return t('settings.iqProbe.errors.keywordEmpty')
  if (values.some(value => charCount(value.trim()) > limits.keywordChars))
    return t('settings.iqProbe.errors.keywordLength', { max: limits.keywordChars })
  const count = normalizedKeywords(values).length
  return count >= min && count <= limits.keywords
    ? ''
    : t('settings.iqProbe.errors.keywordCount', { min, max: limits.keywords })
}

function questionErrors(question: QuestionDraft): QuestionErrors {
  const promptLength = charCount(question.prompt.trim())
  const modelLength = charCount(question.model.trim())
  return {
    prompt: promptLength >= 1 && promptLength <= limits.promptChars
      ? ''
      : t('settings.iqProbe.errors.textLength', { max: limits.promptChars }),
    model: modelLength >= 1 && modelLength <= limits.modelChars
      ? ''
      : t('settings.iqProbe.errors.textLength', { max: limits.modelChars }),
    passKeywords: keywordsError(question.passKeywords, 1),
    failKeywords: keywordsError(question.failKeywords, 0),
    maxAttempts: integerError(question.maxAttempts, limits.maxAttempts),
  }
}

const errors = computed(() => ({
  intervalMinutes: integerError(draft.value.intervalMinutes, limits.intervalMinutes),
  staggerMinutes: integerError(draft.value.staggerMinutes, limits.staggerMinutes),
  recheckDelayMinutes: integerError(draft.value.recheckDelayMinutes, limits.recheckDelayMinutes),
  breakerThreshold: integerError(draft.value.breakerThreshold, limits.breakerThreshold),
  questions: draft.value.questions.length >= 1 && draft.value.questions.length <= limits.questions
    ? ''
    : t('settings.iqProbe.errors.questionCount', { max: limits.questions }),
  questionItems: draft.value.questions.map(questionErrors),
}))

const valid = computed(() => {
  const { questionItems, ...fields } = errors.value
  return Object.values(fields).every(message => !message)
    && questionItems.every(item => Object.values(item).every(message => !message))
})

function visibleError(message: string | undefined) {
  return showErrors.value && message
    ? message
    : undefined
}

function addQuestion() {
  if (busy.value || draft.value.questions.length >= limits.questions)
    return
  draft.value.questions.push({
    key: generateRequestId(),
    prompt: '',
    model: '',
    reasoningEffort: '',
    passKeywords: [],
    failKeywords: [],
    maxAttempts: '1',
  })
}

function removeQuestion(key: string) {
  if (busy.value)
    return
  draft.value.questions = draft.value.questions.filter(question => question.key !== key)
}

function payload(): IqProbeSettings {
  const value = draft.value
  return {
    enabled: value.enabled,
    intervalMinutes: Number(value.intervalMinutes),
    staggerMinutes: Number(value.staggerMinutes),
    recheckDelayMinutes: Number(value.recheckDelayMinutes),
    breakerThreshold: Number(value.breakerThreshold),
    questions: value.questions.map(question => ({
      prompt: question.prompt.trim(),
      model: question.model.trim(),
      reasoningEffort: question.reasoningEffort || null,
      passKeywords: normalizedKeywords(question.passKeywords),
      failKeywords: normalizedKeywords(question.failKeywords),
      maxAttempts: Number(question.maxAttempts),
    })),
  }
}

async function save() {
  if (busy.value || !hasChanges.value)
    return
  showErrors.value = true
  if (!valid.value)
    return
  saving.value = true
  try {
    apply(await updateIqProbeSettings({
      settings: payload(),
      revision: revision.value,
    }, { silent: true }))
    ZNotification.success({
      message: t('settings.iqProbe.saved'),
    })
  }
  catch (error) {
    // 版本冲突单独提示并取回最新设置，其他错误照常显示后端原因
    if (error instanceof ApiError && (error.status === 409 || error.code === 409)) {
      ZNotification.warning({
        message: t('settings.iqProbe.conflict'),
      })
      saving.value = false
      await load()
      return
    }
    if (error instanceof ApiError && error.kind !== 'cancelled')
      ZNotification.error({ message: error.message })
  }
  finally {
    saving.value = false
  }
}

onMounted(load)

defineExpose({ hasChanges })
</script>

<template>
  <ZCard :title="t('settings.iqProbe.title')" :description="t('settings.iqProbe.description')">
    <template #actions>
      <ZButton type="primary" :loading="saving" :disabled="busy || !hasChanges" @click="save">
        <template #icon>
          <Save class="size-4" />
        </template>
        {{ t('common.actions.save') }}
      </ZButton>
    </template>

    <div class="grid min-w-0 max-w-6xl gap-4.5 sm:grid-cols-2 sm:items-end">
      <div class="col-span-full flex flex-wrap items-center justify-between gap-3">
        <div class="grid gap-1">
          <ZSwitch
            v-model="draft.enabled"
            :disabled="busy"
            class="justify-self-start"
            :label="t('settings.iqProbe.enable')"
          />
          <span v-if="enabling" class="text-z-sm text-z-warning-text">{{ t('settings.iqProbe.enableHint') }}</span>
        </div>
        <div v-if="status" class="flex flex-wrap items-center gap-2 text-z-sm">
          <span class="text-z-text-secondary">
            {{ t('settings.iqProbe.autoDisabledInWindow', { hours: windowHours, count: status.autoDisabledInWindow }) }}
          </span>
          <span
            v-if="status.breakerActive"
            role="status"
            class="inline-flex h-6 items-center rounded-full bg-z-warning-container px-2.5 text-z-xs font-heavy text-z-warning-on-container"
          >
            {{ t('settings.iqProbe.breakerActive') }}
          </span>
        </div>
      </div>

      <ZFormItem
        :label="t('settings.iqProbe.intervalMinutes')"
        :description="t('settings.iqProbe.intervalHelp')"
        :error="visibleError(errors.intervalMinutes)"
      >
        <ZInput v-model="draft.intervalMinutes" :disabled="busy" type="number" min="1" max="1440" step="1" :aria-label="t('settings.iqProbe.intervalMinutes')">
          <template #suffix>
            <span class="text-z-sm">{{ t('settings.iqProbe.minutes') }}</span>
          </template>
        </ZInput>
      </ZFormItem>

      <ZFormItem
        :label="t('settings.iqProbe.staggerMinutes')"
        :description="t('settings.iqProbe.staggerHelp')"
        :error="visibleError(errors.staggerMinutes)"
      >
        <ZInput v-model="draft.staggerMinutes" :disabled="busy" type="number" min="0" max="60" step="1" :aria-label="t('settings.iqProbe.staggerMinutes')">
          <template #suffix>
            <span class="text-z-sm">{{ t('settings.iqProbe.minutes') }}</span>
          </template>
        </ZInput>
      </ZFormItem>

      <ZFormItem
        :label="t('settings.iqProbe.recheckDelayMinutes')"
        :description="t('settings.iqProbe.recheckHelp')"
        :error="visibleError(errors.recheckDelayMinutes)"
      >
        <ZInput v-model="draft.recheckDelayMinutes" :disabled="busy" type="number" min="1" max="60" step="1" :aria-label="t('settings.iqProbe.recheckDelayMinutes')">
          <template #suffix>
            <span class="text-z-sm">{{ t('settings.iqProbe.minutes') }}</span>
          </template>
        </ZInput>
      </ZFormItem>

      <ZFormItem
        :label="t('settings.iqProbe.breakerThreshold')"
        :description="t('settings.iqProbe.breakerHelp', { hours: windowHours })"
        :error="visibleError(errors.breakerThreshold)"
      >
        <ZInput v-model="draft.breakerThreshold" :disabled="busy" type="number" min="1" max="100" step="1" :aria-label="t('settings.iqProbe.breakerThreshold')">
          <template #suffix>
            <span class="text-z-sm">{{ t('settings.iqProbe.accountsUnit') }}</span>
          </template>
        </ZInput>
      </ZFormItem>

      <section class="col-span-full grid min-w-0 gap-3" :aria-label="t('settings.iqProbe.questions')">
        <div class="flex flex-wrap items-center justify-between gap-2">
          <span class="text-z-sm font-emphasis text-z-text-secondary">{{ t('settings.iqProbe.questions') }}</span>
          <ZButton size="small" :disabled="busy || draft.questions.length >= limits.questions" @click="addQuestion">
            <template #icon>
              <Plus class="size-3.5" />
            </template>
            {{ t('settings.iqProbe.addQuestion') }}
          </ZButton>
        </div>

        <p v-if="visibleError(errors.questions)" role="alert" class="m-0 text-z-sm text-z-error-text">
          {{ errors.questions }}
        </p>

        <p v-if="loaded && !draft.questions.length" class="m-0 rounded-z bg-z-fill-quaternary px-4 py-4 text-z-sm text-z-text-tertiary">
          {{ t('settings.iqProbe.emptyQuestions') }}
        </p>

        <div
          v-for="(question, index) in draft.questions"
          :key="question.key"
          class="grid min-w-0 gap-4 rounded-z bg-z-fill-quaternary p-3 sm:grid-cols-2 sm:px-4"
        >
          <div class="col-span-full flex items-center justify-between gap-2">
            <span class="text-z-sm font-heavy text-z-text">{{ t('settings.iqProbe.questionIndex', { index: index + 1 }) }}</span>
            <ZIconButton
              :disabled="busy"
              :aria-label="t('settings.iqProbe.removeQuestion', { index: index + 1 })"
              @click="removeQuestion(question.key)"
            >
              <Trash2 class="size-4" />
            </ZIconButton>
          </div>

          <ZFormItem
            class="col-span-full"
            :label="t('settings.iqProbe.prompt')"
            :error="visibleError(errors.questionItems[index]?.prompt)"
          >
            <ZTextarea
              v-model="question.prompt"
              :rows="2"
              :disabled="busy"
              :aria-label="t('settings.iqProbe.prompt')"
              spellcheck="false"
            />
          </ZFormItem>

          <ZFormItem
            :label="t('settings.iqProbe.model')"
            :description="t('settings.iqProbe.modelHelp')"
            :error="visibleError(errors.questionItems[index]?.model)"
          >
            <ZInput v-model="question.model" :disabled="busy" :aria-label="t('settings.iqProbe.model')" spellcheck="false">
              <template #prefix>
                <Sparkles class="size-4" />
              </template>
            </ZInput>
          </ZFormItem>

          <ZFormItem :label="t('settings.iqProbe.reasoningEffort')">
            <ZSelect
              :model-value="question.reasoningEffort"
              :options="reasoningOptions"
              :placeholder="t('settings.iqProbe.reasoningUnset')"
              :disabled="busy"
              :aria-label="t('settings.iqProbe.reasoningEffort')"
              @update:model-value="question.reasoningEffort = ($event ?? '') as QuestionDraft['reasoningEffort']"
            />
          </ZFormItem>

          <ZFormItem
            :label="t('settings.iqProbe.passKeywords')"
            :description="t('settings.iqProbe.passKeywordsHelp')"
            :error="visibleError(errors.questionItems[index]?.passKeywords)"
          >
            <ZInputTag
              v-model="question.passKeywords"
              :max="limits.keywords"
              :disabled="busy"
              :aria-label="t('settings.iqProbe.passKeywords')"
              :placeholder="t('settings.iqProbe.keywordPlaceholder')"
            />
          </ZFormItem>

          <ZFormItem
            :label="t('settings.iqProbe.failKeywords')"
            :description="t('settings.iqProbe.failKeywordsHelp')"
            :error="visibleError(errors.questionItems[index]?.failKeywords)"
          >
            <ZInputTag
              v-model="question.failKeywords"
              :max="limits.keywords"
              :disabled="busy"
              :aria-label="t('settings.iqProbe.failKeywords')"
              :placeholder="t('settings.iqProbe.keywordPlaceholder')"
            />
          </ZFormItem>

          <ZFormItem
            :label="t('settings.iqProbe.maxAttempts')"
            :description="t('settings.iqProbe.maxAttemptsHelp')"
            :error="visibleError(errors.questionItems[index]?.maxAttempts)"
          >
            <ZInput v-model="question.maxAttempts" :disabled="busy" type="number" min="1" max="5" step="1" :aria-label="t('settings.iqProbe.maxAttempts')">
              <template #suffix>
                <span class="text-z-sm">{{ t('settings.iqProbe.timesUnit') }}</span>
              </template>
            </ZInput>
          </ZFormItem>
        </div>
      </section>
    </div>
  </ZCard>
</template>
