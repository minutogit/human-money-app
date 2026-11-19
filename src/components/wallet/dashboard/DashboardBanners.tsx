// src/components/wallet/dashboard/DashboardBanners.tsx
import { useTranslation } from 'react-i18next';
import {
    ArrowUpRight,
    ShieldAlert,
    Clock,
    AlertCircle,
    Archive,
    FileSignature,
    UserCheck,
    X,
    RefreshCw,
    MessageSquare,
    Brain,
} from 'lucide-react';
import { Button } from '../../ui/Button';
import { Card } from '../../ui/Card';
import { truncateUserId } from '../../../utils/userIdHelper';
import { recoveryService } from '../../../services/recoveryService';
import { useOptionalSession } from '../../../context/SessionContext';
import type { useNavigation } from '../../../context/NavigationContext';
import type {
    AggregatedBalance,
    FrontendPendingSignRequest,
    FrontendPendingWotHandshake,
    IntegrityReport,
    RecoveryProtectionStatusDto,
    WalletSyncAlert,
} from '../../../types';

type NavigateFn = ReturnType<typeof useNavigation>['navigate'];

interface DashboardBannersProps {
    balances: AggregatedBalance[];
    activeVouchersCount: number;
    incompleteCount?: number;
    firstIncompleteVoucherId?: string | null;
    quarantinedCount: number;
    expiringSoonCount: number;
    pendingSignRequests: FrontendPendingSignRequest[];
    pendingWotHandshakes: FrontendPendingWotHandshake[];
    openSellerInquiriesCount: number;
    isOpeningSignRequest: string | null;
    integrityReport: IntegrityReport | null;
    protectionStatus: RecoveryProtectionStatusDto | null;
    showBackupReminder: boolean;
    rehearsalSnoozed: boolean;
    resolveIdentity: (did?: string | null, providedName?: string | null) => string;
    onNavigate: NavigateFn;
    onOpenSignRequest: (req: FrontendPendingSignRequest) => void;
    onDismissSignRequest: (msgId: string) => void;
    onOpenIntegrityModal: () => void;
    onSnoozeRehearsal: () => void;
    onOpenRehearsalModal: () => void;
    walletSyncAlert?: WalletSyncAlert | null;
    isReconcilingSyncAlert?: boolean;
    onReconcileL2?: () => void;
    onDismissSyncAlert?: () => void;
}

export function DashboardBanners({
    balances,
    activeVouchersCount,
    incompleteCount = 0,
    firstIncompleteVoucherId = null,
    quarantinedCount,
    expiringSoonCount,
    pendingSignRequests,
    pendingWotHandshakes,
    openSellerInquiriesCount,
    isOpeningSignRequest,
    integrityReport,
    protectionStatus,
    showBackupReminder,
    rehearsalSnoozed,
    resolveIdentity,
    onNavigate,
    onOpenSignRequest,
    onDismissSignRequest,
    onOpenIntegrityModal,
    onSnoozeRehearsal,
    onOpenRehearsalModal,
    walletSyncAlert,
    isReconcilingSyncAlert = false,
    onReconcileL2,
    onDismissSyncAlert,
}: DashboardBannersProps) {
    const { t } = useTranslation();
    const session = useOptionalSession();

    const getIntegrityDetails = (report: IntegrityReport) => {
        switch (report.type) {
            case 'integrityOutdated':
                return {
                    title: t('dashboard.warningOutdatedTitle', 'Veralteter Profil-Stand erkannt'),
                    desc: t(
                        'dashboard.warningOutdatedDesc',
                        'Dein lokaler Datenstand passt nicht mehr zum gesicherten Profil-Siegel. Ohne Reparatur können Zahlungen fehlschlagen.',
                    ),
                };
            case 'manipulatedItems':
            case 'missingItems':
                return {
                    title: t(
                        'dashboard.warningConflictTitle',
                        'Gutschein-Konflikt oder Datenabweichung erkannt',
                    ),
                    desc: t(
                        'dashboard.warningConflictDesc',
                        'Gutscheindaten weichen vom kryptographischen Nachweis ab. Repariere dein Profil jetzt, um Datenverlust zu vermeiden.',
                    ),
                };
            case 'invalidSignature':
                return {
                    title: t('dashboard.warningInvalidSigTitle', 'Ungültige Profil-Signatur erkannt'),
                    desc: t(
                        'dashboard.warningInvalidSigDesc',
                        'Die Signatur deines Profil-Zustands stimmt nicht überein. Zahlungen sind bis zur Reparatur gesperrt.',
                    ),
                };
            case 'unknownItems':
            case 'missingIntegrityRecord':
            default:
                return {
                    title: t('dashboard.warningSealTitle', 'Profil-Integrität muss aktualisiert werden'),
                    desc: t(
                        'dashboard.warningSealDesc',
                        'Für dein Profil liegt noch kein aktuelles Siegel vor. Aktualisiere es jetzt, um dein Profil zu schützen.',
                    ),
                };
        }
    };

    const syncAlertLocalState = walletSyncAlert
        ? `E${walletSyncAlert.localEpoch}/#${walletSyncAlert.localTxNonce}`
        : '';
    const syncAlertRemoteState =
        walletSyncAlert &&
        walletSyncAlert.remoteEpoch != null &&
        walletSyncAlert.remoteTxNonce != null
            ? `E${walletSyncAlert.remoteEpoch}/#${walletSyncAlert.remoteTxNonce}`
            : '—';
    const isCriticalSyncAlert =
        walletSyncAlert?.kind === 'stale_local_backup' ||
        walletSyncAlert?.kind === 'fork_detected';

    return (
        <div className="space-y-3">
            {walletSyncAlert && (
                <div
                    data-testid="wallet-sync-alert-banner"
                    className={`rounded-2xl border p-4 shadow-sm ${
                        isCriticalSyncAlert
                            ? 'bg-rose-50 border-rose-200 text-rose-950'
                            : 'bg-amber-50 border-amber-200 text-amber-950'
                    }`}
                >
                    <div className="flex items-start gap-3">
                        <div
                            className={`mt-0.5 rounded-xl p-2 shrink-0 ${
                                isCriticalSyncAlert
                                    ? 'bg-rose-100 text-rose-700'
                                    : 'bg-amber-100 text-amber-700'
                            }`}
                        >
                            {isCriticalSyncAlert ? (
                                <ShieldAlert size={18} />
                            ) : (
                                <AlertCircle size={18} />
                            )}
                        </div>
                        <div className="min-w-0 flex-1 space-y-1.5">
                            <div className="flex items-start justify-between gap-2">
                                <h3 className="text-sm font-bold">
                                    {walletSyncAlert.kind === 'stale_local_backup'
                                        ? t(
                                              'dashboard.syncAlertStaleTitle',
                                              'Veralteter Profilstand erkannt',
                                          )
                                        : walletSyncAlert.kind === 'fork_detected'
                                          ? t(
                                                'dashboard.syncAlertForkTitle',
                                                'Abweichender Synchronisationsstand erkannt',
                                            )
                                          : walletSyncAlert.kind === 'server_rollback'
                                            ? t(
                                                  'dashboard.syncAlertRollbackTitle',
                                                  'Zurückgesetzter Sync-Dienst erkannt',
                                              )
                                            : t(
                                                  'dashboard.syncAlertDataLossTitle',
                                                  'Unerwarteter Datenverlust auf dem Sync-Dienst',
                                              )}
                                </h3>
                                {!isCriticalSyncAlert && onDismissSyncAlert && (
                                    <button
                                        type="button"
                                        onClick={onDismissSyncAlert}
                                        className="rounded-lg p-1 text-amber-700 hover:bg-amber-100 transition-colors"
                                        aria-label={t('dashboard.syncAlertDismiss', 'Bestätigen')}
                                    >
                                        <X size={16} />
                                    </button>
                                )}
                            </div>
                            <p className="text-xs leading-relaxed opacity-90">
                                {walletSyncAlert.kind === 'stale_local_backup'
                                    ? t('dashboard.syncAlertStaleDesc', {
                                          url: walletSyncAlert.mirrorWriteUrl,
                                          remoteState: syncAlertRemoteState,
                                          localState: syncAlertLocalState,
                                      })
                                    : walletSyncAlert.kind === 'fork_detected'
                                      ? t('dashboard.syncAlertForkDesc', {
                                            url: walletSyncAlert.mirrorWriteUrl,
                                            remoteState: syncAlertRemoteState,
                                            localState: syncAlertLocalState,
                                        })
                                      : walletSyncAlert.kind === 'server_rollback'
                                        ? t('dashboard.syncAlertRollbackDesc', {
                                              url: walletSyncAlert.mirrorWriteUrl,
                                              remoteState: syncAlertRemoteState,
                                              localState: syncAlertLocalState,
                                          })
                                        : t('dashboard.syncAlertDataLossDesc', {
                                              url: walletSyncAlert.mirrorWriteUrl,
                                              localState: syncAlertLocalState,
                                          })}
                            </p>
                            {walletSyncAlert.hasSignedAckProof &&
                                (walletSyncAlert.kind === 'server_rollback' ||
                                    walletSyncAlert.kind === 'unexpected_data_loss') && (
                                    <p className="text-[11px] font-medium opacity-80">
                                        {t(
                                            'dashboard.syncAlertAckProof',
                                            'Signierte Empfangsbestätigung des Sync-Dienstes liegt als Nachweis vor.',
                                        )}
                                    </p>
                                )}
                            <div className="flex flex-wrap items-center gap-2 pt-1">
                                {isCriticalSyncAlert ? (
                                    <>
                                        {onReconcileL2 && (
                                            <button
                                                type="button"
                                                onClick={onReconcileL2}
                                                disabled={isReconcilingSyncAlert}
                                                className="inline-flex items-center gap-1.5 rounded-xl bg-rose-700 px-3 py-1.5 text-xs font-semibold text-white hover:bg-rose-800 disabled:opacity-60 transition-colors"
                                            >
                                                <RefreshCw
                                                    size={13}
                                                    className={
                                                        isReconcilingSyncAlert ? 'animate-spin' : ''
                                                    }
                                                />
                                                {t(
                                                    'dashboard.syncAlertReconcileBtn',
                                                    'Gutscheine jetzt abgleichen',
                                                )}
                                            </button>
                                        )}
                                        <button
                                            type="button"
                                            onClick={() =>
                                                onNavigate({
                                                    view: 'settings',
                                                    initialSection: 'backup',
                                                })
                                            }
                                            className="inline-flex items-center gap-1.5 rounded-xl border border-rose-300 bg-white px-3 py-1.5 text-xs font-semibold text-rose-900 hover:bg-rose-100/70 transition-colors"
                                        >
                                            {t(
                                                'dashboard.syncAlertOpenBackup',
                                                'Sicherung öffnen',
                                            )}
                                            <ArrowUpRight size={13} />
                                        </button>
                                    </>
                                ) : (
                                    <>
                                        <button
                                            type="button"
                                            onClick={() =>
                                                onNavigate({
                                                    view: 'settings',
                                                    initialSection: 'sync',
                                                })
                                            }
                                            className="inline-flex items-center gap-1.5 rounded-xl bg-amber-700 px-3 py-1.5 text-xs font-semibold text-white hover:bg-amber-800 transition-colors"
                                        >
                                            {t(
                                                'dashboard.syncAlertOpenSync',
                                                'Sync-Dienste prüfen',
                                            )}
                                            <ArrowUpRight size={13} />
                                        </button>
                                        {onDismissSyncAlert && (
                                            <button
                                                type="button"
                                                onClick={onDismissSyncAlert}
                                                className="inline-flex items-center gap-1.5 rounded-xl border border-amber-300 bg-white px-3 py-1.5 text-xs font-semibold text-amber-900 hover:bg-amber-100/70 transition-colors"
                                            >
                                                {t(
                                                    'dashboard.syncAlertDismiss',
                                                    'Hinweis ausblenden',
                                                )}
                                            </button>
                                        )}
                                    </>
                                )}
                            </div>
                        </div>
                    </div>
                </div>
            )}
            {incompleteCount > 0 && (
                <Card
                    variant="accent"
                    className="py-3 px-4 flex flex-col sm:flex-row sm:items-center justify-between gap-3 border-l-4 border-l-theme-accent cursor-pointer"
                    hover
                    onClick={() =>
                        onNavigate(
                            incompleteCount === 1 && firstIncompleteVoucherId
                                ? {
                                      view: 'voucher_details',
                                      voucherId: firstIncompleteVoucherId,
                                      previousView: { view: 'logged_in' },
                                  }
                                : { view: 'wallet', initialStatusFilter: 'incomplete' },
                        )
                    }
                    data-testid="incomplete-drafts-resume-banner"
                >
                    <div className="flex items-center gap-3 min-w-0">
                        <div className="p-2 rounded-xl bg-theme-accent text-white shrink-0">
                            <Clock size={20} />
                        </div>
                        <div className="min-w-0">
                            <p className="font-black text-theme-secondary uppercase tracking-wider text-[10px]">
                                {t('auth.actionRequired')}
                            </p>
                            <p className="text-sm font-bold text-theme-primary truncate">
                                {t('dashboard.needSignatures', { count: incompleteCount })}
                            </p>
                        </div>
                    </div>
                    <Button variant="primary" size="xs" className="rounded-full px-4 shrink-0 self-end sm:self-center">
                        {t('common.resume')}
                    </Button>
                </Card>
            )}
            {pendingSignRequests.map((req) => {
                const senderName =
                    req.senderName ||
                    resolveIdentity(req.senderDid, null) ||
                    truncateUserId(req.senderDid);
                const isOpening = isOpeningSignRequest === req.msgId;
                return (
                    <Card
                        key={req.msgId}
                        variant="glass"
                        className="py-3 px-4 flex flex-col sm:flex-row sm:items-center justify-between gap-3 border-l-4 border-l-teal-500 bg-teal-50/40"
                        data-testid="pending-sign-request-card"
                    >
                        <div className="flex items-center gap-3 min-w-0">
                            <div className="p-2 rounded-xl bg-teal-500 text-white shrink-0">
                                <FileSignature size={20} />
                            </div>
                            <div className="min-w-0">
                                <p className="font-bold text-sm text-theme-secondary truncate">
                                    {t('dashboard.pendingSignRequestTitle', { name: senderName })}
                                </p>
                                <p className="text-xs text-theme-light truncate">
                                    {t('dashboard.pendingSignRequestDesc')}
                                </p>
                            </div>
                        </div>
                        <div className="flex items-center gap-2 shrink-0 self-end sm:self-center">
                            <Button
                                variant="outline"
                                size="xs"
                                disabled={isOpening}
                                onClick={() => onDismissSignRequest(req.msgId)}
                                data-testid="dismiss-sign-request-button"
                                className="text-theme-light hover:text-rose-600 gap-1"
                            >
                                <X size={12} />
                                <span>{t('common.dismiss', 'Verwerfen')}</span>
                            </Button>
                            <Button
                                variant="primary"
                                size="xs"
                                disabled={isOpening}
                                onClick={() => onOpenSignRequest(req)}
                                data-testid="open-sign-request-button"
                                className="gap-1 bg-teal-600 hover:bg-teal-700 text-white"
                            >
                                {isOpening ? (
                                    <RefreshCw size={12} className="animate-spin" />
                                ) : (
                                    <FileSignature size={12} />
                                )}
                                <span>{t('dashboard.checkAndSign', 'Prüfen & Unterschreiben')}</span>
                            </Button>
                        </div>
                    </Card>
                );
            })}

            {pendingWotHandshakes.length > 0 && (
                <Card
                    variant="glass"
                    className="py-2.5 px-4 flex items-center justify-between border-l-4 border-l-sky-500 cursor-pointer bg-sky-50/40"
                    hover
                    onClick={() => onNavigate({ view: 'address_book' })}
                    data-testid="pending-wot-handshake-dashboard-card"
                >
                    <div className="flex items-center gap-4">
                        <div className="p-2 rounded-xl bg-sky-500 text-white shrink-0">
                            <UserCheck size={20} />
                        </div>
                        <div>
                            <p className="font-bold text-sm text-theme-secondary">
                                {t('dashboard.pendingWotHandshakesCount', {
                                    count: pendingWotHandshakes.length,
                                })}
                            </p>
                            <p className="text-xs opacity-70">{t('dashboard.pendingWotHandshakesHint')}</p>
                        </div>
                    </div>
                    <ArrowUpRight size={18} className="opacity-30" />
                </Card>
            )}

            {openSellerInquiriesCount > 0 && (
                <Card
                    variant="glass"
                    className="py-2.5 px-4 flex items-center justify-between border-l-4 border-l-emerald-500 cursor-pointer bg-emerald-50/40"
                    hover
                    onClick={() => onNavigate({ view: 'marketplace', initialTab: 'mine' })}
                    data-testid="pending-seller-inquiries-dashboard-card"
                >
                    <div className="flex items-center gap-4">
                        <div className="p-2 rounded-xl bg-emerald-600 text-white shrink-0">
                            <MessageSquare size={20} />
                        </div>
                        <div>
                            <p className="font-bold text-sm text-theme-secondary">
                                {t('dashboard.openSellerInquiriesCount', {
                                    count: openSellerInquiriesCount,
                                })}
                            </p>
                            <p className="text-xs opacity-70">{t('dashboard.openSellerInquiriesHint')}</p>
                        </div>
                    </div>
                    <ArrowUpRight size={18} className="opacity-30" />
                </Card>
            )}

            {integrityReport &&
                integrityReport.type !== 'valid' &&
                integrityReport.type !== 'missingIntegrityRecord' &&
                (() => {
                    const details = getIntegrityDetails(integrityReport);
                    return (
                        <Card
                            variant="glass"
                            className="py-3 px-4 flex flex-col sm:flex-row sm:items-center justify-between gap-3 border-l-4 border-l-amber-500 bg-amber-50/60"
                            data-testid="security-warning-card"
                        >
                            <div className="flex items-start gap-3 min-w-0">
                                <div className="p-2 rounded-xl shrink-0 mt-0.5 bg-amber-500 text-white">
                                    <ShieldAlert size={20} />
                                </div>
                                <div className="min-w-0 space-y-1">
                                    <p className="font-bold text-sm text-theme-secondary">{details.title}</p>
                                    <p className="text-xs text-theme-light">{details.desc}</p>
                                </div>
                            </div>
                            <Button
                                variant="primary"
                                size="xs"
                                onClick={onOpenIntegrityModal}
                                data-testid="fix-integrity-button"
                                className="shrink-0 self-end sm:self-center bg-amber-600 hover:bg-amber-700 text-white"
                            >
                                {t('dashboard.fixNow', 'Jetzt beheben')}
                            </Button>
                        </Card>
                    );
                })()}

            {quarantinedCount > 0 && (
                <Card
                    variant="glass"
                    className="py-2 px-4 flex items-center justify-between border-l-4 border-l-rose-500"
                    hover
                    onClick={() => onNavigate({ view: 'conflict_list' })}
                >
                    <div className="flex items-center gap-4">
                        <div className="p-2 rounded-xl bg-rose-500 text-white">
                            <AlertCircle size={20} />
                        </div>
                        <div>
                            <p className="font-bold text-sm text-theme-secondary">
                                {t('dashboard.quarantineLocked', { count: quarantinedCount })}
                            </p>
                            <p className="text-xs opacity-70">{t('dashboard.quarantineDesc')}</p>
                        </div>
                    </div>
                    <ArrowUpRight size={18} className="opacity-30" />
                </Card>
            )}

            {expiringSoonCount > 0 && (
                <Card
                    variant="glass"
                    className="py-2 px-4 flex items-center justify-between border-l-4 border-l-amber-500 cursor-pointer"
                    hover
                    onClick={() =>
                        onNavigate({
                            view: 'wallet',
                            initialStatusFilter: 'active',
                            initialFilter: 'expiringSoon',
                        })
                    }
                >
                    <div className="flex items-center gap-4">
                        <div className="p-2 rounded-xl bg-amber-500 text-white">
                            <Clock size={20} />
                        </div>
                        <div>
                            <p className="font-bold text-sm text-theme-secondary">
                                {t('dashboard.expiringSoonTitle', { count: expiringSoonCount })}
                            </p>
                            <p className="text-xs opacity-70">{t('dashboard.expiringSoonDesc')}</p>
                        </div>
                    </div>
                    <ArrowUpRight size={18} className="opacity-30" />
                </Card>
            )}

            {(activeVouchersCount > 0 || balances.length > 0) &&
                (!protectionStatus?.hasIdentityRecovery || !protectionStatus?.hasVoucherBackup) && (
                    <Card
                        variant="glass"
                        className="py-3 px-4 flex flex-wrap gap-3 items-center justify-between border-l-4 border-l-amber-500 bg-amber-50/40 dark:bg-amber-950/20"
                        hover
                        onClick={() =>
                            onNavigate({
                                view: 'settings',
                                initialSection: !protectionStatus?.hasIdentityRecovery
                                    ? 'security'
                                    : 'backup',
                            })
                        }
                        data-testid="protection-warning-banner"
                    >
                        <div className="flex items-center gap-4 min-w-0">
                            <div className="p-2 rounded-xl bg-amber-500 text-white shrink-0">
                                <ShieldAlert size={20} />
                            </div>
                            <div className="min-w-0">
                                <p className="font-bold text-sm text-theme-secondary truncate">
                                    {t('dashboard.protectionWarningTitle')}
                                </p>
                                <p className="text-xs text-theme-light line-clamp-2">
                                    {!protectionStatus?.hasIdentityRecovery &&
                                    !protectionStatus?.hasVoucherBackup
                                        ? t('dashboard.protectionWarningBoth')
                                        : protectionStatus?.hasIdentityRecovery &&
                                            !protectionStatus?.hasVoucherBackup
                                          ? t('dashboard.protectionWarningSyncOnly')
                                          : t('dashboard.protectionWarningIdentityOnly')}
                                </p>
                            </div>
                        </div>
                        <Button
                            variant="primary"
                            size="xs"
                            className="shadow-none shrink-0 bg-amber-500 hover:bg-amber-600 border-amber-600 text-white"
                        >
                            {t('dashboard.protectionWarningButton')}
                        </Button>
                    </Card>
                )}

            {showBackupReminder && (
                <Card
                    variant="glass"
                    className="py-2 px-4 flex flex-wrap gap-3 items-center justify-between border-l-4 border-l-theme-primary"
                    hover
                    onClick={() => onNavigate({ view: 'settings' })}
                >
                    <div className="flex items-center gap-4 min-w-0">
                        <div className="p-2 rounded-xl bg-theme-primary text-white shrink-0">
                            <Archive size={20} />
                        </div>
                        <div className="min-w-0">
                            <p className="font-bold text-sm text-theme-secondary truncate">
                                {t('dashboard.backupReminderTitle')}
                            </p>
                            <p className="text-xs opacity-70 truncate">
                                {t('dashboard.backupReminderDesc')}
                            </p>
                        </div>
                    </div>
                    <Button variant="primary" size="xs" className="shadow-none shrink-0">
                        {t('dashboard.backupReminderButton')}
                    </Button>
                </Card>
            )}

            {protectionStatus?.rehearsalDue &&
                protectionStatus?.questionIds &&
                !rehearsalSnoozed && (
                    <Card
                        variant="glass"
                        className="py-3 px-4 flex flex-wrap gap-3 items-center justify-between border-l-4 border-l-indigo-500 bg-indigo-50/40 dark:bg-indigo-950/20"
                        data-testid="rehearsal-reminder-banner"
                    >
                        <div className="flex items-center gap-4 min-w-0">
                            <div className="p-2 rounded-xl bg-indigo-600 text-white shrink-0">
                                <Brain size={20} />
                            </div>
                            <div className="min-w-0">
                                <p className="font-bold text-sm text-theme-secondary">
                                    {t('dashboard.rehearsalReminderTitle')}
                                </p>
                                <p className="text-xs text-theme-light">
                                    {t('dashboard.rehearsalReminderDesc')}
                                </p>
                            </div>
                        </div>
                        <div className="flex items-center gap-2 shrink-0">
                            <Button
                                type="button"
                                variant="secondary"
                                size="xs"
                                onClick={() => {
                                    const runSnooze = async () => {
                                        if (typeof recoveryService?.snoozeRecoveryRehearsal !== 'function') {
                                            onSnoozeRehearsal();
                                            return;
                                        }
                                        if (session?.protectAction) {
                                            let didPersist = false;
                                            await session.protectAction(async (pwd) => {
                                                await recoveryService.snoozeRecoveryRehearsal(
                                                    24,
                                                    pwd || undefined,
                                                );
                                                didPersist = true;
                                            });
                                            if (didPersist) {
                                                onSnoozeRehearsal();
                                            }
                                        } else {
                                            await recoveryService.snoozeRecoveryRehearsal(24);
                                            onSnoozeRehearsal();
                                        }
                                    };
                                    void runSnooze().catch(() => undefined);
                                }}
                                data-testid="rehearsal-snooze-btn"
                            >
                                {t('dashboard.rehearsalSnoozeButton')}
                            </Button>
                            <Button
                                type="button"
                                variant="primary"
                                size="xs"
                                onClick={onOpenRehearsalModal}
                                data-testid="rehearsal-start-btn"
                                className="bg-indigo-600 hover:bg-indigo-700 text-white"
                            >
                                {t('dashboard.rehearsalStartButton')}
                            </Button>
                        </div>
                    </Card>
                )}
        </div>
    );
}
