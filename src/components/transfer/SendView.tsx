// src/components/transfer/SendView.tsx
import { useTranslation } from 'react-i18next';
import { Shield, Edit3, Clipboard, AlertTriangle, RefreshCw } from 'lucide-react';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { HelpIcon } from '../ui/HelpIcon';
import { PageLayout } from '../ui/PageLayout';
import { AlertBanner } from '../ui/AlertBanner';
import { Advanced } from '../ui/Advanced';
import { Disclosure } from '../ui/Disclosure';
import { TechId } from '../ui/TechId';
import { UserAvatar } from '../ui/UserAvatar';
import { RecipientSelector } from '../send/RecipientSelector';
import { PrivacyToggle } from '../send/PrivacyToggle';
import { AssetInventory } from '../send/AssetInventory';
import { useSendTransfer } from './useSendTransfer';
import { SendConfirmPanel } from './SendConfirmPanel';
import { parseHumocoInput } from '../../utils/humocoUri';

export interface SendViewProps {
    initialRecipientId?: string;
    initialVoucherId?: string;
    initialNote?: string;
    initialPaymentUri?: string;
}

export function SendView({
    initialRecipientId,
    initialVoucherId,
    initialNote,
    initialPaymentUri,
}: SendViewProps = {}) {
    const { t } = useTranslation();
    const flow = useSendTransfer({
        initialRecipientId,
        initialVoucherId,
        initialNote,
        initialPaymentUri,
    });
    const { state, dispatch } = flow;

    return (
        <PageLayout
            title={t('transfer.title')}
            description={t('transfer.description')}
            onBack={flow.goBack}
            advancedActions={
                <Button
                    variant="secondary"
                    size="sm"
                    onClick={flow.handlePastePaymentRequest}
                    title={t('transfer.pastePaymentRequestHint')}
                    className="gap-2 text-xs shadow-sm"
                >
                    <Clipboard size={16} className="shrink-0" />
                    <span>{t('transfer.pastePaymentRequest')}</span>
                </Button>
            }
        >
            <div className="max-w-xl mx-auto space-y-5 pb-16">
                {flow.paymentRequestSuccessMsg && (
                    <AlertBanner variant="success">{flow.paymentRequestSuccessMsg}</AlertBanner>
                )}

                {state.feedbackMsg && (
                    <AlertBanner variant="error">{state.feedbackMsg}</AlertBanner>
                )}

                {(flow.appConfig?.lifecycle.pendingRestoreL2Reconciliation ||
                    flow.appConfig?.lifecycle.walletSyncAlert?.kind === 'stale_local_backup' ||
                    flow.appConfig?.lifecycle.walletSyncAlert?.kind === 'fork_detected') && (
                    <div className="p-4 rounded-2xl bg-amber-50 border border-amber-200 text-amber-900 text-xs space-y-2 mb-4">
                        <div className="flex items-center gap-2 font-bold text-amber-800">
                            <AlertTriangle size={16} className="shrink-0" />
                            <span>
                                {flow.appConfig?.lifecycle.walletSyncAlert?.kind ===
                                'fork_detected'
                                    ? t(
                                          'dashboard.syncAlertForkTitle',
                                          'Abweichender Synchronisationsstand erkannt',
                                      )
                                    : flow.appConfig?.lifecycle.walletSyncAlert?.kind ===
                                        'stale_local_backup'
                                      ? t(
                                            'dashboard.syncAlertStaleTitle',
                                            'Veralteter Profilstand erkannt',
                                        )
                                      : t(
                                            'transfer.pendingL2ReconciliationTitle',
                                            'Online-Sicherheitsabgleich erforderlich',
                                        )}
                            </span>
                        </div>
                        <p className="leading-relaxed">
                            {flow.appConfig?.lifecycle.walletSyncAlert?.kind ===
                                'stale_local_backup' ||
                            flow.appConfig?.lifecycle.walletSyncAlert?.kind ===
                                'fork_detected'
                                ? t(
                                      'transfer.pendingL2ReconciliationStaleDesc',
                                      'Auf einem Sync-Dienst wurde ein neuerer oder abweichender Profilstand erkannt. Das Ausgeben von Gutscheinen ist gesperrt, bis Sie entweder die neuere Sicherung einspielen oder einen Online-Sicherheitsabgleich durchführen.',
                                  )
                                : t(
                                      'transfer.pendingL2ReconciliationDesc',
                                      'Nach einer Wiederherstellung aus der Sicherung ist vor dem Ausgeben von Gutscheinen ein einmaliger Online-Sicherheitsabgleich erforderlich, um SST-V4-Konflikte auszuschließen.',
                                  )}
                        </p>
                        <Button
                            variant="secondary"
                            size="sm"
                            disabled={flow.isReconcilingL2}
                            onClick={flow.handleReconcileL2}
                            className="w-full mt-1"
                        >
                            <RefreshCw
                                size={14}
                                className={flow.isReconcilingL2 ? 'animate-spin mr-2' : 'mr-2'}
                            />
                            {flow.isReconcilingL2
                                ? t('profile.backupRestoreReconcilingL2', 'Gleiche mit Layer 2 ab...')
                                : t('profile.backupRestoreReconcileL2Btn', 'Jetzt online abgleichen')}
                        </Button>
                    </div>
                )}

                <div className="space-y-5">
                    {!flow.isEditingSender ? (
                        <div className="flex items-center justify-between gap-3 bg-white border border-theme-subtle rounded-2xl px-4 py-3 shadow-xs">
                            <div className="flex items-center gap-3 min-w-0">
                                <UserAvatar
                                    size={28}
                                    name={
                                        state.sendProfileName
                                            ? state.customSenderName || flow.ownUserId || 'sender'
                                            : 'anonymous'
                                    }
                                />
                                <div className="min-w-0 flex items-baseline gap-2 flex-wrap">
                                    <span className="text-[10px] font-black uppercase tracking-widest text-theme-light shrink-0">
                                        {t('transfer.senderNameLabel')}:
                                    </span>
                                    <span className="text-sm font-bold text-theme-secondary truncate">
                                        {state.sendProfileName
                                            ? state.customSenderName.trim() ||
                                              t('transfer.noSenderNameProvided')
                                            : t('transfer.anonymous')}
                                    </span>
                                </div>
                            </div>
                            <div className="flex items-center gap-1 shrink-0">
                                <HelpIcon topic="senderName" size={14} />
                                <button
                                    type="button"
                                    data-testid="edit-sender-name-btn"
                                    onClick={() => flow.setIsEditingSender(true)}
                                    aria-label={t('common.edit')}
                                    title={t('common.edit')}
                                    className="p-1.5 text-theme-light hover:text-theme-primary hover:bg-theme-primary/5 rounded-xl transition-colors cursor-pointer"
                                >
                                    <Edit3 size={15} />
                                </button>
                            </div>
                        </div>
                    ) : (
                        <div className="bg-white border border-theme-primary/30 rounded-2xl p-4 shadow-xs space-y-3">
                            <div className="flex items-center justify-between gap-2 flex-wrap">
                                <div className="flex items-center gap-1.5">
                                    <span className="text-[10px] font-black uppercase tracking-widest text-theme-primary">
                                        {t('transfer.senderNameLabel')}
                                    </span>
                                    <HelpIcon topic="senderName" size={14} />
                                </div>
                                <div className="flex items-center gap-3">
                                    <label className="inline-flex items-center gap-1.5 text-[11px] font-semibold text-theme-light hover:text-theme-secondary cursor-pointer select-none">
                                        <input
                                            type="checkbox"
                                            checked={!state.sendProfileName}
                                            onChange={(e) =>
                                                dispatch({
                                                    type: 'TOGGLE_PROFILE_NAME',
                                                    value: !e.target.checked,
                                                })
                                            }
                                            className="rounded border-slate-300 accent-theme-primary h-3.5 w-3.5 cursor-pointer"
                                        />
                                        <span>{t('transfer.sendAnonymously')}</span>
                                    </label>
                                    <Button
                                        type="button"
                                        size="xs"
                                        variant="secondary"
                                        onClick={() => flow.setIsEditingSender(false)}
                                        data-testid="save-sender-name-btn"
                                    >
                                        {t('common.done')}
                                    </Button>
                                </div>
                            </div>
                            {state.sendProfileName ? (
                                <div className="flex items-center gap-3">
                                    <UserAvatar
                                        size={30}
                                        name={state.customSenderName || flow.ownUserId || 'sender'}
                                    />
                                    <Input
                                        value={state.customSenderName}
                                        onChange={(e) =>
                                            dispatch({ type: 'SET_SENDER_NAME', name: e.target.value })
                                        }
                                        placeholder={t('transfer.senderNamePlaceholder')}
                                        className="bg-white border-slate-200 text-sm font-bold rounded-xl py-2 px-3"
                                    />
                                </div>
                            ) : (
                                <div className="flex items-center gap-2.5 py-1.5 px-3 bg-slate-50 border border-slate-200/60 rounded-xl text-xs text-slate-500">
                                    <Shield size={15} className="text-slate-400 shrink-0" />
                                    <span>{t('transfer.anonymousHint')}</span>
                                </div>
                            )}
                            <Advanced>
                                {flow.ownUserId && <TechId value={flow.ownUserId} kind="did" />}
                            </Advanced>
                        </div>
                    )}

                    <RecipientSelector
                        recipientId={state.recipientId}
                        onRecipientChange={(e) => {
                            const rawVal = e.target.value;
                            const parsed = parseHumocoInput(rawVal);
                            if (parsed) {
                                void flow.handleApplyRawCode(rawVal);
                                return;
                            }
                            if (rawVal.toLowerCase().includes('humoco://pay')) {
                                dispatch({
                                    type: 'SET_FEEDBACK',
                                    msg: t('transfer.invalidPaymentRequest'),
                                });
                                return;
                            }
                            const val = rawVal.replace(/\s+/g, '');
                            e.target.value = val;
                            dispatch({ type: 'SET_RECIPIENT', id: val });
                        }}
                        onApplyRawCode={flow.handleApplyRawCode}
                        onSelectContact={(c) => dispatch({ type: 'SET_RECIPIENT', id: c.did })}
                        contacts={flow.contacts}
                        trustStatus={flow.trustStatus}
                        recipientError={state.recipientError}
                        onClearRecipient={() => dispatch({ type: 'SET_RECIPIENT', id: '' })}
                    />

                    <AssetInventory
                        filteredVouchers={flow.filteredVouchers}
                        activeAssetClasses={flow.activeAssetClasses}
                        selectedStandardId={state.selectedStandardId}
                        selectedIsTest={state.selectedIsTest}
                        onStandardSelect={flow.handleStandardSelect}
                        targetAmountStr={state.targetAmountStr}
                        onTargetAmountChange={flow.handleTargetAmountChange}
                        selection={flow.selectedMap}
                        onVoucherToggle={flow.handleVoucherToggle}
                        onAmountChange={flow.handleAmountChange}
                        uuidToPrecisionMap={flow.uuidToPrecisionMap}
                        standardIdToUuidMap={flow.standardIdToUuidMap}
                        currentPlan={flow.currentPlan}
                        recipientSelected={state.recipientId.trim().length >= 10}
                        onCreateVoucher={() => flow.navigate({ view: 'create_voucher' })}
                        onReceiveVoucher={() => flow.navigate({ view: 'receive_bundle' })}
                    />

                    <div className="bg-white border border-theme-subtle rounded-2xl px-4 py-3 shadow-xs space-y-1.5">
                        <label
                            htmlFor="transfer-note-input"
                            className="text-[10px] font-black text-theme-light uppercase tracking-widest block"
                        >
                            {t('transfer.transactionNote')}
                        </label>
                        <Input
                            id="transfer-note-input"
                            value={state.notes}
                            onChange={(e) => dispatch({ type: 'SET_NOTES', notes: e.target.value })}
                            placeholder={t('transfer.notePlaceholder')}
                            aria-label={t('transfer.transactionNote')}
                            className="rounded-xl text-sm"
                        />
                    </div>

                    <Disclosure
                        defaultOpen={false}
                        open={state.privacyError ? true : undefined}
                        title={t('transfer.moreOptionsTitle')}
                        indicatorDot={flow.sendCardOverride !== null}
                        toggleTestId="send-more-options-toggle"
                        className="border border-theme-subtle rounded-2xl overflow-hidden bg-white/50"
                        buttonClassName="w-full flex items-center justify-between gap-3 px-4 py-3 bg-slate-50/70 hover:bg-slate-100/70 transition-colors text-left cursor-pointer text-xs font-bold text-theme-secondary"
                        contentClassName="p-4 border-t border-theme-subtle bg-white"
                    >
                        <div className="space-y-4">
                            <PrivacyToggle
                                privacyMode={state.privacyMode}
                                onPrivacyChange={(mode) => dispatch({ type: 'SET_PRIVACY', mode })}
                                privacyRules={{
                                    mode: flow.detectedPrivacy,
                                    forced:
                                        flow.detectedPrivacy === 'stealth' ||
                                        flow.detectedPrivacy === 'public',
                                }}
                                privacyError={state.privacyError}
                            />

                            {flow.hasPublishedCard ? (
                                <div className="flex items-start gap-3 p-3.5 bg-white border border-theme-subtle rounded-2xl">
                                    <input
                                        type="checkbox"
                                        id="sendCardToggle"
                                        checked={
                                            flow.sendCardOverride !== null
                                                ? flow.sendCardOverride
                                                : (flow.appConfig?.uiPreferences.marketSendCardOnPayment ?? false)
                                        }
                                        onChange={(e) => flow.setSendCardOverride(e.target.checked)}
                                        className="mt-0.5 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary"
                                    />
                                    <div className="flex-1">
                                        <label
                                            htmlFor="sendCardToggle"
                                            className="text-xs font-bold text-theme-secondary block cursor-pointer"
                                        >
                                            {t('transfer.sendCardTitle')}
                                        </label>
                                        <p className="text-[11px] text-theme-light mt-0.5">
                                            {t('transfer.sendCardDesc')}
                                        </p>
                                    </div>
                                </div>
                            ) : (
                                <div className="p-3 bg-slate-50 border border-theme-subtle/60 rounded-2xl text-[11px] text-theme-light flex flex-col sm:flex-row sm:items-center justify-between gap-2">
                                    <span>{t('transfer.sendCardNoCardNotice')}</span>
                                    <button
                                        type="button"
                                        onClick={() =>
                                            flow.navigate({
                                                view: 'marketplace',
                                                initialTab: 'mine',
                                                initialOpenEditor: true,
                                            })
                                        }
                                        className="text-xs text-theme-primary hover:underline shrink-0 self-start sm:self-auto p-0 h-auto font-medium"
                                    >
                                        {t('transfer.openMarketplace')}
                                    </button>
                                </div>
                            )}
                        </div>
                    </Disclosure>

                    <SendConfirmPanel
                        recipientId={state.recipientId}
                        contacts={flow.contacts}
                        selectedMap={flow.selectedMap}
                        selectionStats={flow.selectionStats}
                        availableVouchers={flow.availableVouchers}
                        uuidToPrecisionMap={flow.uuidToPrecisionMap}
                        privacyMode={state.privacyMode}
                        isProcessing={state.isProcessing}
                        showConfirm={state.showConfirm}
                        onPrepareTransfer={flow.handlePrepareTransfer}
                        onConfirmExecute={flow.executeTransfer}
                        onCancelConfirm={() => dispatch({ type: 'TOGGLE_CONFIRM', value: false })}
                    />
                </div>
            </div>
        </PageLayout>
    );
}
