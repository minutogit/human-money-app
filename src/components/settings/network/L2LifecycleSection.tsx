// src/components/settings/network/L2LifecycleSection.tsx
import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { RefreshCw } from 'lucide-react';
import { L2LifecycleStatusDto } from '../../../types';
import { settingsService } from '../../../services/settingsService';
import { protectCommandOptions } from '../../../services/commandPolicy';
import { useSession } from '../../../context/SessionContext';
import { Button } from '../../ui/Button';
import { Input } from '../../ui/Input';
import { TechId } from '../../ui/TechId';
import { translateError } from '../../../utils/errorHelper';

export interface L2LifecycleSectionProps {
    onSuccess: (message: string) => void;
    onError: (message: string) => void;
    onClearFeedback: () => void;
}

/**
 * Displays committee health, rotation state, automatic vetoes, and guarantor role registration.
 */
export const L2LifecycleSection: React.FC<L2LifecycleSectionProps> = ({
    onSuccess,
    onError,
    onClearFeedback,
}) => {
    const { t } = useTranslation();
    const { protectAction } = useSession();

    const [lifecycleStatus, setLifecycleStatus] = useState<L2LifecycleStatusDto | null>(null);
    const [isRunningLifecycle, setIsRunningLifecycle] = useState(false);
    const [regAnchorHex, setRegAnchorHex] = useState('');
    const [regGuarantorIdx, setRegGuarantorIdx] = useState('0');
    const [regContextNonceHex, setRegContextNonceHex] = useState('');
    const [isRegisteringRole, setIsRegisteringRole] = useState(false);
    const [registeredGuarantorPub, setRegisteredGuarantorPub] = useState<string | null>(null);

    useEffect(() => {
        if (typeof settingsService.getL2LifecycleStatus === 'function') {
            settingsService
                .getL2LifecycleStatus()
                .then(setLifecycleStatus)
                .catch(() => {});
        }
    }, []);

    const handleRunLifecycleTick = async () => {
        setIsRunningLifecycle(true);
        onClearFeedback();
        try {
            const st = await protectAction(async (pwd) => {
                return await settingsService.runL2LifecycleTick(true, pwd || null);
            });
            if (st) {
                setLifecycleStatus(st);
                onSuccess(t('settings.lifecycleTickCompleteMsg'));
            }
        } catch (e) {
            onError(translateError(e, t));
        } finally {
            setIsRunningLifecycle(false);
        }
    };

    const handleRegisterGuarantorRole = async () => {
        if (!regAnchorHex.trim()) return;
        const idx = Number.parseInt(regGuarantorIdx, 10);
        if (!Number.isFinite(idx) || idx < 0) return;
        setIsRegisteringRole(true);
        onClearFeedback();
        try {
            const pub = await protectAction(async (pwd) => {
                return await settingsService.registerGuarantorAnchorRole({
                    anchorIdHex: regAnchorHex.trim(),
                    guarantorIndex: idx,
                    contextNonceHex: regContextNonceHex.trim() || null,
                    password: pwd || null,
                });
            }, protectCommandOptions('register_guarantor_anchor_role'));
            if (pub) {
                setRegisteredGuarantorPub(pub);
                const st = await settingsService.getL2LifecycleStatus().catch(() => null);
                if (st) setLifecycleStatus(st);
                onSuccess(t('settings.guarantorRoleRegisteredMsg'));
            }
        } catch (e) {
            onError(translateError(e, t));
        } finally {
            setIsRegisteringRole(false);
        }
    };

    const lifecycleStats = lifecycleStatus
        ? [
              { label: t('settings.lifecycleTrackedAnchors'), value: lifecycleStatus.trackedAnchorsCount },
              { label: t('settings.lifecycleGuarantorRoles'), value: lifecycleStatus.guarantorRolesCount },
              { label: t('settings.lifecycleOffenders'), value: lifecycleStatus.serverOffendersCount },
              { label: t('settings.lifecycleAutoVetoes'), value: lifecycleStatus.autoVetoesSent },
          ]
        : [];

    return (
        <div data-testid="l2-lifecycle-panel" className="space-y-3 pt-4 border-t border-theme-subtle/50">
            <div className="flex items-center justify-between gap-2">
                <div>
                    <h4 className="text-xs font-bold uppercase tracking-wider text-theme-primary flex items-center gap-1.5">
                        <RefreshCw size={14} className="text-theme-primary" />
                        {t('settings.lifecycleTitle')}
                    </h4>
                    <p className="text-xs text-theme-secondary/80 mt-0.5">
                        {t('settings.lifecycleDesc')}
                    </p>
                </div>
                <Button
                    type="button"
                    size="sm"
                    variant="secondary"
                    onClick={handleRunLifecycleTick}
                    disabled={isRunningLifecycle}
                >
                    {isRunningLifecycle
                        ? t('settings.runningLifecycleTick')
                        : t('settings.runLifecycleTickBtn')}
                </Button>
            </div>

            {lifecycleStatus && (
                <div className="space-y-2">
                    <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 text-[11px]">
                        {lifecycleStats.map((stat) => (
                            <div
                                key={stat.label}
                                className="p-2 rounded-lg border border-theme-subtle bg-bg-app/30"
                            >
                                <p className="text-[10px] text-theme-light">{stat.label}</p>
                                <p className="font-bold">{stat.value}</p>
                            </div>
                        ))}
                    </div>
                    {lifecycleStatus.lastVetoReason && (
                        <p className="text-[11px] text-amber-800 bg-amber-50 border border-amber-200 rounded-lg p-2">
                            {lifecycleStatus.lastVetoReason}
                        </p>
                    )}
                    {lifecycleStatus.anchors.map((a) => (
                        <div
                            key={a.anchorId}
                            className="p-2.5 rounded-lg border border-theme-subtle/70 bg-bg-app/20 text-[11px] space-y-1"
                        >
                            <div className="flex items-center justify-between gap-2">
                                <TechId value={a.anchorId} kind="hash" />
                                <div className="flex items-center gap-1.5">
                                    <span className="font-mono font-bold">Rev {a.revision}</span>
                                    {a.reducedGuarantorProtection && (
                                        <span className="px-1.5 py-0.5 rounded bg-amber-100 text-amber-900 font-bold text-[10px]">
                                            {t('voucher.reducedGuarantorProtectionBadge')}
                                        </span>
                                    )}
                                    {a.activeRotationPhase && (
                                        <span className="px-1.5 py-0.5 rounded bg-indigo-100 text-indigo-900 font-bold text-[10px]">
                                            {a.activeRotationPhase}
                                        </span>
                                    )}
                                </div>
                            </div>
                            {(a.unhealthyServers.length > 0 || a.revokedServers.length > 0) && (
                                <p className="text-[10px] text-rose-700">
                                    {t('settings.lifecycleDegradedCounts', {
                                        unhealthy: a.unhealthyServers.length,
                                        revoked: a.revokedServers.length,
                                    })}
                                </p>
                            )}
                        </div>
                    ))}
                </div>
            )}

            <div className="pt-2 border-t border-theme-subtle/40 space-y-2">
                <p className="text-[11px] font-bold text-theme-secondary">
                    {t('settings.registerGuarantorRoleTitle')}
                </p>
                <div className="grid grid-cols-1 sm:grid-cols-3 gap-2">
                    <Input
                        value={regAnchorHex}
                        onChange={(e) => setRegAnchorHex(e.target.value)}
                        placeholder={t('settings.guarantorAnchorHexPlaceholder')}
                    />
                    <Input
                        value={regGuarantorIdx}
                        onChange={(e) => setRegGuarantorIdx(e.target.value)}
                        placeholder={t('settings.guarantorIdxPlaceholder')}
                    />
                    <Input
                        value={regContextNonceHex}
                        onChange={(e) => setRegContextNonceHex(e.target.value)}
                        placeholder={t('settings.guarantorContextNoncePlaceholder')}
                    />
                </div>
                <div className="flex items-center justify-between gap-2">
                    {registeredGuarantorPub ? (
                        <div className="flex items-center gap-1.5 text-[11px]">
                            <span className="text-theme-light">{t('settings.derivedGuarantorKeyLabel')}:</span>
                            <TechId value={registeredGuarantorPub} kind="hash" />
                        </div>
                    ) : (
                        <span />
                    )}
                    <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={handleRegisterGuarantorRole}
                        disabled={isRegisteringRole || !regAnchorHex.trim()}
                    >
                        {t('settings.registerGuarantorRoleBtn')}
                    </Button>
                </div>
            </div>
        </div>
    );
};
