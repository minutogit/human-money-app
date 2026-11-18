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
import type { useNavigation } from '../../../context/NavigationContext';
import type {
    AggregatedBalance,
    FrontendPendingSignRequest,
    FrontendPendingWotHandshake,
    IntegrityReport,
    RecoveryProtectionStatusDto,
} from '../../../types';

type NavigateFn = ReturnType<typeof useNavigation>['navigate'];

interface DashboardBannersProps {
    balances: AggregatedBalance[];
    activeVouchersCount: number;
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
    accessMode: string;
    resolveIdentity: (did?: string | null, providedName?: string | null) => string;
    onNavigate: NavigateFn;
    onOpenSignRequest: (req: FrontendPendingSignRequest) => void;
    onDismissSignRequest: (msgId: string) => void;
    onOpenIntegrityModal: () => void;
    onSnoozeRehearsal: () => void;
    onOpenRehearsalModal: () => void;
}

export function DashboardBanners({
    balances,
    activeVouchersCount,
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
    accessMode,
    resolveIdentity,
    onNavigate,
    onOpenSignRequest,
    onDismissSignRequest,
    onOpenIntegrityModal,
    onSnoozeRehearsal,
    onOpenRehearsalModal,
}: DashboardBannersProps) {
    const { t } = useTranslation();

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

    return (
        <div className="space-y-3">
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
                !rehearsalSnoozed &&
                accessMode !== 'receiveOnly' && (
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
                                    onSnoozeRehearsal();
                                    void recoveryService
                                        .snoozeRecoveryRehearsal?.(24)
                                        .catch(() => undefined);
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
