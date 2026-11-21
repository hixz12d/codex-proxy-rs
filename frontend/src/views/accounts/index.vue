<script setup lang="ts">
import type { Account } from '@/api'

import { useTableColumns, ZCard, ZCheckbox, ZConfirmDialog, ZIconButton, ZPageHeader, ZPagination, ZTable, ZTableColumnSettings } from '@codex-proxy/ui'

import { ChevronDown } from '@lucide/vue'
import { computed, ref, shallowRef, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import AccountPlanBadge from '@/components/account/AccountPlanBadge.vue'
import AccountGroupMarks from '@/components/AccountGroupMarks.vue'
import LastUsedAtCell from '@/components/LastUsedAtCell.vue'
import ProviderIconGroup from '@/components/ProviderIconGroup.vue'
import { useAccountGroupCatalog } from '@/composables/useAccountGroupCatalog'
import AccountCreateModal from './components/account-create-modal/index.vue'
import AccountEmailAuth from './components/account-email-auth/index.vue'
import AccountImportTasks from './components/account-import-tasks/index.vue'
import AccountIqProbeCell from './components/account-iq-probe-cell/index.vue'
import { useIqProbeRounds } from './components/account-iq-probe-cell/useIqProbeRounds'
import AccountQuotaPanel from './components/account-quota-panel/index.vue'
import AccountQuotaSummaryCell from './components/account-quota-summary-cell/index.vue'
import AccountStatusBadge from './components/account-status-badge/index.vue'
import AccountBatchEditModal from './components/AccountBatchEditModal.vue'
import AccountCompareTestModal from './components/AccountCompareTestModal.vue'
import AccountConcurrencyModal from './components/AccountConcurrencyModal.vue'
import AccountConnectionTestModal from './components/AccountConnectionTestModal.vue'
import AccountEditModal from './components/AccountEditModal.vue'
import AccountFilters from './components/AccountFilters.vue'
import AccountIdentityCell from './components/AccountIdentityCell.vue'
import AccountOverviewCards from './components/AccountOverviewCards.vue'
import AccountTableActions from './components/AccountTableActions.vue'
import AccountUsagePanel from './components/AccountUsagePanel.vue'
import { useAccountBatchEditor } from './composables/useAccountBatchEditor'
import { useAccountConnectionTest } from './composables/useAccountConnectionTest'
import { useAccountEditor } from './composables/useAccountEditor'
import { useAccountEmailAuth } from './composables/useAccountEmailAuth'
import { useAccountImportTasks } from './composables/useAccountImportTasks'
import { useAccountMutations } from './composables/useAccountMutations'
import { useAccountsQuery } from './composables/useAccountsQuery'
import { useAccountsTable } from './composables/useAccountsTable'
import { accountColumns } from './constants'
import { accountIdentityLabel } from './utils/accountIdentity'

const { t } = useI18n({ useScope: 'global' })

const concurrencyAccount = shallowRef<Account | null>(null)
const showConcurrency = shallowRef(false)
function openConcurrency(account: Account) {
  concurrencyAccount.value = account
  showConcurrency.value = true
}

const selectedIds = ref<Set<string>>(new Set())
const { visibleColumns, columnOptions, setColumnVisible, setColumnOrder, resetColumns } = useTableColumns(accountColumns, 'accounts')
const {
  loading,
  accounts,
  loadAccounts,
  refreshAccountsSilently,
  searchQuery,
  providerQuery,
  statusQuery,
  groupQuery,
  sort,
  accountSummary,
  accountPagination,
  replaceAccount,
  handlePageChange,
  handlePageSizeChange,
  handleSortChange,
} = useAccountsQuery()

const { roundsByAccount: iqProbeRounds } = useIqProbeRounds(accounts)

const {
  groups,
  loading: groupsLoading,
  loadGroups,
} = useAccountGroupCatalog()

const importTasks = useAccountImportTasks({
  reload: () => Promise.all([
    loadAccounts(),
    loadGroups(),
  ]),
})
const {
  open: showImportTasks,
  tasks: recentImportTasks,
  selectedId: importTaskId,
  detail: importTaskDetail,
  loading: loadingImportTasks,
  stopping: stoppingImportTask,
  error: importTaskError,
  activeCount: activeImportCount,
} = importTasks

const emailAuth = useAccountEmailAuth({
  reload: () => Promise.all([
    loadAccounts(),
    loadGroups(),
  ]),
})
const {
  open: showEmailAuth,
  batch: emailAuthBatch,
  loading: loadingEmailAuth,
  stopping: stoppingEmailAuth,
  retryingEmails,
  error: emailAuthError,
  hasBatch: hasEmailAuth,
  activeCount: activeEmailAuthCount,
} = emailAuth

const {
  showCreateModal,
  showDeleteModal,
  showSingleDeleteModal,
  pendingDeleteAccount,
  deleteCount,
  recoveringAccountIds,
  refreshingAccountIds,
  refreshingQuotaAccountIds,
  downloadingCatalogAccountIds,
  togglingSchedulingAccountIds,
  deletingAccount,
  creatingAccount,
  authorization,
  authorizationCallback,
  emailAuthSubmitError,
  batchDeleting,
  exportingAccounts,
  exportDisabledReason,
  reauthorizingAccount,
  createForm,
  handleCreate,
  handleAuthorizeOAuth,
  openCreateAccount,
  clearCreate,
  openReauthorizeAccount,
  requestDeleteAccount,
  handleDelete,
  handleBatchDelete,
  handleExportAccounts,
  handleDownloadModelCatalog,
  handleRecover,
  handleRefresh,
  handleRefreshQuota,
  handleQuotaReset,
  handleToggleScheduling,
} = useAccountMutations({
  onImportTaskCreated: importTasks.created,
  onEmailAuthCreated: emailAuth.created,
  onEmailAuthRefresh: () => void emailAuth.refresh(true),
  accounts,
  selectedIds,
  reload: () => Promise.all([
    loadAccounts(),
    loadGroups(),
  ]),
  replaceAccount,
})

const emailAuthStatusView = computed(() => ({
  initialized: emailAuth.initialized.value,
  configured: emailAuth.configured.value,
  isRunning: emailAuth.isRunning.value,
  submitError: emailAuthSubmitError.value,
}))

const {
  showConnectionTestModal,
  testingAccount,
  connectionTestStatus,
  connectionTestModel,
  connectionTestLogs,
  connectionTestError,
  connectionTestStartedAt,
  connectionTestFinishedAt,
  connectionTestDurationMs,
  testingConnectionIds,
  loadingConnectionTestModels,
  refreshingConnectionTestModels,
  connectionTestSelectedModel,
  connectionTestModelOptions,
  connectionTestInputText,
  connectionTestReasoning,
  connectionTestStatusView,
  openConnectionTest,
  handleRefreshConnectionTestModels,
  handleTestConnection,
} = useAccountConnectionTest({ reload: refreshAccountsSilently })

// 选中账号可能跨页，记住见过的行数据，打开对比测试时按选中 ID 映射
const selectedAccountRows = new Map<string, Account>()
watch([accounts, selectedIds], ([rows, ids]) => {
  for (const row of rows) {
    if (ids.has(row.id))
      selectedAccountRows.set(row.id, row)
  }
  for (const id of selectedAccountRows.keys()) {
    if (!ids.has(id))
      selectedAccountRows.delete(id)
  }
}, { immediate: true, flush: 'sync' })

const showCompareTestModal = shallowRef(false)
const compareTestAccounts = shallowRef<Account[]>([])

function openCompareTest() {
  compareTestAccounts.value = [...selectedIds.value]
    .map(id => selectedAccountRows.get(id))
    .filter((row): row is Account => Boolean(row))
  if (compareTestAccounts.value.length > 0)
    showCompareTestModal.value = true
}

const {
  expandedAccountIds,
  allSelected,
  indeterminate,
  selectedRowKeys,
  expandedRowKeys,
  toggleSelection,
  toggleExpanded,
  toggleAll,
} = useAccountsTable(accounts, selectedIds)

const {
  showBatchEditModal,
  schedulingEnabled: batchSchedulingEnabled,
  concurrencyLimit: batchConcurrencyLimit,
  weight: batchWeight,
  modelAccess: batchModelAccess,
  editedFields: batchEditedFields,
  catalogAccountId: batchCatalogAccountId,
  editingCount: batchEditingCount,
  proxyMode: batchProxyMode,
  proxyId: batchProxyId,
  selectedGroupIds: batchGroupIds,
  saving: savingBatchEdit,
  open: openBatchEdit,
  save: saveBatchEdit,
} = useAccountBatchEditor({
  accounts,
  selectedIds,
  reloadAccounts: loadAccounts,
  reloadGroups: loadGroups,
})

const {
  apiKey: editingApiKey,
  oauthTransport: editingOAuthTransport,
  configurationReady,
  showEditModal,
  editingAccount,
  name: editingName,
  notes: editingNotes,
  schedulingEnabled,
  concurrencyLimit: editingConcurrencyLimit,
  weight: editingWeight,
  modelAccess: editingModelAccess,
  proxyMode: editingProxyMode,
  proxyId: editingProxyId,
  selectedGroupIds: editingGroupIds,
  saving: savingAccountEdit,
  open: openAccountEdit,
  save: saveAccountEdit,
  clearCredentials,
} = useAccountEditor({
  reloadAccounts: loadAccounts,
  reloadGroups: loadGroups,
})
</script>

<template>
  <div class="flex min-h-0 w-full flex-col xl:h-[calc(100dvh-3rem)] xl:flex-none!">
    <ZPageHeader
      :title="t('navigation.accounts')"
      :description="t('accounts.page.description')"
    />

    <AccountOverviewCards :summary="accountSummary" />

    <ZCard
      class="mt-4 flex flex-col xl:min-h-80 xl:flex-1"
    >
      <template #header>
        <AccountFilters
          v-model:search="searchQuery"
          v-model:status="statusQuery"
          v-model:provider="providerQuery"
          v-model:group="groupQuery"
          :groups="groups"
          :groups-loading="groupsLoading"
          :selected-count="selectedIds.size"
          :batch-deleting="batchDeleting"
          :exporting-accounts="exportingAccounts"
          :export-disabled-reason="exportDisabledReason"
          :has-import-tasks="recentImportTasks.length > 0"
          :active-import-count="activeImportCount"
          :has-email-auth="hasEmailAuth"
          :active-email-auth-count="activeEmailAuthCount"
          @import-tasks="showImportTasks = true"
          @email-auth="showEmailAuth = true"
          @delete-selected="showDeleteModal = true"
          @export-selected="handleExportAccounts"
          @create="openCreateAccount"
          @edit-selected="openBatchEdit"
          @compare-selected="openCompareTest"
        >
          <template #actions>
            <ZTableColumnSettings
              :options="columnOptions"
              @change="setColumnVisible"
              @reorder="setColumnOrder"
              @reset="resetColumns"
            />
          </template>
        </AccountFilters>
      </template>

      <template #body>
        <div class="flex min-h-0 flex-col xl:h-full">
          <ZTable
            class="h-100! min-h-100 flex-none [--z-table-row-height:72px] xl:h-auto! xl:min-h-0 xl:flex-1"
            :columns="visibleColumns"
            :data="accounts"
            :loading="loading"
            :selected-row-keys="selectedRowKeys"
            :expanded-row-keys="expandedRowKeys"
            :sort="sort"
            :empty-text="t('accounts.page.noAccountDataYet')"
            @sort-change="handleSortChange"
          >
            <template #expander="{ row }">
              <ZIconButton
                size="small"
                class="size-6! rounded-md! hover:bg-z-bg-text-hover!"
                :aria-label="expandedAccountIds.has(row.id) ? t('accounts.page.collapseStatistics') : t('accounts.page.expandStatistics')"
                :aria-expanded="expandedAccountIds.has(row.id)"
                @click.stop="toggleExpanded(row.id)"
              >
                <ChevronDown
                  class="size-3.5 transition-transform"
                  :class="expandedAccountIds.has(row.id) ? '' : '-rotate-90'"
                />
              </ZIconButton>
            </template>

            <template #header-selection>
              <ZCheckbox
                :model-value="allSelected"
                :indeterminate="indeterminate"
                :aria-label="t('accounts.page.selectAccountOnCurrentPage')"
                @update:model-value="toggleAll"
              />
            </template>

            <template #selection="{ row }">
              <ZCheckbox
                :model-value="selectedIds.has(row.id)"
                :aria-label="t('accounts.page.selectAccount')"
                @update:model-value="toggleSelection(row.id)"
              />
            </template>

            <template #identity="{ row }">
              <AccountIdentityCell :account="row" show-notes />
            </template>

            <template #provider="{ row }">
              <ProviderIconGroup
                :provider="row.provider"
                :authentication-kind="row.authenticationKind"
              />
            </template>

            <template #status="{ row }">
              <AccountStatusBadge
                :quota="row.quota"
                :status="row.status"
                :error-reason="row.errorReason"
                :error-message="row.errorMessage"
                :rate-limit-recovery-display="row.quota.rateLimitRecoveryDisplay"
                :rate-limit-reason="row.quota.rateLimitReason"
                :protection-rules="row.quota.protectionRules"
                :recovery-probe-required="row.quota.recoveryProbeRequired"
                :next-refresh-at="row.nextRefreshAt"
                :next-refresh-at-display="row.nextRefreshAtDisplay"
              />
            </template>

            <template #iqProbe="{ row }">
              <div class="flex w-full justify-center">
                <AccountIqProbeCell :rounds="iqProbeRounds?.get(row.id) ?? null" />
              </div>
            </template>

            <template #planType="{ row }">
              <div class="-mx-3 flex min-w-0 justify-center">
                <AccountPlanBadge wrap :upstream="row.upstream" :authentication-kind="row.authenticationKind" :plan-type="row.planType" :plan-type-display="row.planTypeDisplay" />
              </div>
            </template>

            <template #usage="{ row }">
              <AccountQuotaSummaryCell :account="row" @show-concurrency="openConcurrency(row)" />
            </template>

            <template #groups="{ row }">
              <div class="flex w-full justify-center">
                <AccountGroupMarks :groups="row.groups" />
              </div>
            </template>

            <template #lastUsedAt="{ row }">
              <LastUsedAtCell :value="row.usage.lastUsedAt" :full-display="row.usage.lastUsedAtFullDisplay" />
            </template>

            <template #actions="{ row }">
              <AccountTableActions
                :account="row"
                :deleting="deletingAccount"
                :downloading-catalog="downloadingCatalogAccountIds.has(row.id)"
                :recovering="recoveringAccountIds.has(row.id)"
                :refreshing="refreshingAccountIds.has(row.id)"
                :testing="testingConnectionIds.has(row.id)"
                :toggling-scheduling="togglingSchedulingAccountIds.has(row.id)"
                @edit="openAccountEdit"
                @delete="requestDeleteAccount"
                @download-model-catalog="handleDownloadModelCatalog"
                @recover="handleRecover"
                @refresh="handleRefresh"
                @reauthorize="openReauthorizeAccount"
                @test="openConnectionTest"
                @toggle-scheduling="handleToggleScheduling"
              />
            </template>

            <template #expanded="{ row }">
              <div class="grid items-stretch gap-3 p-4 lg:grid-cols-[1.05fr_2.45fr] xl:h-77.5">
                <AccountQuotaPanel
                  :account="row"
                  :refreshing="refreshingQuotaAccountIds.has(row.id)"
                  @account-updated="void replaceAccount($event)"
                  @quota-reset="handleQuotaReset"
                  @refresh-quota="handleRefreshQuota"
                />
                <AccountUsagePanel :account="row" />
              </div>
            </template>
          </ZTable>
          <ZPagination
            v-bind="accountPagination"
            :disabled="loading"
            @current-change="handlePageChange"
            @size-change="handlePageSizeChange"
          />
        </div>
      </template>
    </ZCard>

    <AccountConcurrencyModal v-model="showConcurrency" :account="concurrencyAccount" />

    <AccountConnectionTestModal
      v-model="showConnectionTestModal"
      v-model:selected-model="connectionTestSelectedModel"
      v-model:input-text="connectionTestInputText"
      v-model:reasoning="connectionTestReasoning"
      :account="testingAccount"
      :duration-ms="connectionTestDurationMs"
      :error="connectionTestError"
      :finished-at="connectionTestFinishedAt"
      :loading-models="loadingConnectionTestModels"
      :refreshing-models="refreshingConnectionTestModels"
      :logs="connectionTestLogs"
      :model="connectionTestModel"
      :model-options="connectionTestModelOptions"
      :started-at="connectionTestStartedAt"
      :status="connectionTestStatus"
      :status-view="connectionTestStatusView"
      @refresh-models="handleRefreshConnectionTestModels()"
      @test="handleTestConnection()"
    />

    <AccountCompareTestModal
      v-model="showCompareTestModal"
      :accounts="compareTestAccounts"
      @finished="refreshAccountsSilently()"
    />

    <AccountImportTasks
      v-model="showImportTasks"
      :tasks="recentImportTasks"
      :selected-id="importTaskId"
      :detail="importTaskDetail"
      :loading="loadingImportTasks"
      :stopping="stoppingImportTask"
      :error="importTaskError"
      @select="importTasks.select"
      @stop="importTasks.stop"
      @after-leave="importTasks.refresh(true)"
      @view-accounts="showImportTasks = false; loadAccounts()"
    />

    <AccountEmailAuth
      v-model="showEmailAuth"
      :batch="emailAuthBatch"
      :accounts="accounts"
      :loading="loadingEmailAuth"
      :stopping="stoppingEmailAuth"
      :retrying-emails="retryingEmails"
      :error="emailAuthError"
      @stop="emailAuth.stop"
      @retry="emailAuth.retry"
      @after-leave="emailAuth.refresh(true)"
      @view-accounts="showEmailAuth = false; loadAccounts()"
    />

    <AccountCreateModal
      v-model="showCreateModal"
      v-model:form="createForm"
      v-model:callback="authorizationCallback"
      :authorization="authorization"
      :email-auth="emailAuthStatusView"
      :account="reauthorizingAccount"
      :groups="groups"
      :groups-loading="groupsLoading"
      :reauthorizing="Boolean(reauthorizingAccount)"
      :saving="creatingAccount"
      @create="handleCreate"
      @generate-oauth="handleAuthorizeOAuth"
      @open-email-auth="showCreateModal = false; showEmailAuth = true"
      @after-leave="clearCreate"
    />

    <AccountEditModal
      v-model="showEditModal"
      v-model:api-key="editingApiKey"
      v-model:oauth-transport="editingOAuthTransport"
      v-model:name="editingName"
      v-model:notes="editingNotes"
      v-model:enabled="schedulingEnabled"
      v-model:concurrency-limit="editingConcurrencyLimit"
      v-model:weight="editingWeight"
      v-model:model-access="editingModelAccess"
      v-model:proxy-mode="editingProxyMode"
      v-model:proxy-id="editingProxyId"
      v-model:selected-group-ids="editingGroupIds"
      :configuration-ready="configurationReady"
      :account="editingAccount"
      :groups="groups"
      :groups-loading="groupsLoading"
      :saving="savingAccountEdit"
      @save="saveAccountEdit"
      @after-leave="clearCredentials"
    />

    <AccountBatchEditModal
      v-model="showBatchEditModal"
      v-model:enabled="batchSchedulingEnabled"
      v-model:concurrency-limit="batchConcurrencyLimit"
      v-model:weight="batchWeight"
      v-model:model-access="batchModelAccess"
      v-model:proxy-mode="batchProxyMode"
      v-model:proxy-id="batchProxyId"
      v-model:selected-group-ids="batchGroupIds"
      :catalog-account-id="batchCatalogAccountId"
      :edited-fields="batchEditedFields"
      :selected-count="batchEditingCount"
      :groups="groups"
      :groups-loading="groupsLoading"
      :saving="savingBatchEdit"
      @save="saveBatchEdit"
    />

    <ZConfirmDialog
      v-model="showDeleteModal"
      :title="t('common.actions.delete')"
      :description="t('accounts.page.deleteDescription')"
      destructive
      :confirm-text="t('common.actions.delete')"
      :loading="batchDeleting"
      @confirm="handleBatchDelete"
    >
      <p class="m-0">
        {{ t('accounts.page.batchDeleteDescription', { count: deleteCount }) }}
      </p>
    </ZConfirmDialog>

    <ZConfirmDialog
      v-model="showSingleDeleteModal"
      :title="t('accounts.actions.deleteAccount')"
      :description="t('accounts.page.deleteDescription')"
      destructive
      :confirm-text="t('common.actions.delete')"
      :loading="deletingAccount"
      @confirm="handleDelete"
    >
      <p class="m-0">
        {{ t('accounts.page.delete', { value: pendingDeleteAccount ? accountIdentityLabel(pendingDeleteAccount) : t('accounts.page.thisAccount') }) }}
      </p>
    </ZConfirmDialog>
  </div>
</template>
