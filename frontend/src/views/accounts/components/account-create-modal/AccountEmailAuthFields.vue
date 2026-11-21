<script setup lang="ts">
import type { AccountEmailAuthStatusView } from '../../composables/useAccountEmailAuth'

import { ZButton, ZFormItem, ZScrollbar, ZTextarea } from '@codex-proxy/ui'
import { CircleAlert, Mail } from '@lucide/vue'
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { MAX_EMAIL_AUTH_COUNT, parseEmailAuthInput } from '../../utils/accountCreate'

defineProps<{
  disabled: boolean
  emailAuth: AccountEmailAuthStatusView
}>()

const emit = defineEmits<{ openProgress: [] }>()

const { t } = useI18n({ useScope: 'global' })

const text = defineModel<string>({ required: true })
const parsed = computed(() => parseEmailAuthInput(text.value))
</script>

<template>
  <div class="flex flex-col gap-4">
    <div class="rounded-z bg-z-fill-quaternary px-4 py-3">
      <div class="flex items-start gap-3">
        <div
          class="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-z bg-z-bg-container text-z-primary-text"
        >
          <Mail class="size-4" aria-hidden="true" />
        </div>
        <div class="min-w-0 flex-1">
          <p class="m-0 text-z font-bold text-z-text">
            {{ t('accounts.emailAuth.inputHelpTitle') }}
          </p>
          <p class="m-0 mt-1 text-z-sm leading-[1.55] font-medium text-z-text-secondary">
            {{ t('accounts.emailAuth.inputHelpDetail') }}
          </p>
        </div>
      </div>
    </div>

    <div
      v-if="emailAuth.initialized && !emailAuth.configured"
      role="alert"
      class="flex items-center gap-2 rounded-z bg-z-warning-container px-3.5 py-2.5 text-z-sm font-medium text-z-warning-on-container"
    >
      <CircleAlert class="size-4 shrink-0" aria-hidden="true" />
      <span>{{ t('accounts.emailAuth.notConfigured') }}</span>
    </div>

    <div
      v-else-if="emailAuth.isRunning"
      role="alert"
      class="flex flex-wrap items-center justify-between gap-2 rounded-z bg-z-warning-container px-3.5 py-2.5 text-z-sm font-medium text-z-warning-on-container"
    >
      <span class="flex items-center gap-2">
        <CircleAlert class="size-4 shrink-0" aria-hidden="true" />
        <span>{{ t('accounts.emailAuth.batchRunning') }}</span>
      </span>
      <ZButton size="small" variant="plain" :disabled="disabled" @click="emit('openProgress')">
        {{ t('accounts.emailAuth.openProgress') }}
      </ZButton>
    </div>

    <ZFormItem :label="t('accounts.emailAuth.emailsLabel')" required>
      <template #extra>
        <span
          class="font-mono text-z-xs tabular-nums"
          :class="parsed.tooMany ? 'font-semibold text-z-error-text' : 'text-z-text-tertiary'"
        >
          {{ parsed.totalCount }} / {{ MAX_EMAIL_AUTH_COUNT }}
        </span>
      </template>
      <ZTextarea
        v-model="text"
        :aria-label="t('accounts.emailAuth.emailsLabel')"
        :rows="8"
        :placeholder="t('accounts.emailAuth.emailsPlaceholder')"
        :disabled="disabled"
      />
    </ZFormItem>

    <p
      v-if="parsed.tooMany"
      role="alert"
      class="m-0 flex items-center gap-1.5 text-z-xs text-z-error-text"
    >
      <CircleAlert class="size-3.5 shrink-0" aria-hidden="true" />
      <span>{{ t('accounts.emailAuth.tooManyEmails', { max: MAX_EMAIL_AUTH_COUNT, count: parsed.totalCount }) }}</span>
    </p>

    <div
      v-if="parsed.invalidEmails.length > 0"
      role="alert"
      class="grid gap-1.5 rounded-z bg-z-error-container px-3.5 py-2.5 text-z-xs text-z-error-on-container"
    >
      <p class="m-0 flex items-center gap-1.5 font-semibold">
        <CircleAlert class="size-3.5 shrink-0" aria-hidden="true" />
        <span>{{ t('accounts.emailAuth.invalidEmails', { count: parsed.invalidEmails.length }) }}</span>
      </p>
      <ZScrollbar max-height="96px">
        <ul class="m-0 list-none space-y-0.5 p-0 font-mono">
          <li v-for="invalid in parsed.invalidEmails" :key="invalid" class="wrap-anywhere">
            {{ invalid }}
          </li>
        </ul>
      </ZScrollbar>
    </div>

    <p
      v-if="emailAuth.submitError"
      role="alert"
      class="m-0 flex items-start gap-1.5 rounded-z bg-z-error-container px-3.5 py-2.5 text-z-xs leading-relaxed text-z-error-on-container"
    >
      <CircleAlert class="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
      <span class="wrap-anywhere">{{ emailAuth.submitError }}</span>
    </p>
  </div>
</template>
