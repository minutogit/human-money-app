// src/components/GuardianSetupModal.tsx
import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { ModalShell } from './ui/ModalShell';
import { Button } from './ui/Button';
import { Input } from './ui/Input';
import { AlertBanner } from './ui/AlertBanner';
import { contactService } from '../services/contactService';
import { recoveryService } from '../services/recoveryService';
import { Contact } from '../types';
import { translateError } from '../utils/errorHelper';
import { ShieldCheck, Users, CheckCircle2, AlertTriangle, KeyRound } from 'lucide-react';

interface GuardianSetupModalProps {
    isOpen: boolean;
    onClose: () => void;
    onSuccess?: () => void;
}

const QUESTION_CATALOG: Array<{ id: number; key: string }> = [
    { id: 1, key: 'backup.recoveryQuestion1' },
    { id: 2, key: 'backup.recoveryQuestion2' },
    { id: 3, key: 'backup.recoveryQuestion3' },
    { id: 4, key: 'backup.recoveryQuestion4' },
    { id: 5, key: 'backup.recoveryQuestion5' },
    { id: 6, key: 'backup.recoveryQuestion6' },
];

export const GuardianSetupModal: React.FC<GuardianSetupModalProps> = ({
    isOpen,
    onClose,
    onSuccess,
}) => {
    const { t } = useTranslation();
    const [contacts, setContacts] = useState<Contact[]>([]);
    const [selectedDids, setSelectedDids] = useState<string[]>([]);
    const [thresholdM, setThresholdM] = useState<number>(2);

    // Selected questions (3 distinct question IDs)
    const [q1, setQ1] = useState<number>(1);
    const [q2, setQ2] = useState<number>(2);
    const [q3, setQ3] = useState<number>(3);

    // Answers
    const [a1, setA1] = useState<string>('');
    const [a2, setA2] = useState<string>('');
    const [a3, setA3] = useState<string>('');

    const [password, setPassword] = useState<string>('');
    const [isLoadingContacts, setIsLoadingContacts] = useState<boolean>(false);
    const [isSubmitting, setIsSubmitting] = useState<boolean>(false);
    const [errorMsg, setErrorMsg] = useState<string>('');

    // Reset transient state when opened or closed
    useEffect(() => {
        if (!isOpen) {
            setPassword('');
            setErrorMsg('');
            setIsSubmitting(false);
            return;
        }

        setErrorMsg('');
        setPassword('');
        setIsSubmitting(false);
        let active = true;

        const loadEligibleContacts = async () => {
            setIsLoadingContacts(true);
            try {
                const list = await contactService.getContacts();
                if (!active) return;
                // Filter eligible contacts: trustLevel >= 1 (Acquaintance / Bekannt)
                const eligible = (list || []).filter(
                    (c) => (c.trustLevel ?? 0) >= 1
                );
                setContacts(eligible);
                setSelectedDids((prev) => {
                    if (eligible.length >= 3 && prev.length === 0) {
                        setThresholdM(2);
                        return eligible.slice(0, 3).map((c) => c.did);
                    }
                    return prev;
                });
            } catch (err) {
                if (active) {
                    setErrorMsg(translateError(err, t));
                }
            } finally {
                if (active) {
                    setIsLoadingContacts(false);
                }
            }
        };

        loadEligibleContacts();

        return () => {
            active = false;
        };
    }, [isOpen, t]);

    const handleToggleContact = (did: string) => {
        setErrorMsg('');
        let next: string[];
        if (selectedDids.includes(did)) {
            next = selectedDids.filter((d) => d !== did);
        } else {
            if (selectedDids.length >= 5) {
                return;
            }
            next = [...selectedDids, did];
        }
        setSelectedDids(next);

        // Recalculate default recommended threshold
        const count = next.length;
        if (count >= 2) {
            const recommended = count <= 2 ? 2 : count === 3 ? 2 : count === 4 ? 3 : 3;
            setThresholdM(Math.min(count, Math.max(2, recommended)));
        }
    };

    const normalize = (val: string): string => val.trim().toLowerCase();
    const norm1 = normalize(a1);
    const norm2 = normalize(a2);
    const norm3 = normalize(a3);
    const totalLength = norm1.length + norm2.length + norm3.length;
    const areQuestionsDistinct = q1 !== q2 && q1 !== q3 && q2 !== q3;
    const areAnswersValid = norm1.length >= 2 && norm2.length >= 2 && norm3.length >= 2 && totalLength >= 12;
    const isContactCountValid = selectedDids.length >= 3 && selectedDids.length <= 5;
    const isThresholdValid = thresholdM >= 2 && thresholdM <= selectedDids.length;
    const canSubmit =
        isContactCountValid &&
        isThresholdValid &&
        areQuestionsDistinct &&
        areAnswersValid &&
        password.length > 0 &&
        !isSubmitting;

    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!canSubmit) return;

        setIsSubmitting(true);
        setErrorMsg('');

        try {
            await recoveryService.setupSocialRecoveryGuardians({
                password,
                guardianDids: selectedDids,
                thresholdM,
                questionIds: [q1, q2, q3],
                answers: [a1, a2, a3],
            });
            onSuccess?.();
            onClose();
        } catch (err) {
            setErrorMsg(translateError(err, t));
        } finally {
            setIsSubmitting(false);
        }
    };

    if (!isOpen) return null;

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={onClose}
            title={t('backup.selectGuardiansTitle')}
        >
            <form onSubmit={handleSubmit} className="space-y-6">
                {errorMsg && (
                    <AlertBanner
                        variant="error"
                        onDismiss={() => setErrorMsg('')}
                    >
                        {errorMsg}
                    </AlertBanner>
                )}

                {/* Household hint */}
                <div className="p-4 bg-amber-500/10 border border-amber-500/20 rounded-2xl flex items-start gap-3">
                    <AlertTriangle className="text-amber-500 shrink-0 mt-0.5" size={18} />
                    <p className="text-xs text-theme-secondary leading-relaxed font-medium">
                        {t('backup.guardianHintHousehold')}
                    </p>
                </div>

                {/* Contact selection */}
                <div className="space-y-3">
                    <div className="flex items-center justify-between">
                        <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider flex items-center gap-1.5">
                            <Users size={16} className="text-theme-primary" />
                            {t('backup.selectGuardiansMinMax')}
                        </label>
                        <span className="text-xs font-bold text-theme-primary">
                            {selectedDids.length} / 5
                        </span>
                    </div>

                    {isLoadingContacts ? (
                        <div className="p-6 text-center text-xs text-theme-light">
                            {t('common.loading')}
                        </div>
                    ) : contacts.length === 0 ? (
                        <div className="p-4 bg-bg-app rounded-xl border border-theme-subtle text-xs text-theme-light text-center">
                            {t('contacts.noContactsFound')}
                        </div>
                    ) : (
                        <div className="max-h-48 overflow-y-auto space-y-1.5 p-1 border border-theme-subtle rounded-xl">
                            {contacts.map((contact) => {
                                const isSelected = selectedDids.includes(contact.did);
                                const displayName = (contact.profile?.firstName || contact.profile?.lastName)
                                    ? `${contact.profile?.firstName || ''} ${contact.profile?.lastName || ''}`.trim()
                                    : contact.profile?.organization || contact.did;
                                return (
                                    <button
                                        type="button"
                                        key={contact.did}
                                        onClick={() => handleToggleContact(contact.did)}
                                        data-testid={`select-contact-${contact.did}`}
                                        className={`w-full p-2.5 rounded-lg flex items-center justify-between text-left text-xs transition-colors ${
                                            isSelected
                                                ? 'bg-theme-primary/10 border border-theme-primary/30 text-theme-primary font-bold'
                                                : 'hover:bg-theme-subtle/40 text-theme-secondary'
                                        }`}
                                    >
                                        <div className="min-w-0 pr-2">
                                            <p className="truncate font-semibold">{displayName}</p>
                                            <p className="text-[10px] text-theme-light truncate opacity-70">
                                                {contact.did}
                                            </p>
                                        </div>
                                        {isSelected && (
                                            <CheckCircle2 size={16} className="text-theme-primary shrink-0" />
                                        )}
                                    </button>
                                );
                            })}
                        </div>
                    )}
                </div>

                {/* Threshold selection */}
                {selectedDids.length >= 2 && (
                    <div className="space-y-2">
                        <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider block">
                            {t('backup.thresholdSelectionLabel')}
                        </label>
                        <div className="flex gap-2">
                            {Array.from(
                                { length: selectedDids.length - 1 },
                                (_, i) => i + 2
                            ).map((m) => (
                                <button
                                    type="button"
                                    key={m}
                                    onClick={() => setThresholdM(m)}
                                    className={`flex-1 py-2 rounded-xl text-xs font-bold transition-all ${
                                        thresholdM === m
                                            ? 'bg-theme-primary text-white shadow-sm'
                                            : 'bg-bg-app border border-theme-subtle hover:border-theme-primary/50 text-theme-secondary'
                                    }`}
                                >
                                    {m} / {selectedDids.length}
                                </button>
                            ))}
                        </div>
                    </div>
                )}

                {/* 3 Personal Recovery Questions */}
                <div className="space-y-3 pt-2 border-t border-theme-subtle/50">
                    <div>
                        <h4 className="text-xs font-bold text-theme-secondary uppercase tracking-wider">
                            {t('backup.questionsSelectionTitle')}
                        </h4>
                        <p className="text-xs text-theme-light mt-0.5">
                            {t('backup.questionsSelectionHint')}
                        </p>
                    </div>

                    {/* Question 1 */}
                    <div className="space-y-1.5 p-3 bg-bg-app/50 rounded-xl border border-theme-subtle/60">
                        <select
                            value={q1}
                            onChange={(e) => setQ1(Number(e.target.value))}
                            className="w-full text-xs font-bold bg-bg-surface border border-theme-subtle rounded-lg p-2 text-theme-secondary focus:ring-1 focus:ring-theme-primary"
                        >
                            {QUESTION_CATALOG.map((item) => (
                                <option key={item.id} value={item.id} disabled={item.id === q2 || item.id === q3}>
                                    {t(item.key)}
                                </option>
                            ))}
                        </select>
                        <Input
                            type="text"
                            value={a1}
                            onChange={(e) => setA1(e.target.value)}
                            data-testid="guardian-answer-0"
                            className="text-xs py-1.5"
                        />
                    </div>

                    {/* Question 2 */}
                    <div className="space-y-1.5 p-3 bg-bg-app/50 rounded-xl border border-theme-subtle/60">
                        <select
                            value={q2}
                            onChange={(e) => setQ2(Number(e.target.value))}
                            className="w-full text-xs font-bold bg-bg-surface border border-theme-subtle rounded-lg p-2 text-theme-secondary focus:ring-1 focus:ring-theme-primary"
                        >
                            {QUESTION_CATALOG.map((item) => (
                                <option key={item.id} value={item.id} disabled={item.id === q1 || item.id === q3}>
                                    {t(item.key)}
                                </option>
                            ))}
                        </select>
                        <Input
                            type="text"
                            value={a2}
                            onChange={(e) => setA2(e.target.value)}
                            data-testid="guardian-answer-1"
                            className="text-xs py-1.5"
                        />
                    </div>

                    {/* Question 3 */}
                    <div className="space-y-1.5 p-3 bg-bg-app/50 rounded-xl border border-theme-subtle/60">
                        <select
                            value={q3}
                            onChange={(e) => setQ3(Number(e.target.value))}
                            className="w-full text-xs font-bold bg-bg-surface border border-theme-subtle rounded-lg p-2 text-theme-secondary focus:ring-1 focus:ring-theme-primary"
                        >
                            {QUESTION_CATALOG.map((item) => (
                                <option key={item.id} value={item.id} disabled={item.id === q1 || item.id === q2}>
                                    {t(item.key)}
                                </option>
                            ))}
                        </select>
                        <Input
                            type="text"
                            value={a3}
                            onChange={(e) => setA3(e.target.value)}
                            data-testid="guardian-answer-2"
                            className="text-xs py-1.5"
                        />
                    </div>

                    {!areAnswersValid && (a1 || a2 || a3) && (
                        <p className="text-[11px] text-amber-600 font-medium">
                            {t('backup.answersLengthValidation')}
                        </p>
                    )}
                </div>

                {/* Password confirmation */}
                <div className="space-y-1.5 pt-2 border-t border-theme-subtle/50">
                    <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider flex items-center gap-1.5">
                        <KeyRound size={15} className="text-theme-primary" />
                        {t('auth.password')}
                    </label>
                    <Input
                        type="password"
                        value={password}
                        onChange={(e) => setPassword(e.target.value)}
                        placeholder={t('auth.passwordPlaceholder')}
                        data-testid="guardian-password-input"
                        className="text-xs"
                    />
                </div>

                <div className="flex justify-end gap-3 pt-3">
                    <Button type="button" variant="outline" onClick={onClose} disabled={isSubmitting}>
                        {t('common.cancel')}
                    </Button>
                    <Button
                        type="submit"
                        variant="primary"
                        disabled={!canSubmit}
                        data-testid="guardian-setup-submit-btn"
                        className="flex items-center gap-2"
                    >
                        <ShieldCheck size={16} />
                        {isSubmitting ? t('common.saving') : t('backup.setupGuardiansButton')}
                    </Button>
                </div>
            </form>
        </ModalShell>
    );
};
