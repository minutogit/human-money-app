// src/components/wallet/dashboard/useDashboardData.ts
import { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { voucherService } from '../../../services/voucherService';
import { transferService } from '../../../services/transferService';
import { profileService } from '../../../services/profileService';
import { contactService } from '../../../services/contactService';
import { settingsService } from '../../../services/settingsService';
import { marketService } from '../../../services/marketService';
import { recoveryService } from '../../../services/recoveryService';
import { backupService } from '../../../services/backupService';
import { postService } from '../../../services/postService';
import { APP_EVENTS } from '../../../utils/appEvents';
import { useNavigation } from '../../../context/NavigationContext';
import { useSession } from '../../../context/SessionContext';
import { logger } from '../../../utils/log';
import { stringifyError, translateError } from '../../../utils/errorHelper';
import type {
    AggregatedBalance,
    AppConfigDto,
    VoucherSummary,
    WalletEvent,
    FrontendPendingSignRequest,
    FrontendPendingTolerancePayment,
    FrontendPendingWotHandshake,
    FrontendUnsupportedPostItem,
    RecoveryProtectionStatusDto,
} from '../../../types';

export function useDashboardData() {
    const { t } = useTranslation();
    const tRef = useRef(t);
    tRef.current = t;

    const { navigate } = useNavigation();
    const {
        profileName,
        folderName,
        integrityReport,
        hasCachedSession,
        protectAction,
    } = useSession();

    const [userId, setUserId] = useState('');
    const [balances, setBalances] = useState<AggregatedBalance[]>([]);
    const [activeVouchers, setActiveVouchers] = useState<VoucherSummary[]>([]);
    const [recentEvents, setRecentEvents] = useState<WalletEvent[]>([]);
    const [feedbackMsg, setFeedbackMsg] = useState('');
    const [activeVouchersCount, setActiveVouchersCount] = useState(0);
    const [incompleteCount, setIncompleteCount] = useState(0);
    const [firstIncompleteVoucherId, setFirstIncompleteVoucherId] = useState<string | null>(null);
    const [quarantinedCount, setQuarantinedCount] = useState(0);

    const [pendingSignRequests, setPendingSignRequests] = useState<FrontendPendingSignRequest[]>([]);
    const [pendingWotHandshakes, setPendingWotHandshakes] = useState<FrontendPendingWotHandshake[]>([]);
    const [pendingTolerancePayments, setPendingTolerancePayments] = useState<FrontendPendingTolerancePayment[]>([]);
    const [unsupportedMessages, setUnsupportedMessages] = useState<FrontendUnsupportedPostItem[]>([]);
    const [openSellerInquiriesCount, setOpenSellerInquiriesCount] = useState(0);
    const [isOpeningSignRequest, setIsOpeningSignRequest] = useState<string | null>(null);
    const [expiringSoonCount, setExpiringSoonCount] = useState(0);
    const [isLoading, setIsLoading] = useState(true);
    const [voucherCountsByStandard, setVoucherCountsByStandard] = useState<Record<string, number>>({});
    const [showBackupReminder, setShowBackupReminder] = useState(false);
    const [rehearsalSnoozed, setRehearsalSnoozed] = useState(false);
    const [isReconcilingSyncAlert, setIsReconcilingSyncAlert] = useState(false);
    const [appConfig, setAppConfig] = useState<AppConfigDto | null>(null);
    const [protectionStatus, setProtectionStatus] = useState<RecoveryProtectionStatusDto | null>(null);
    const [contactsCount, setContactsCount] = useState(0);
    const [syncConnectedCount, setSyncConnectedCount] = useState(0);
    const [balanceViewMode, setBalanceViewMode] = useState<'spendable' | 'net'>('spendable');
    const mountedRef = useRef(true);
    const fetchSeqRef = useRef(0);

    const fetchData = useCallback(async () => {
        const seq = ++fetchSeqRef.current;
        try {
            const [
                id,
                balanceList,
                ,
                voucherSummaries,
                ,
                contacts,
                eventHistory,
                userSettings,
                syncOverview,
                recoveryProtection,
            ] = await Promise.all([
                profileService.getUserId(),
                voucherService.getTotalBalanceByCurrency(),
                transferService.getHistory().catch(() => []),
                voucherService.getSummaries().catch(() => []),
                profileService.getProfile().catch(() => null),
                contactService.getContacts().catch(() => []),
                transferService.getEventHistory(0, 5).catch(() => []),
                settingsService.getConfig().catch(() => null),
                marketService.getSyncOverviewStatus().catch(() => null),
                recoveryService.getRecoveryProtectionStatus().catch(() => null),
            ]);
            if (!mountedRef.current || seq !== fetchSeqRef.current) return;

            setUserId(id);
            setBalances(balanceList || []);
            setRecentEvents(eventHistory || []);
            setContactsCount((contacts || []).length);
            setProtectionStatus(recoveryProtection);
            setSyncConnectedCount(syncOverview?.connectedCount || 0);

            const allSummaries = voucherSummaries || [];
            const activeVoucherList = allSummaries
                .filter((v: VoucherSummary) => v.status === 'active')
                .sort((a: VoucherSummary, b: VoucherSummary) => {
                    const aTime = a.validUntil
                        ? new Date(a.validUntil).getTime()
                        : Number.MAX_SAFE_INTEGER;
                    const bTime = b.validUntil
                        ? new Date(b.validUntil).getTime()
                        : Number.MAX_SAFE_INTEGER;
                    return aTime - bTime;
                });
            setActiveVouchers(activeVoucherList);

            const countsByStandard: Record<string, number> = {};
            activeVoucherList.forEach((v: VoucherSummary) => {
                if (v.voucherStandardUuid) {
                    countsByStandard[v.voucherStandardUuid] =
                        (countsByStandard[v.voucherStandardUuid] || 0) + 1;
                }
            });
            setVoucherCountsByStandard(countsByStandard);
            setActiveVouchersCount(activeVoucherList.length);
            const incompleteList = allSummaries.filter(
                (v: VoucherSummary) => v.status === 'incomplete',
            );
            setIncompleteCount(incompleteList.length);
            setFirstIncompleteVoucherId(
                incompleteList.length === 1 ? incompleteList[0].localInstanceId : null,
            );
            setQuarantinedCount(
                allSummaries.filter((v: VoucherSummary) => v.status === 'quarantined').length,
            );

            const now = new Date();
            const thirtyDays = new Date(now.getTime() + 30 * 24 * 60 * 60 * 1000);
            setExpiringSoonCount(
                allSummaries.filter((v: VoucherSummary) => {
                    if (v.status !== 'active' || !v.validUntil) return false;
                    const exp = new Date(v.validUntil);
                    return exp > now && exp <= thirtyDays;
                }).length,
            );

            setAppConfig(userSettings);
            const initialMode =
                (userSettings?.uiPreferences.balanceViewMode as 'spendable' | 'net') ||
                'spendable';
            setBalanceViewMode(initialMode);

            const hasRealActiveVouchers = allSummaries.some(
                (v: VoucherSummary) => v.status === 'active' && !v.isTestVoucher,
            );
            const effectiveBackupAt =
                [userSettings?.lifecycle.lastBackupAt, userSettings?.lifecycle.lastSyncBackupAt]
                    .filter((d): d is string => Boolean(d) && !isNaN(new Date(d as string).getTime()))
                    .sort((a, b) => new Date(b).getTime() - new Date(a).getTime())[0] ?? null;
            const isBackupOutdatedOrMissing =
                !effectiveBackupAt ||
                (() => {
                    const backupDate = new Date(effectiveBackupAt);
                    if (isNaN(backupDate.getTime())) return true;
                    const fourteenDaysAgo = new Date(Date.now() - 14 * 24 * 60 * 60 * 1000);
                    return backupDate < fourteenDaysAgo;
                })();
            setShowBackupReminder(
                hasRealActiveVouchers &&
                    !userSettings?.securityPolicy.backupSyncFolder &&
                    isBackupOutdatedOrMissing,
            );
        } catch (e) {
            if (!mountedRef.current || seq !== fetchSeqRef.current) return;
            console.error(`Failed to fetch dashboard data: ${stringifyError(e)}`);
            setFeedbackMsg(translateError(e, tRef.current));
        } finally {
            if (mountedRef.current && seq === fetchSeqRef.current) setIsLoading(false);
        }
    }, []);

    useEffect(() => {
        mountedRef.current = true;
        logger.info('Dashboard component displayed');
        void fetchData();

        const loadPostInbox = () => {
            postService
                .getPostInboxSummary()
                .then((summary) => {
                    if (!mountedRef.current) return;
                    setPendingSignRequests(summary.pendingSignRequests || []);
                    setPendingWotHandshakes(summary.pendingWotHandshakes || []);
                    setPendingTolerancePayments(summary.pendingTolerancePayments || []);
                    setUnsupportedMessages(summary.unsupportedMessages || []);
                    setOpenSellerInquiriesCount(
                        (summary.inquiries || []).filter(
                            (inq) => inq.unread || (inq.role === 'seller' && inq.status === 'open'),
                        ).length,
                    );
                })
                .catch(() => {});
        };

        const handlePostInboxUpdated = () => {
            loadPostInbox();
            void fetchData();
        };

        loadPostInbox();
        postService.syncPostChannels().catch(() => {});
        window.addEventListener(APP_EVENTS.POST_INBOX_UPDATED, handlePostInboxUpdated);
        return () => {
            mountedRef.current = false;
            window.removeEventListener(APP_EVENTS.POST_INBOX_UPDATED, handlePostInboxUpdated);
        };
    }, [fetchData]);

    const handleOpenSignRequest = useCallback(
        async (req: FrontendPendingSignRequest) => {
            setIsOpeningSignRequest(req.msgId);
            try {
                const voucherData = await voucherService.openSigningRequest(req.containerBytes);
                if (voucherData) {
                    navigate({
                        view: 'sign_request',
                        voucherData,
                        requestMsgId: req.msgId,
                        senderDid: req.senderDid,
                        anchorManifestJson: voucherData.anchorManifestJson ?? undefined,
                        anchorDelegationsJson: voucherData.anchorDelegationsJson ?? undefined,
                    });
                }
            } catch (e) {
                logger.error(`Failed to open signing request: ${stringifyError(e)}`);
                setFeedbackMsg(translateError(e, t));
            } finally {
                setIsOpeningSignRequest(null);
            }
        },
        [navigate, t],
    );

    const handleDismissSignRequest = useCallback(
        async (msgId: string) => {
            try {
                await protectAction(async (pwd) => {
                    await postService.dismissPendingPostItem('sign_request', msgId, pwd || undefined);
                });
                setPendingSignRequests((prev) => prev.filter((r) => r.msgId !== msgId));
            } catch (e) {
                logger.error(`Failed to dismiss sign request: ${stringifyError(e)}`);
                setFeedbackMsg(translateError(e, t));
            }
        },
        [protectAction, t],
    );

    const handleDismissUnsupportedPost = useCallback(
        async (msgId: string) => {
            try {
                await protectAction(async (pwd) => {
                    await postService.dismissPendingPostItem('unsupported', msgId, pwd || undefined);
                });
                setUnsupportedMessages((prev) => prev.filter((m) => m.msgId !== msgId));
            } catch (e) {
                logger.error(`Failed to dismiss unsupported PoST item: ${stringifyError(e)}`);
                setFeedbackMsg(translateError(e, t));
            }
        },
        [protectAction, t],
    );

    const handleConfirmTolerancePayment = useCallback(
        async (msgId: string): Promise<boolean> => {
            try {
                const payload = await protectAction(async (pwd) => {
                    return await postService.confirmPendingPostPayment(msgId, pwd || undefined);
                });
                if (!payload || !mountedRef.current) return false;
                setPendingTolerancePayments((prev) => prev.filter((p) => p.msgId !== msgId));
                await fetchData();
                return true;
            } catch (e) {
                if (!mountedRef.current) return false;
                logger.error(`Failed to confirm pending tolerance payment: ${stringifyError(e)}`);
                setFeedbackMsg(translateError(e, t));
                return false;
            }
        },
        [fetchData, protectAction, t],
    );

    const handleDismissTolerancePayment = useCallback(
        async (msgId: string) => {
            try {
                await protectAction(async (pwd) => {
                    await postService.dismissPendingPostItem('tolerance_payment', msgId, pwd || undefined);
                });
                setPendingTolerancePayments((prev) => prev.filter((p) => p.msgId !== msgId));
            } catch (e) {
                logger.error(`Failed to dismiss pending tolerance payment: ${stringifyError(e)}`);
                setFeedbackMsg(translateError(e, t));
            }
        },
        [protectAction, t],
    );

    const handleViewModeChange = useCallback(
        async (mode: 'spendable' | 'net') => {
            setBalanceViewMode(mode);
            if (!hasCachedSession) return;
            try {
                const updatedUi = await settingsService.patchUiPreferences({ balanceViewMode: mode });
                if (updatedUi && appConfig) {
                    setAppConfig({ ...appConfig, uiPreferences: updatedUi });
                }
            } catch (e) {
                logger.warn(`Failed to persist balanceViewMode preference: ${stringifyError(e)}`);
            }
        },
        [appConfig, hasCachedSession],
    );

    const handleReconcileSyncAlert = useCallback(async () => {
        setIsReconcilingSyncAlert(true);
        try {
            await protectAction(async (password) => {
                const res = await backupService.reconcileRestoredWalletL2(password || undefined);
                const refreshedConfig = await settingsService.getConfig().catch(() => null);
                if (refreshedConfig) {
                    setAppConfig(refreshedConfig);
                } else if (res.l2ReconciliationStatus === 'reconciled') {
                    setAppConfig((prev) =>
                        prev
                            ? {
                                  ...prev,
                                  lifecycle: {
                                      ...prev.lifecycle,
                                      pendingRestoreL2Reconciliation: false,
                                      walletSyncAlert: null,
                                  },
                              }
                            : prev,
                    );
                }
            });
        } catch (err) {
            setFeedbackMsg(translateError(err, t));
        } finally {
            setIsReconcilingSyncAlert(false);
        }
    }, [protectAction, t]);

    const handleDismissSyncAlert = useCallback(async () => {
        try {
            await protectAction(async (password) => {
                await backupService.dismissWalletSyncAlert(password || undefined);
                setAppConfig((prev) =>
                    prev
                        ? {
                              ...prev,
                              lifecycle: {
                                  ...prev.lifecycle,
                                  walletSyncAlert: null,
                              },
                          }
                        : prev,
                );
            });
        } catch (err) {
            setFeedbackMsg(translateError(err, t));
        }
    }, [protectAction, t]);

    const handleProtectedNavigate = useCallback(
        (target: Parameters<typeof navigate>[0]) => {
            navigate(target);
        },
        [navigate],
    );

    return {
        profileName,
        profileScope: folderName || profileName,
        integrityReport,
        userId,
        balances,
        activeVouchers,
        recentEvents,
        feedbackMsg,
        activeVouchersCount,
        incompleteCount,
        firstIncompleteVoucherId,
        quarantinedCount,
        pendingSignRequests,
        pendingWotHandshakes,
        pendingTolerancePayments,
        unsupportedMessages,
        openSellerInquiriesCount,
        isOpeningSignRequest,
        expiringSoonCount,
        isLoading,
        voucherCountsByStandard,
        showBackupReminder,
        rehearsalSnoozed,
        setRehearsalSnoozed,
        appConfig,
        walletSyncAlert: appConfig?.lifecycle.walletSyncAlert ?? null,
        isReconcilingSyncAlert,
        handleReconcileSyncAlert,
        handleDismissSyncAlert,
        protectionStatus,
        setProtectionStatus,
        contactsCount,
        syncConnectedCount,
        balanceViewMode,
        handleViewModeChange,
        handleOpenSignRequest,
        handleDismissSignRequest,
        handleConfirmTolerancePayment,
        handleDismissTolerancePayment,
        handleDismissUnsupportedPost,
        handleProtectedNavigate,
        navigate,
    };
}
