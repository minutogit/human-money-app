// src/components/wallet/Dashboard.tsx
import { useState } from 'react';
import { UserCircle } from 'lucide-react';
import { useContactResolver } from '../../hooks/useContactResolver';
import { IntegrityReportModal } from './IntegrityReportModal';
import { RecoveryRehearsalModal } from '../auth/RecoveryRehearsalModal';
import { FirstStepsChecklist } from './FirstStepsChecklist';
import { PageLayout } from '../ui/PageLayout';
import { Advanced } from '../ui/Advanced';
import { TechId } from '../ui/TechId';
import { useDashboardData } from './dashboard/useDashboardData';
import { DashboardActionsGrid } from './dashboard/DashboardActionsGrid';
import { DashboardBanners } from './dashboard/DashboardBanners';
import { BalanceSummaryCards } from './dashboard/BalanceSummaryCards';
import { ActiveVouchersDisclosure } from './dashboard/ActiveVouchersDisclosure';
import { RecentActivitySection } from './dashboard/RecentActivitySection';

export function Dashboard() {
    const { resolveIdentity } = useContactResolver();
    const data = useDashboardData();
    const [showIntegrityModal, setShowIntegrityModal] = useState(false);
    const [showRehearsalModal, setShowRehearsalModal] = useState(false);

    return (
        <PageLayout
            customHeader={
                <header className="flex-shrink-0 sticky top-0 z-20 px-4 sm:px-6 py-2.5 glass-panel border-b-0 rounded-b-3xl mb-5">
                    <div className="flex flex-wrap items-center justify-between gap-3 text-[10px] sm:text-xs font-bold text-theme-light">
                        <div className="flex items-center gap-3 min-w-0">
                            <span className="relative flex h-2 w-2 shrink-0">
                                <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75" />
                                <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500" />
                            </span>
                            <div className="flex items-center gap-1.5 px-2 py-1 bg-white/50 rounded-full border border-theme-subtle/30 shadow-inner-soft min-w-0">
                                <UserCircle size={12} className="text-theme-primary shrink-0" />
                                <span className="text-theme-secondary truncate">{data.profileName}</span>
                            </div>
                        </div>

                        <Advanced>
                            <div className="flex items-center gap-2 shrink-0">
                                <TechId value={data.userId} kind="did" />
                            </div>
                        </Advanced>
                    </div>
                </header>
            }
        >
            <div className="max-w-5xl mx-auto space-y-6 pb-24 md:pb-8">
                <DashboardActionsGrid
                    balances={data.balances}
                    feedbackMsg={data.feedbackMsg}
                    onProtectedNavigate={data.handleProtectedNavigate}
                    onNavigate={data.navigate}
                />

                <DashboardBanners
                    balances={data.balances}
                    activeVouchersCount={data.activeVouchersCount}
                    incompleteCount={data.incompleteCount}
                    firstIncompleteVoucherId={data.firstIncompleteVoucherId}
                    quarantinedCount={data.quarantinedCount}
                    expiringSoonCount={data.expiringSoonCount}
                    pendingSignRequests={data.pendingSignRequests}
                    pendingWotHandshakes={data.pendingWotHandshakes}
                    unsupportedMessages={data.unsupportedMessages}
                    openSellerInquiriesCount={data.openSellerInquiriesCount}
                    isOpeningSignRequest={data.isOpeningSignRequest}
                    integrityReport={data.integrityReport}
                    protectionStatus={data.protectionStatus}
                    walletSyncAlert={data.walletSyncAlert}
                    isReconcilingSyncAlert={data.isReconcilingSyncAlert}
                    showBackupReminder={data.showBackupReminder}
                    rehearsalSnoozed={data.rehearsalSnoozed}
                    resolveIdentity={resolveIdentity}
                    onNavigate={data.navigate}
                    onOpenSignRequest={data.handleOpenSignRequest}
                    onDismissSignRequest={data.handleDismissSignRequest}
                    onDismissUnsupportedPost={data.handleDismissUnsupportedPost}
                    onOpenIntegrityModal={() => setShowIntegrityModal(true)}
                    onSnoozeRehearsal={() => data.setRehearsalSnoozed(true)}
                    onOpenRehearsalModal={() => setShowRehearsalModal(true)}
                    onReconcileL2={data.handleReconcileSyncAlert}
                    onDismissSyncAlert={data.handleDismissSyncAlert}
                />

                <FirstStepsChecklist
                    hasBackup={Boolean(
                        data.appConfig?.lifecycle.lastBackupAt ||
                            data.appConfig?.securityPolicy.backupSyncFolder ||
                            data.protectionStatus?.hasVoucherBackup,
                    )}
                    hasContact={data.contactsCount > 0}
                    hasVoucher={
                        data.activeVouchersCount > 0 ||
                        data.incompleteCount > 0 ||
                        data.balances.length > 0
                    }
                    hasSync={data.syncConnectedCount > 0}
                    hasIdentityRecovery={Boolean(data.protectionStatus?.hasIdentityRecovery)}
                    userId={data.userId}
                    profileScope={data.profileScope}
                />

                <BalanceSummaryCards
                    isLoading={data.isLoading}
                    balances={data.balances}
                    voucherCountsByStandard={data.voucherCountsByStandard}
                    balanceViewMode={data.balanceViewMode}
                    onViewModeChange={data.handleViewModeChange}
                    onProtectedNavigate={data.handleProtectedNavigate}
                    onNavigate={data.navigate}
                />

                {!data.isLoading && (
                    <ActiveVouchersDisclosure
                        activeVouchers={data.activeVouchers}
                        resolveIdentity={resolveIdentity}
                        onNavigate={data.navigate}
                    />
                )}

                <RecentActivitySection
                    recentEvents={data.recentEvents}
                    resolveIdentity={resolveIdentity}
                    onNavigate={data.navigate}
                />
            </div>

            {showIntegrityModal && data.integrityReport && (
                <IntegrityReportModal
                    report={data.integrityReport}
                    onClose={() => setShowIntegrityModal(false)}
                />
            )}

            {showRehearsalModal && data.protectionStatus?.questionIds && (
                <RecoveryRehearsalModal
                    isOpen={showRehearsalModal}
                    questionIds={data.protectionStatus.questionIds}
                    onClose={() => setShowRehearsalModal(false)}
                    onVerified={() => {
                        data.setProtectionStatus((prev) =>
                            prev ? { ...prev, rehearsalDue: false } : prev,
                        );
                    }}
                    onResetQuestions={() => {
                        setShowRehearsalModal(false);
                        data.navigate({ view: 'settings', initialSection: 'security' });
                    }}
                />
            )}
        </PageLayout>
    );
}
