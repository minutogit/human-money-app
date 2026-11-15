// src/components/settings/IdentityRecoveryCard.tsx
import React, { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { ShieldCheck, FileText, Users, Trash2 } from 'lucide-react';
import { RecoveryProtectionStatusDto } from '../../types';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { AlertBanner } from '../ui/AlertBanner';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { GuardianSetupModal } from '../GuardianSetupModal';
import { EmergencyKitModal } from '../EmergencyKitModal';
import { useSession } from '../../context/SessionContext';
import { recoveryService } from '../../services/recoveryService';
import { translateError } from '../../utils/errorHelper';

export const IdentityRecoveryCard: React.FC = () => {
    const { t } = useTranslation();
    const { protectAction } = useSession();

    const [protectionStatus, setProtectionStatus] = useState<RecoveryProtectionStatusDto | null>(null);
    const [isGuardianSetupOpen, setIsGuardianSetupOpen] = useState(false);
    const [isEmergencyKitOpen, setIsEmergencyKitOpen] = useState(false);
    const [isRevokingGuardians, setIsRevokingGuardians] = useState(false);
    const [isRevokeModalOpen, setIsRevokeModalOpen] = useState(false);
    const [revokeFeedback, setRevokeFeedback] = useState<{ type: 'error' | 'success'; text: string } | null>(null);

    const loadProtectionStatus = useCallback(async () => {
        if (typeof recoveryService?.getRecoveryProtectionStatus !== 'function') return;
        try {
            const st = await recoveryService.getRecoveryProtectionStatus();
            setProtectionStatus(st);
        } catch {
            // ignore
        }
    }, []);

    useEffect(() => {
        loadProtectionStatus();
    }, [loadProtectionStatus]);

    const handleRevokeGuardians = async () => {
        setIsRevokingGuardians(true);
        setRevokeFeedback(null);
        try {
            await protectAction(async (pwd) => {
                await recoveryService.revokeSocialRecovery(pwd || '');
            });
            setRevokeFeedback({ type: 'success', text: t('backup.revokeGuardiansSuccess') });
            await loadProtectionStatus();
        } catch (err) {
            setRevokeFeedback({ type: 'error', text: translateError(err, t) });
        } finally {
            setIsRevokingGuardians(false);
            setIsRevokeModalOpen(false);
        }
    };

    return (
        <div id="settings-recovery-section" className="space-y-6">
            <Card
                header={
                    <div className="flex items-center justify-between w-full">
                        <div className="flex items-center gap-2">
                            <ShieldCheck size={18} className="text-theme-primary" />
                            <span className="font-black text-xs uppercase tracking-widest">
                                {t('backup.socialRecoveryTitle')}
                            </span>
                        </div>
                        <span
                            className={`text-[10px] font-bold px-2 py-0.5 rounded-full ${
                                protectionStatus?.hasIdentityRecovery
                                    ? 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300'
                                    : 'bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300'
                            }`}
                        >
                            {protectionStatus?.hasIdentityRecovery
                                ? t('backup.emergencyKitConfirmedBadge')
                                : t('backup.emergencyKitNotConfirmedBadge')}
                        </span>
                    </div>
                }
            >
                <div className="space-y-6">
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                        {/* Emergency Sheet Card */}
                        <div className="p-4 rounded-2xl border border-theme-subtle bg-bg-surface flex flex-col justify-between space-y-4">
                            <div className="space-y-2">
                                <div className="flex items-center justify-between">
                                    <div className="flex items-center gap-2">
                                        <FileText size={16} className="text-theme-primary" />
                                        <h4 className="text-xs font-bold text-theme-secondary">
                                            {t('backup.emergencyKitStatusLabel')}
                                        </h4>
                                    </div>
                                    <span
                                        className={`text-[9px] font-bold px-2 py-0.5 rounded-full ${
                                            protectionStatus?.emergencySheetConfirmedAt
                                                ? 'bg-emerald-100 text-emerald-800'
                                                : 'bg-slate-100 text-slate-600'
                                        }`}
                                    >
                                        {protectionStatus?.emergencySheetConfirmedAt
                                            ? t('backup.emergencyKitConfirmedBadge')
                                            : t('backup.emergencyKitNotConfirmedBadge')}
                                    </span>
                                </div>
                                <p className="text-[11px] text-theme-light leading-relaxed">
                                    {t('backup.emergencyKitDesc')}
                                </p>
                            </div>
                            <Button
                                type="button"
                                variant="secondary"
                                onClick={() => setIsEmergencyKitOpen(true)}
                                className="w-full text-xs py-2 gap-1.5"
                            >
                                <FileText size={14} />
                                <span>{t('backup.emergencyKitPrintButton')}</span>
                            </Button>
                        </div>

                        {/* Social Recovery / Guardians Card */}
                        <div className="p-4 rounded-2xl border border-theme-subtle bg-bg-surface flex flex-col justify-between space-y-4">
                            <div className="space-y-2">
                                <div className="flex items-center justify-between">
                                    <div className="flex items-center gap-2">
                                        <Users size={16} className="text-theme-primary" />
                                        <h4 className="text-xs font-bold text-theme-secondary">
                                            {t('backup.socialRecoveryStatusLabel')}
                                        </h4>
                                    </div>
                                    <span
                                        className={`text-[9px] font-bold px-2 py-0.5 rounded-full ${
                                            protectionStatus?.guardians && protectionStatus.guardians.length > 0
                                                ? 'bg-emerald-100 text-emerald-800'
                                                : 'bg-slate-100 text-slate-600'
                                        }`}
                                    >
                                        {protectionStatus?.guardians && protectionStatus.guardians.length > 0
                                            ? `${protectionStatus.requiredThreshold}-von-${protectionStatus.guardians.length}`
                                            : t('backup.emergencyKitNotConfirmedBadge')}
                                    </span>
                                </div>
                                <p className="text-[11px] text-theme-light leading-relaxed">
                                    {t('backup.socialRecoveryDesc')}
                                </p>
                            </div>
                            <div className="flex gap-2">
                                <Button
                                    type="button"
                                    variant="secondary"
                                    onClick={() => setIsGuardianSetupOpen(true)}
                                    className="flex-1 text-xs py-2 gap-1.5"
                                >
                                    <Users size={14} />
                                    <span>{t('backup.manageGuardiansButton')}</span>
                                </Button>
                                {protectionStatus?.guardians && protectionStatus.guardians.length > 0 && (
                                    <Button
                                        type="button"
                                        variant="danger"
                                        onClick={() => setIsRevokeModalOpen(true)}
                                        className="text-xs py-2 px-3"
                                        title={t('backup.revokeGuardiansButton')}
                                    >
                                        <Trash2 size={14} />
                                    </Button>
                                )}
                            </div>
                        </div>
                    </div>

                    {/* Guardian Health Status List */}
                    {protectionStatus?.guardians && protectionStatus.guardians.length > 0 && (
                        <div className="space-y-2 pt-2 border-t border-theme-subtle/50">
                            <h5 className="text-[10px] font-black uppercase tracking-wider text-theme-light">
                                {t('backup.emergencyKitGuardianNames')}
                            </h5>
                            <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                                {protectionStatus.guardians.map((g) => {
                                    const isConfirmed = g.status.status === 'confirmed';
                                    const isStale = g.isStale90d;
                                    return (
                                        <div
                                            key={g.userId}
                                            className="p-2.5 rounded-xl border border-theme-subtle/70 bg-bg-app flex items-center justify-between text-xs"
                                        >
                                            <div className="min-w-0 pr-2">
                                                <p className="font-bold text-theme-secondary truncate">
                                                    {g.displayName || g.userId.slice(0, 16) + '...'}
                                                </p>
                                                <p className="text-[10px] text-theme-light font-mono truncate">
                                                    {g.userId.slice(0, 20)}...
                                                </p>
                                            </div>
                                            <span
                                                className={`text-[9px] font-bold px-2 py-0.5 rounded-full shrink-0 ${
                                                    isStale
                                                        ? 'bg-amber-100 text-amber-800'
                                                        : isConfirmed
                                                        ? 'bg-emerald-100 text-emerald-800'
                                                        : 'bg-slate-100 text-slate-600'
                                                }`}
                                            >
                                                {isStale
                                                    ? t('backup.guardianStatusStale90d')
                                                    : isConfirmed
                                                    ? t('backup.guardianStatusConfirmed')
                                                    : t('backup.guardianStatusPending')}
                                            </span>
                                        </div>
                                    );
                                })}
                            </div>
                        </div>
                    )}

                    {revokeFeedback && (
                        <AlertBanner variant={revokeFeedback.type}>{revokeFeedback.text}</AlertBanner>
                    )}
                </div>
            </Card>

            <GuardianSetupModal
                isOpen={isGuardianSetupOpen}
                onClose={() => {
                    setIsGuardianSetupOpen(false);
                    loadProtectionStatus();
                }}
            />

            <EmergencyKitModal
                isOpen={isEmergencyKitOpen}
                onClose={() => {
                    setIsEmergencyKitOpen(false);
                    loadProtectionStatus();
                }}
            />

            <ConfirmationModal
                isOpen={isRevokeModalOpen}
                title={t('backup.revokeGuardiansButton')}
                description={t('backup.guardianHintHousehold')}
                confirmVariant="danger"
                confirmText={t('backup.revokeGuardiansButton')}
                isProcessing={isRevokingGuardians}
                onConfirm={handleRevokeGuardians}
                onCancel={() => setIsRevokeModalOpen(false)}
            />
        </div>
    );
};

export default IdentityRecoveryCard;
