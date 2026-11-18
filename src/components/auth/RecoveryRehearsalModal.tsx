import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Brain, CheckCircle2, XCircle, RefreshCw, KeyRound } from 'lucide-react';
import { ModalShell } from '../ui/ModalShell';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { AlertBanner } from '../ui/AlertBanner';
import { RECOVERY_QUESTION_MAP } from '../../constants/recoveryQuestions';
import { recoveryService } from '../../services/recoveryService';
import { RehearsalCheckResultDto } from '../../types/recovery';
import { translateError, isBackendError, stringifyError } from '../../utils/errorHelper';
import { useOptionalSession } from '../../context/SessionContext';

export interface RecoveryRehearsalModalProps {
    isOpen: boolean;
    questionIds: [number, number, number];
    onClose: () => void;
    onVerified: () => void;
    onResetQuestions: () => void;
}

export const RecoveryRehearsalModal: React.FC<RecoveryRehearsalModalProps> = ({
    isOpen,
    questionIds,
    onClose,
    onVerified,
    onResetQuestions,
}) => {
    const { t } = useTranslation();
    const session = useOptionalSession();
    const [answers, setAnswers] = useState<[string, string, string]>(['', '', '']);
    const [result, setResult] = useState<RehearsalCheckResultDto | null>(null);
    const [isChecking, setIsChecking] = useState(false);
    const [errorMsg, setErrorMsg] = useState('');

    useEffect(() => {
        if (isOpen) {
            setAnswers(['', '', '']);
            setResult(null);
            setErrorMsg('');
        }
    }, [isOpen]);

    if (!isOpen) return null;

    const allFilled = answers.every((a) => a.trim().length > 0);

    const isLockedWalletError = (err: unknown): boolean => {
        const code = isBackendError(err) ? err.code : '';
        const msg = stringifyError(err).toLowerCase();
        return (
            code === 'error.auth.walletLocked' ||
            code === 'error.auth.sessionLocked' ||
            msg.includes('locked') ||
            msg.includes('password required') ||
            msg.includes('passwort erforderlich')
        );
    };

    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!allFilled || isChecking) return;
        setIsChecking(true);
        setErrorMsg('');
        try {
            let res: RehearsalCheckResultDto;
            try {
                res = await recoveryService.verifyRecoveryRehearsal(answers);
            } catch (firstErr) {
                if (isLockedWalletError(firstErr) && session?.protectAction) {
                    const protectedRes = await session.protectAction(async (pwd) => {
                        return await recoveryService.verifyRecoveryRehearsal(answers, pwd || undefined);
                    });
                    if (!protectedRes) return;
                    res = protectedRes;
                } else {
                    throw firstErr;
                }
            }
            setResult(res);
            if (res.allCorrect) {
                onVerified();
            }
        } catch (err) {
            setErrorMsg(translateError(err, t));
        } finally {
            setIsChecking(false);
        }
    };

    const handleSnooze = async () => {
        try {
            try {
                await recoveryService.snoozeRecoveryRehearsal(24);
            } catch (firstErr) {
                if (isLockedWalletError(firstErr) && session?.protectAction) {
                    await session.protectAction(async (pwd) => {
                        return await recoveryService.snoozeRecoveryRehearsal(24, pwd || undefined);
                    });
                } else {
                    throw firstErr;
                }
            }
            onVerified();
        } catch {
            // ignore
        }
        onClose();
    };

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={onClose}
            title={t('backup.rehearsalModalTitle')}
        >
            <div className="space-y-5">
                {errorMsg && (
                    <AlertBanner variant="error" onDismiss={() => setErrorMsg('')}>
                        {errorMsg}
                    </AlertBanner>
                )}

                {result?.allCorrect ? (
                    <div className="space-y-4 text-center py-2">
                        <div className="mx-auto w-14 h-14 rounded-2xl bg-emerald-50 text-emerald-600 flex items-center justify-center">
                            <CheckCircle2 size={32} />
                        </div>
                        <div className="space-y-1">
                            <h4 className="font-bold text-theme-secondary">
                                {t('backup.rehearsalSuccessTitle')}
                            </h4>
                            <p className="text-xs text-theme-light">
                                {t('backup.rehearsalSuccessDesc')}
                            </p>
                        </div>
                        <Button type="button" onClick={onClose} className="w-full">
                            {t('common.done', 'Fertig')}
                        </Button>
                    </div>
                ) : (
                    <form onSubmit={handleSubmit} className="space-y-4">
                        <div className="p-3 rounded-xl bg-theme-primary/5 border border-theme-primary/20 flex items-start gap-3">
                            <Brain size={18} className="text-theme-primary shrink-0 mt-0.5" />
                            <p className="text-xs text-theme-secondary leading-relaxed">
                                {t('backup.rehearsalModalDesc')}
                            </p>
                        </div>

                        {questionIds.map((qId, idx) => {
                            const matched = result ? result.questionMatches[idx] : null;
                            return (
                                <div key={qId} className="space-y-1.5">
                                    <div className="flex items-center justify-between gap-2">
                                        <label className="text-xs font-bold text-theme-secondary">
                                            {idx + 1}. {t(RECOVERY_QUESTION_MAP[qId] || 'backup.recoveryQuestion1')}
                                        </label>
                                        {matched !== null && (
                                            <span
                                                className={`inline-flex items-center gap-1 text-[10px] font-bold px-2 py-0.5 rounded-full shrink-0 ${
                                                    matched
                                                        ? 'bg-emerald-100 text-emerald-800'
                                                        : 'bg-rose-100 text-rose-700'
                                                }`}
                                            >
                                                {matched ? <CheckCircle2 size={11} /> : <XCircle size={11} />}
                                                {matched
                                                    ? t('backup.rehearsalMatchBadge')
                                                    : t('backup.rehearsalMismatchBadge')}
                                            </span>
                                        )}
                                    </div>
                                    <Input
                                        type="text"
                                        value={answers[idx]}
                                        onChange={(e) => {
                                            const next: [string, string, string] = [...answers] as [string, string, string];
                                            next[idx] = e.target.value;
                                            setAnswers(next);
                                            if (result) setResult(null);
                                        }}
                                        placeholder={t('backup.questionAnswerPlaceholder')}
                                        autoFocus={idx === 0}
                                    />
                                </div>
                            );
                        })}

                        {result && !result.allCorrect && (
                            <AlertBanner variant="warning">
                                <div className="space-y-2">
                                    <p className="text-xs">{t('backup.rehearsalPartialMismatchHint')}</p>
                                    <button
                                        type="button"
                                        onClick={() => {
                                            onClose();
                                            onResetQuestions();
                                        }}
                                        className="inline-flex items-center gap-1.5 text-xs font-bold underline cursor-pointer"
                                    >
                                        <KeyRound size={13} />
                                        {t('backup.rehearsalResetQuestionsLink')}
                                    </button>
                                </div>
                            </AlertBanner>
                        )}

                        <div className="flex flex-wrap items-center justify-between gap-2 pt-2">
                            <Button type="button" variant="secondary" size="sm" onClick={handleSnooze}>
                                {t('backup.rehearsalSnoozeButton')}
                            </Button>
                            <Button
                                type="submit"
                                disabled={!allFilled || isChecking}
                                className="gap-1.5"
                            >
                                {isChecking && <RefreshCw size={14} className="animate-spin" />}
                                <span>{t('backup.rehearsalCheckButton')}</span>
                            </Button>
                        </div>
                    </form>
                )}
            </div>
        </ModalShell>
    );
};

export default RecoveryRehearsalModal;
