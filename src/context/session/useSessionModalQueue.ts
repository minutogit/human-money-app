import { MutableRefObject, useCallback, useEffect, useRef, useState } from 'react';
import type { TFunction } from 'i18next';
import { authService, SessionAccessMode, SessionStatusDto } from '../../services/authService';
import { settingsService } from '../../services/settingsService';
import { isBackendError, stringifyError, translateError } from '../../utils/errorHelper';
import { logger } from '../../utils/log';
import type { PendingModalRequest } from './types';

interface UseSessionModalQueueParams {
    t: TFunction;
    accessModeRef: MutableRefObject<SessionAccessMode>;
    baseAccessModeRef: MutableRefObject<SessionAccessMode>;
    hasCachedSessionRef: MutableRefObject<boolean>;
    folderNameRef: MutableRefObject<string>;
    applySessionStatus: (status: SessionStatusDto) => void;
    executeWithSessionCheck: <T>(
        fn: (pwd: string | null) => Promise<T>,
        pwd: string | null,
    ) => Promise<T>;
}

export function useSessionModalQueue({
    t,
    accessModeRef,
    baseAccessModeRef,
    hasCachedSessionRef,
    folderNameRef,
    applySessionStatus,
    executeWithSessionCheck,
}: UseSessionModalQueueParams) {
    const [modalErrorMsg, setModalErrorMsg] = useState<string>('');
    const [activePending, setActivePending] = useState<PendingModalRequest | null>(null);
    const [isModalSubmitting, setIsModalSubmitting] = useState(false);

    const pendingQueueRef = useRef<PendingModalRequest[]>([]);
    const nextPendingIdRef = useRef(1);
    const isModalSubmittingRef = useRef(false);
    const modalAttemptGenRef = useRef(0);

    useEffect(() => {
        const pendingQueue = pendingQueueRef;
        return () => {
            modalAttemptGenRef.current += 1;
            isModalSubmittingRef.current = false;

            if (pendingQueue.current.length > 0) {
                const remaining = pendingQueue.current;
                pendingQueue.current = [];
                remaining.forEach((entry) => entry.reject(new Error('SessionProvider unmounted')));
            }
        };
    }, []);

    const enqueuePendingRequest = useCallback(
        <T,>(
            mode: 'session' | 'stepUp',
            credentialKind: 'password' | 'pin',
            action: (password: string | null) => Promise<T>,
            retriedExpired = false,
        ): Promise<T> => {
            return new Promise<T>((resolve, reject) => {
                const entry: PendingModalRequest = {
                    id: nextPendingIdRef.current++,
                    mode,
                    credentialKind,
                    resolve: resolve as (value: unknown) => void,
                    reject,
                    action: action as (password: string | null) => Promise<unknown>,
                    retriedExpired,
                };
                pendingQueueRef.current.push(entry);
                if (pendingQueueRef.current.length === 1) {
                    setModalErrorMsg('');
                    setActivePending(entry);
                }
            });
        },
        [],
    );

    const switchActivePendingToPassword = useCallback((errorText?: string) => {
        modalAttemptGenRef.current += 1;
        isModalSubmittingRef.current = false;
        setIsModalSubmitting(false);
        const current = pendingQueueRef.current[0];
        if (!current) return;
        current.credentialKind = 'password';
        setModalErrorMsg(errorText ?? '');
        setActivePending({ ...current });
    }, []);

    const rejectAllPending = useCallback((reason: string) => {
        modalAttemptGenRef.current += 1;
        isModalSubmittingRef.current = false;
        setIsModalSubmitting(false);

        if (pendingQueueRef.current.length > 0) {
            const remaining = pendingQueueRef.current;
            pendingQueueRef.current = [];
            setActivePending(null);
            setModalErrorMsg('');
            remaining.forEach((entry) => entry.reject(new Error(reason)));
        }
    }, []);

    const runPendingEntry = useCallback(
        async (entry: PendingModalRequest, password: string | null) => {
            try {
                const result = await executeWithSessionCheck(entry.action, password);
                entry.resolve(result);
            } catch (actionErr) {
                if (
                    entry.mode === 'session' &&
                    actionErr instanceof Error &&
                    actionErr.message === 'STEP_UP_REQUIRED_RETRY'
                ) {
                    enqueuePendingRequest('stepUp', 'password', entry.action).then(
                        entry.resolve,
                        entry.reject,
                    );
                    return;
                }
                if (
                    entry.mode === 'session' &&
                    !entry.retriedExpired &&
                    actionErr instanceof Error &&
                    actionErr.message === 'SESSION_EXPIRED_RETRY'
                ) {
                    enqueuePendingRequest(
                        'session',
                        baseAccessModeRef.current === 'dailyPay' ? 'pin' : 'password',
                        entry.action,
                        true,
                    ).then(entry.resolve, entry.reject);
                    return;
                }
                entry.reject(actionErr);
            }
        },
        [baseAccessModeRef, enqueuePendingRequest, executeWithSessionCheck],
    );

    const handleModalConfirm = useCallback(
        async (credential: string) => {
            const currentPending = pendingQueueRef.current[0];
            if (!currentPending || isModalSubmittingRef.current) return;
            const gen = ++modalAttemptGenRef.current;
            isModalSubmittingRef.current = true;
            setIsModalSubmitting(true);

            const isAttemptCurrent = () =>
                modalAttemptGenRef.current === gen &&
                pendingQueueRef.current[0]?.id === currentPending.id;

            try {
                if (currentPending.mode === 'stepUp') {
                    try {
                        const status = await authService.elevateSessionWithPassword(credential);
                        if (!isAttemptCurrent()) return;
                        applySessionStatus(status);
                    } catch (err) {
                        if (!isAttemptCurrent()) return;
                        setModalErrorMsg(translateError(err, t));
                        return;
                    }

                    let timeout = 0;
                    try {
                        const config = await settingsService.getConfig();
                        if (!isAttemptCurrent()) return;
                        timeout = config.securityPolicy.sessionTimeoutSeconds;
                    } catch {
                        if (!isAttemptCurrent()) return;
                        timeout = 0;
                    }

                    setModalErrorMsg('');
                    pendingQueueRef.current.shift();

                    const toRun: Array<{ entry: PendingModalRequest; pwd: string | null }> = [
                        { entry: currentPending, pwd: credential },
                    ];

                    if (timeout > 0) {
                        while (pendingQueueRef.current.length > 0) {
                            const next = pendingQueueRef.current[0];
                            if (next.mode === 'stepUp') {
                                pendingQueueRef.current.shift();
                                toRun.push({ entry: next, pwd: credential });
                            } else if (next.mode === 'session' && hasCachedSessionRef.current) {
                                pendingQueueRef.current.shift();
                                toRun.push({ entry: next, pwd: null });
                            } else {
                                break;
                            }
                        }
                    }

                    setActivePending(pendingQueueRef.current[0] ?? null);
                    isModalSubmittingRef.current = false;
                    setIsModalSubmitting(false);
                    await Promise.all(toRun.map((item) => runPendingEntry(item.entry, item.pwd)));
                    return;
                }

                let timeout = 0;
                try {
                    const config = await settingsService.getConfig();
                    if (!isAttemptCurrent()) return;
                    timeout = config.securityPolicy.sessionTimeoutSeconds;
                } catch (err) {
                    if (!isAttemptCurrent()) return;
                    setModalErrorMsg(translateError(err, t));
                    return;
                }

                const folderArg = folderNameRef.current || undefined;

                if (timeout > 0) {
                    if (currentPending.credentialKind === 'pin') {
                        try {
                            const status = await authService.unlockSessionWithPin(
                                credential,
                                folderArg,
                            );
                            if (!isAttemptCurrent()) return;
                            applySessionStatus(status);
                            setModalErrorMsg('');
                        } catch (err) {
                            if (!isAttemptCurrent()) return;
                            const code = isBackendError(err) ? err.code : '';
                            const msg = stringifyError(err);
                            const isPinWiped =
                                code === 'error.auth.pinWipedMaxAttempts' ||
                                msg.includes('pinWipedMaxAttempts');
                            if (isPinWiped) {
                                switchActivePendingToPassword(translateError(err, t));
                            } else {
                                setModalErrorMsg(translateError(err, t));
                            }
                            return;
                        }
                    } else {
                        try {
                            if (baseAccessModeRef.current !== 'full') {
                                const status =
                                    await authService.elevateSessionWithPassword(credential);
                                if (!isAttemptCurrent()) return;
                                applySessionStatus(status);
                            } else {
                                const status = await authService.unlockSession(credential);
                                if (!isAttemptCurrent()) return;
                                if ((status as unknown) === false) {
                                    throw new Error('Invalid password');
                                }
                                applySessionStatus(status);
                            }
                            setModalErrorMsg('');
                            logger.info(`Session unlocked via modal for ${timeout} seconds.`);
                        } catch (err) {
                            if (!isAttemptCurrent()) return;
                            const msg = stringifyError(err);
                            const code = isBackendError(err) ? err.code : '';
                            const isInsufficient =
                                code === 'error.auth.insufficientAccessMode' ||
                                msg.includes('InsufficientAccessMode');
                            if (isInsufficient && accessModeRef.current !== 'full') {
                                try {
                                    const status =
                                        await authService.elevateSessionWithPassword(credential);
                                    if (!isAttemptCurrent()) return;
                                    applySessionStatus(status);
                                    setModalErrorMsg('');
                                } catch (elevErr) {
                                    if (!isAttemptCurrent()) return;
                                    setModalErrorMsg(translateError(elevErr, t));
                                    return;
                                }
                            } else {
                                setModalErrorMsg(translateError(err, t));
                                return;
                            }
                        }
                    }

                    if (!isAttemptCurrent()) return;
                    pendingQueueRef.current.shift();
                    const toRun: PendingModalRequest[] = [currentPending];
                    while (
                        pendingQueueRef.current.length > 0 &&
                        pendingQueueRef.current[0].mode === 'session'
                    ) {
                        toRun.push(pendingQueueRef.current.shift()!);
                    }

                    setActivePending(pendingQueueRef.current[0] ?? null);
                    isModalSubmittingRef.current = false;
                    setIsModalSubmitting(false);
                    await Promise.all(toRun.map((entry) => runPendingEntry(entry, null)));
                } else {
                    // Mode A (timeout === 0): prompt per action; PIN uses a single-use backend grant token
                    if (currentPending.credentialKind === 'pin') {
                        let status: Awaited<ReturnType<typeof authService.unlockSessionWithPin>>;
                        try {
                            status = await authService.unlockSessionWithPin(credential, folderArg);
                            if (!isAttemptCurrent()) return;
                        } catch (err) {
                            if (!isAttemptCurrent()) return;
                            const code = isBackendError(err) ? err.code : '';
                            const msg = stringifyError(err);
                            const isPinWiped =
                                code === 'error.auth.pinWipedMaxAttempts' ||
                                msg.includes('pinWipedMaxAttempts');
                            if (isPinWiped) {
                                switchActivePendingToPassword(translateError(err, t));
                            } else {
                                setModalErrorMsg(translateError(err, t));
                            }
                            return;
                        }

                        applySessionStatus(status);
                        setModalErrorMsg('');
                        pendingQueueRef.current.shift();
                        setActivePending(null);
                        isModalSubmittingRef.current = false;
                        setIsModalSubmitting(false);
                        try {
                            await runPendingEntry(currentPending, status.oneShotGrant ?? null);
                        } finally {
                            setActivePending(pendingQueueRef.current[0] ?? null);
                        }
                    } else {
                        if (baseAccessModeRef.current !== 'full') {
                            try {
                                const status =
                                    await authService.elevateSessionWithPassword(credential);
                                if (!isAttemptCurrent()) return;
                                applySessionStatus(status);
                            } catch (err) {
                                if (!isAttemptCurrent()) return;
                                setModalErrorMsg(translateError(err, t));
                                return;
                            }
                        } else {
                            try {
                                const status = await authService.unlockSession(credential);
                                if (!isAttemptCurrent()) return;
                                if ((status as unknown) === false) {
                                    throw new Error('Invalid password');
                                }
                                applySessionStatus(status);
                            } catch (err) {
                                if (!isAttemptCurrent()) return;
                                setModalErrorMsg(translateError(err, t));
                                return;
                            }
                        }
                        if (!isAttemptCurrent()) return;
                        setModalErrorMsg('');
                        pendingQueueRef.current.shift();
                        setActivePending(pendingQueueRef.current[0] ?? null);
                        isModalSubmittingRef.current = false;
                        setIsModalSubmitting(false);
                        await runPendingEntry(currentPending, credential);
                    }
                }
            } finally {
                if (modalAttemptGenRef.current === gen) {
                    isModalSubmittingRef.current = false;
                    setIsModalSubmitting(false);
                }
            }
        },
        [
            accessModeRef,
            applySessionStatus,
            baseAccessModeRef,
            folderNameRef,
            hasCachedSessionRef,
            runPendingEntry,
            switchActivePendingToPassword,
            t,
        ],
    );

    const handleModalCancel = useCallback(() => {
        rejectAllPending('User cancelled authentication');
    }, [rejectAllPending]);

    return {
        modalErrorMsg,
        activePending,
        isModalSubmitting,
        enqueuePendingRequest,
        switchActivePendingToPassword,
        handleModalConfirm,
        handleModalCancel,
        rejectAllPending,
    };
}
