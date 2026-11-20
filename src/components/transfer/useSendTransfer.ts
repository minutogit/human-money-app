// src/components/transfer/useSendTransfer.ts
import { useState, useEffect, useMemo, FormEvent, useReducer, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { voucherService } from '../../services/voucherService';
import { settingsService } from '../../services/settingsService';
import { backupService } from '../../services/backupService';
import { marketService } from '../../services/marketService';
import { useSession } from '../../context/SessionContext';
import { useNavigation } from '../../context/NavigationContext';
import { useVoucherSelection } from '../../hooks/useVoucherSelection';
import { usePrivacyDetection } from '../../hooks/usePrivacyDetection';
import { useRateLimitCooldown } from '../../hooks/useRateLimitCooldown';
import { logger } from '../../utils/log';
import { translateError, stringifyError } from '../../utils/errorHelper';
import { formatAmount } from '../../utils/format';
import { readFromClipboard } from '../../utils/clipboard';
import { parseHumocoInput } from '../../utils/humocoUri';
import {
    commitAndDepositTransfer,
    initialSendState,
    resolvePaymentRequestInput,
    sendReducer,
} from './sendTransferHelpers';
import { loadSendInitialData } from './sendTransferData';
import type {
    VoucherSummary,
    VoucherStandardInfo,
    Contact,
    TrustStatus,
    AppConfigDto,
    AssetClassSummary,
    VoucherStandardDefinition,
} from '../../types';

export interface UseSendTransferParams {
    initialRecipientId?: string;
    initialVoucherId?: string;
    initialNote?: string;
    initialPaymentUri?: string;
}

export function useSendTransfer({
    initialRecipientId,
    initialVoucherId,
    initialNote,
    initialPaymentUri,
}: UseSendTransferParams) {
    const { t } = useTranslation();
    const { protectAction, profileName } = useSession();
    const { navigate, goBack } = useNavigation();
    const { checkAndTriggerCooldown } = useRateLimitCooldown({ trackCountdown: false });

    const [state, dispatch] = useReducer(sendReducer, initialSendState);
    const [paymentRequestSuccessMsg, setPaymentRequestSuccessMsg] = useState<string | null>(null);
    const [isDataLoaded, setIsDataLoaded] = useState(false);
    const [isEditingSender, setIsEditingSender] = useState(false);

    const appliedInitialUriRef = useRef<string | null>(null);
    const preserveInitialVoucherRef = useRef<boolean>(Boolean(initialVoucherId));
    const paymentSuccessTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const reputationSeqRef = useRef(0);
    const autoSelectSeqRef = useRef(0);
    const prevProfileRef = useRef(profileName);
    const isMountedRef = useRef(true);

    const [availableVouchers, setAvailableVouchers] = useState<VoucherSummary[]>([]);
    const [voucherStandards, setVoucherStandards] = useState<VoucherStandardInfo[]>([]);
    const [activeAssetClasses, setActiveAssetClasses] = useState<AssetClassSummary[]>([]);
    const [ownUserId, setOwnUserId] = useState('');
    const [trustStatus, setTrustStatus] = useState<TrustStatus>('clean');
    const [appConfig, setAppConfig] = useState<AppConfigDto | null>(null);
    const [contacts, setContacts] = useState<Contact[]>([]);
    const [standardIdToUuidMap, setStandardIdToUuidMap] = useState<Map<string, string>>(new Map());
    const [uuidToPrecisionMap, setUuidToPrecisionMap] = useState<Map<string, number>>(new Map());
    const [standardsDict, setStandardsDict] = useState<Record<string, VoucherStandardDefinition>>({});
    const [isReconcilingL2, setIsReconcilingL2] = useState(false);
    const [hasPublishedCard, setHasPublishedCard] = useState(false);
    const [sendCardOverride, setSendCardOverride] = useState<boolean | null>(null);

    const {
        selectedMap,
        toggleVoucher,
        selectSingleVoucher,
        setVoucherAmount,
        selectAmount,
        clearSelection,
        selectionStats,
        currentPlan,
        autoSelectGravitational,
    } = useVoucherSelection(availableVouchers);

    const detectedPrivacy = usePrivacyDetection(
        Array.from(selectedMap.keys()),
        availableVouchers,
        standardsDict,
    );

    useEffect(() => {
        isMountedRef.current = true;
        return () => {
            isMountedRef.current = false;
            if (paymentSuccessTimerRef.current) {
                clearTimeout(paymentSuccessTimerRef.current);
                paymentSuccessTimerRef.current = null;
            }
        };
    }, []);

    useEffect(() => {
        preserveInitialVoucherRef.current = Boolean(initialVoucherId);
    }, [initialVoucherId]);

    useEffect(() => {
        if (prevProfileRef.current === profileName) return;
        prevProfileRef.current = profileName;
        autoSelectSeqRef.current += 1;
        reputationSeqRef.current += 1;
        appliedInitialUriRef.current = null;
        preserveInitialVoucherRef.current = Boolean(initialVoucherId);
        clearSelection();
        dispatch({ type: 'RESET', recipientId: initialRecipientId, notes: initialNote });
        setSendCardOverride(null);
        setTrustStatus('clean');
        setPaymentRequestSuccessMsg(null);
        setIsDataLoaded(false);
    }, [profileName, initialRecipientId, initialNote, initialVoucherId, clearSelection]);

    useEffect(() => {
        if (initialRecipientId) dispatch({ type: 'SET_RECIPIENT', id: initialRecipientId });
        if (initialNote) dispatch({ type: 'SET_NOTES', notes: initialNote });
    }, [initialRecipientId, initialNote]);

    useEffect(() => {
        void settingsService.prewarmL2Checkout?.().catch(() => undefined);
    }, []);

    useEffect(() => {
        let active = true;
        loadSendInitialData()
            .then((loaded) => {
                if (!active) return;
                setHasPublishedCard(loaded.hasPublishedCard);
                dispatch({ type: 'SET_SENDER_NAME', name: loaded.defaultSenderName });
                setStandardsDict(loaded.standardsDict);
                setStandardIdToUuidMap(loaded.standardIdToUuidMap);
                setUuidToPrecisionMap(loaded.uuidToPrecisionMap);
                setAvailableVouchers(loaded.availableVouchers);
                setVoucherStandards(loaded.voucherStandards);
                setOwnUserId(loaded.ownUserId);
                setAppConfig(loaded.appConfig);
                setContacts(loaded.contacts);
                setActiveAssetClasses(loaded.activeAssetClasses);

                if (initialVoucherId) {
                    const target = loaded.availableVouchers.find(
                        (v) => v.localInstanceId === initialVoucherId,
                    );
                    if (target) {
                        const stdId =
                            [...loaded.standardIdToUuidMap.entries()].find(
                                ([, u]) => u === target.voucherStandardUuid,
                            )?.[0] || target.voucherStandardUuid;
                        dispatch({ type: 'SET_STANDARD', id: stdId, isTest: target.isTestVoucher });
                        dispatch({ type: 'SET_TARGET_AMOUNT', amount: target.currentAmount });
                        selectSingleVoucher(target.localInstanceId, target.currentAmount);
                    }
                } else if (loaded.activeAssetClasses.length > 0) {
                    const firstClass =
                        loaded.activeAssetClasses.find((c) => !c.isTestVoucher) ||
                        loaded.activeAssetClasses[0];
                    const stdId =
                        [...loaded.standardIdToUuidMap.entries()].find(
                            ([, u]) => u === firstClass.standardUuid,
                        )?.[0] || firstClass.standardUuid;
                    dispatch({
                        type: 'SET_STANDARD',
                        id: stdId,
                        isTest: firstClass.isTestVoucher,
                        preserveAmount: true,
                    });
                }
                setIsDataLoaded(true);
            })
            .catch((e) => {
                if (!active) return;
                logger.error(`Failed to fetch data for SendView: ${stringifyError(e)}`);
                dispatch({ type: 'SET_FEEDBACK', msg: `Error: ${translateError(e, t)}` });
            });
        return () => {
            active = false;
        };
    }, [profileName, t, initialVoucherId, selectSingleVoucher]);

    useEffect(() => {
        if (detectedPrivacy === 'stealth') dispatch({ type: 'SET_PRIVACY', mode: 'stealth' });
        else if (detectedPrivacy === 'public') dispatch({ type: 'SET_PRIVACY', mode: 'public' });
        else if (detectedPrivacy === 'flexible' && state.privacyMode === null && appConfig) {
            const privacyDefault = appConfig.securityPolicy.privacyDefault;
            const defaultMode =
                privacyDefault === 'stealth'
                    ? 'stealth'
                    : privacyDefault === 'public'
                      ? 'public'
                      : null;
            dispatch({ type: 'SET_PRIVACY', mode: defaultMode });
        }
    }, [detectedPrivacy, appConfig, state.privacyMode]);

    useEffect(() => {
        const seq = ++reputationSeqRef.current;
        let active = true;
        setTrustStatus('clean');
        if (state.recipientId.length < 10) {
            return;
        }
        const timer = setTimeout(async () => {
            try {
                const status = await voucherService.checkReputation(state.recipientId);
                if (active && seq === reputationSeqRef.current) setTrustStatus(status);
            } catch (e) {
                logger.error(`Reputation check failed: ${stringifyError(e)}`);
            }
        }, 500);
        return () => {
            active = false;
            clearTimeout(timer);
        };
    }, [state.recipientId]);

    const filteredVouchers = useMemo(() => {
        if (!state.selectedStandardId) return availableVouchers;
        const selectedUuid = standardIdToUuidMap.get(state.selectedStandardId) || state.selectedStandardId;
        if (!selectedUuid) return [];
        return availableVouchers.filter(
            (v) => v.voucherStandardUuid === selectedUuid && (state.selectedIsTest === null || v.isTestVoucher === state.selectedIsTest),
        );
    }, [availableVouchers, state.selectedStandardId, state.selectedIsTest, standardIdToUuidMap]);

    useEffect(() => {
        if (preserveInitialVoucherRef.current) return;
        const seq = ++autoSelectSeqRef.current;
        let active = true;
        const amountStr = state.targetAmountStr.trim().replace(',', '.');
        const amt = parseFloat(amountStr);
        if (isNaN(amt) || amt <= 0 || !state.selectedStandardId) return;
        const standardUuid = standardIdToUuidMap.get(state.selectedStandardId) || state.selectedStandardId;
        if (!standardUuid) return;
        const voucher = filteredVouchers[0];
        const precision = voucher ? (uuidToPrecisionMap.get(voucher.voucherStandardUuid) ?? 4) : 4;
        const timer = setTimeout(() => {
            if (!active || seq !== autoSelectSeqRef.current) return;
            autoSelectGravitational(amountStr, state.recipientId.trim(), standardUuid, precision, state.selectedIsTest).catch((e) => {
                if (!active || seq !== autoSelectSeqRef.current) return;
                logger.error(`Gravitational auto-select failed: ${stringifyError(e)}`);
                const val = parseFloat(amountStr);
                if (!isNaN(val) && val > 0 && voucher) {
                    selectAmount(val, voucher.displayCurrency, precision, state.selectedIsTest, standardUuid);
                }
            });
        }, 400);
        return () => {
            active = false;
            clearTimeout(timer);
        };
    }, [state.targetAmountStr, state.recipientId, state.selectedStandardId, state.selectedIsTest, filteredVouchers, standardIdToUuidMap, uuidToPrecisionMap, autoSelectGravitational, selectAmount]);

    const handleTargetAmountChange = (valStr: string) => {
        preserveInitialVoucherRef.current = false;
        autoSelectSeqRef.current += 1;
        dispatch({ type: 'SET_TARGET_AMOUNT', amount: valStr });
        let effectiveStdId = state.selectedStandardId;
        let effectiveIsTest = state.selectedIsTest;

        if (!effectiveStdId && activeAssetClasses.length > 0) {
            const preferred = activeAssetClasses.find((c) => !c.isTestVoucher) || activeAssetClasses[0];
            effectiveStdId = [...standardIdToUuidMap.entries()].find(([, u]) => u === preferred.standardUuid)?.[0] || preferred.standardUuid;
            effectiveIsTest = preferred.isTestVoucher;
            dispatch({ type: 'SET_STANDARD', id: effectiveStdId, isTest: effectiveIsTest, preserveAmount: true });
        }

        const val = parseFloat(valStr.replace(',', '.'));
        if (!valStr.trim() || isNaN(val) || val <= 0) {
            clearSelection();
            return;
        }

        if (state.recipientId.trim().length < 10) {
            const effUuid = effectiveStdId ? standardIdToUuidMap.get(effectiveStdId) || effectiveStdId : null;
            const match = availableVouchers.find(
                (v) => (!effUuid || v.voucherStandardUuid === effUuid) && (effectiveIsTest === null || v.isTestVoucher === effectiveIsTest),
            );
            if (match) {
                selectAmount(val, match.displayCurrency, uuidToPrecisionMap.get(match.voucherStandardUuid) ?? 4, effectiveIsTest, effUuid);
            }
        }
    };

    const applyPaymentRequest = useCallback(
        (rawText: string): boolean => {
            const res = resolvePaymentRequestInput(rawText, standardIdToUuidMap, voucherStandards, state.selectedStandardId);
            if (!res.matched || !res.to) return false;
            preserveInitialVoucherRef.current = false;
            autoSelectSeqRef.current += 1;
            dispatch({ type: 'SET_FEEDBACK', msg: '' });
            dispatch({ type: 'SET_RECIPIENT', id: res.to });
            if (res.note) dispatch({ type: 'SET_NOTES', notes: res.note });
            if (res.missingStandardName) {
                clearSelection();
                setPaymentRequestSuccessMsg(null);
                dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.paymentRequestStandardMissing', { name: res.missingStandardName }) });
                return true;
            }
            if (res.standardId !== undefined && res.isTest !== undefined) {
                if (res.standardId !== state.selectedStandardId || res.isTest !== state.selectedIsTest) {
                    clearSelection();
                }
                dispatch({ type: 'SET_STANDARD', id: res.standardId, isTest: res.isTest });
            }
            if (res.amount) dispatch({ type: 'SET_TARGET_AMOUNT', amount: res.amount });
            if (paymentSuccessTimerRef.current) {
                clearTimeout(paymentSuccessTimerRef.current);
            }
            setPaymentRequestSuccessMsg(t('transfer.paymentRequestApplied'));
            paymentSuccessTimerRef.current = setTimeout(() => {
                paymentSuccessTimerRef.current = null;
                setPaymentRequestSuccessMsg(null);
            }, 3500);
            return true;
        },
        [standardIdToUuidMap, voucherStandards, state.selectedStandardId, state.selectedIsTest, clearSelection, t],
    );

    useEffect(() => {
        if (!isDataLoaded || !initialPaymentUri || appliedInitialUriRef.current === initialPaymentUri) return;
        appliedInitialUriRef.current = initialPaymentUri;
        applyPaymentRequest(initialPaymentUri);
    }, [isDataLoaded, initialPaymentUri, applyPaymentRequest]);

    const handleApplyRawCode = async (rawText: string): Promise<boolean> => {
        const trimmed = rawText.trim();
        if (!trimmed) {
            dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.emptyClipboard') });
            return false;
        }
        const parsed = parseHumocoInput(trimmed);
        if (parsed?.kind === 'payment') {
            const ok = applyPaymentRequest(trimmed);
            if (!ok) dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.invalidPaymentRequest') });
            return ok;
        }
        if (parsed?.kind === 'contact') {
            dispatch({ type: 'SET_FEEDBACK', msg: '' });
            dispatch({ type: 'SET_RECIPIENT', id: parsed.contact.did });
            return true;
        }
        if (parsed?.kind === 'card') {
            try {
                const resolved = await marketService.resolveCardRecipient(parsed.payload);
                dispatch({ type: 'SET_FEEDBACK', msg: '' });
                dispatch({ type: 'SET_RECIPIENT', id: resolved.recipientId });
                return true;
            } catch (e) {
                dispatch({ type: 'SET_FEEDBACK', msg: translateError(e, t) });
                return false;
            }
        }
        if (parsed?.kind === 'did') {
            dispatch({ type: 'SET_FEEDBACK', msg: '' });
            dispatch({ type: 'SET_RECIPIENT', id: parsed.did });
            return true;
        }
        dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.invalidPaymentRequest') });
        return false;
    };

    const handlePastePaymentRequest = async () => {
        try {
            const { text, error } = await readFromClipboard();
            if (error) return dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.clipboardReadFailed') });
            if (!text.trim()) return dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.emptyClipboard') });
            await handleApplyRawCode(text);
        } catch (e) {
            logger.error(`Failed to read clipboard for payment request: ${stringifyError(e)}`);
            dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.clipboardReadFailed') });
        }
    };

    const handlePrepareTransfer = (e: FormEvent) => {
        e.preventDefault();
        if (appConfig?.lifecycle.pendingRestoreL2Reconciliation) {
            dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.pendingL2ReconciliationBlocked', 'Ausgaben sind vorübergehend gesperrt, bis der Online-Sicherheitsabgleich durchgeführt wurde.') });
            return;
        }
        if (!state.recipientId) {
            dispatch({ type: 'SET_RECIPIENT_ERROR', value: true });
            (document.getElementById('recipientId') || document.querySelector<HTMLElement>('[data-testid="recipient-search-input"]'))?.scrollIntoView({ behavior: 'smooth', block: 'center' });
            return;
        }
        if (selectedMap.size === 0) {
            dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.assetSelectionRequired') });
            return;
        }
        if (state.selectedStandardId) {
            const expectedUuid =
                standardIdToUuidMap.get(state.selectedStandardId) || state.selectedStandardId;
            const mismatched = Array.from(selectedMap.keys()).some((vId) => {
                const found = availableVouchers.find((v) => v.localInstanceId === vId);
                return (
                    found &&
                    (found.voucherStandardUuid !== expectedUuid ||
                        (state.selectedIsTest !== null &&
                            Boolean(found.isTestVoucher) !== Boolean(state.selectedIsTest)))
                );
            });
            if (mismatched) {
                clearSelection();
                dispatch({ type: 'SET_TARGET_AMOUNT', amount: '' });
                dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.assetSelectionRequired') });
                return;
            }
        }
        if (detectedPrivacy === 'incompatible' || (detectedPrivacy === 'flexible' && state.privacyMode === null)) {
            dispatch({ type: 'SET_PRIVACY_ERROR', value: true });
            dispatch({ type: 'SET_FEEDBACK', msg: t(detectedPrivacy === 'incompatible' ? 'transfer.privacyConflict' : 'transfer.privacyRequired') });
            document.getElementById('privacyMode')?.scrollIntoView({ behavior: 'smooth', block: 'center' });
            return;
        }
        dispatch({ type: 'TOGGLE_CONFIRM', value: true });
    };

    const executeTransfer = async () => {
        dispatch({ type: 'SET_PROCESSING', value: true });
        try {
            const txOutcome = await commitAndDepositTransfer({
                state, selectedMap, voucherStandards, standardIdToUuidMap,
                sendCardOverride, protectAction, checkAndTriggerCooldown, translate: t,
            });
            if (!isMountedRef.current || !txOutcome) return;
            const summaryString = Object.entries(selectionStats)
                .map(([currency, stat]) => `${formatAmount(stat.total.toString())} ${currency}`)
                .join(', ');
            dispatch({ type: 'TOGGLE_CONFIRM', value: false });
            navigate({
                view: 'transfer_success',
                bundleData: txOutcome.bundleResult.bundleData,
                recipientId: state.recipientId,
                summary: summaryString,
                l2Dispatch: txOutcome.bundleResult.l2Dispatch,
                bundleId: txOutcome.bundleResult.bundleId,
                depositedTo: txOutcome.depositedTo,
                recipientDepositedTo: txOutcome.recipientDepositedTo,
                depositError: txOutcome.depositError,
            });
        } catch (e) {
            if (!isMountedRef.current) return;
            checkAndTriggerCooldown(e);
            logger.error(`Transfer failed: ${stringifyError(e)}`);
            dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.transferFailed', { error: translateError(e, t) }) });
            dispatch({ type: 'TOGGLE_CONFIRM', value: false });
        } finally {
            if (isMountedRef.current) {
                dispatch({ type: 'SET_PROCESSING', value: false });
            }
        }
    };

    const handleReconcileL2 = async () => {
        setIsReconcilingL2(true);
        try {
            await protectAction(async (password) => {
                const res = await backupService.reconcileRestoredWalletL2(password || undefined);
                if (isMountedRef.current && res.l2ReconciliationStatus === 'reconciled') {
                    setAppConfig((prev) =>
                        prev
                            ? {
                                  ...prev,
                                  lifecycle: {
                                      ...prev.lifecycle,
                                      pendingRestoreL2Reconciliation: false,
                                      walletSyncAlert:
                                          prev.lifecycle.walletSyncAlert?.kind ===
                                              'stale_local_backup' ||
                                          prev.lifecycle.walletSyncAlert?.kind ===
                                              'fork_detected'
                                              ? null
                                              : prev.lifecycle.walletSyncAlert,
                                  },
                              }
                            : prev,
                    );
                }
            });
        } catch (err) {
            if (!isMountedRef.current) return;
            logger.error(`Reconcile failed: ${stringifyError(err)}`);
            dispatch({ type: 'SET_FEEDBACK', msg: translateError(err, t) });
        } finally {
            if (isMountedRef.current) {
                setIsReconcilingL2(false);
            }
        }
    };

    return {
        state, dispatch, navigate, goBack,
        paymentRequestSuccessMsg, isEditingSender, setIsEditingSender,
        availableVouchers, filteredVouchers, activeAssetClasses, ownUserId, trustStatus,
        appConfig, contacts, standardIdToUuidMap, uuidToPrecisionMap,
        isReconcilingL2, hasPublishedCard, sendCardOverride, setSendCardOverride,
        selectedMap, selectionStats, currentPlan, detectedPrivacy,
        applyPaymentRequest, handlePastePaymentRequest, handleApplyRawCode, handleTargetAmountChange,
        handleVoucherToggle: (v: VoucherSummary) => { preserveInitialVoucherRef.current = false; autoSelectSeqRef.current += 1; toggleVoucher(v.localInstanceId, v.currentAmount); },
        handleAmountChange: (id: string, amt: string) => { preserveInitialVoucherRef.current = false; autoSelectSeqRef.current += 1; setVoucherAmount(id, amt); },
        handleStandardSelect: (id: string | null, isTest: boolean | null) => {
            preserveInitialVoucherRef.current = false;
            autoSelectSeqRef.current += 1;
            if (id !== state.selectedStandardId || isTest !== state.selectedIsTest) {
                clearSelection();
            }
            dispatch({ type: 'SET_STANDARD', id, isTest });
        },
        handlePrepareTransfer, executeTransfer, handleReconcileL2,
    };
}
