// src/components/settings/network/LockServerProbeSection.tsx
import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { RefreshCw, ShieldCheck, Trash2, CheckCircle2, XCircle } from 'lucide-react';
import { AppSettings, LockServerProbeResult } from '../../../types';
import { settingsService } from '../../../services/settingsService';
import { useSession } from '../../../context/SessionContext';
import { Button } from '../../ui/Button';
import { Input } from '../../ui/Input';
import { TechId } from '../../ui/TechId';
import { translateError } from '../../../utils/errorHelper';
import { formatQuarter } from '../../../utils/retentionQuarter';

/**
 * Formats a Unix timestamp (seconds) into a compact relative age label.
 */
export function formatAgeLabel(tsSecs: number | null | undefined): string {
    if (!tsSecs || tsSecs <= 0) return '—';
    const diffSecs = Math.max(0, Math.floor(Date.now() / 1000) - tsSecs);
    if (diffSecs < 60) return `${diffSecs}s`;
    const mins = Math.floor(diffSecs / 60);
    if (mins < 60) return `${mins}m`;
    const hours = Math.floor(mins / 60);
    if (hours < 48) return `${hours}h`;
    const days = Math.floor(hours / 24);
    return `${days}d`;
}

export interface LockServerProbeSectionProps {
    settings: AppSettings | null;
    probeList: LockServerProbeResult[];
    passingProbes: LockServerProbeResult[];
    onSettingsUpdated: (updater: AppSettings | ((prev: AppSettings | null) => AppSettings | null)) => void;
    onSuccess: (message: string) => void;
    onError: (message: string) => void;
    onClearFeedback: () => void;
    onRefreshSettings: () => Promise<void>;
}

/**
 * Verifies external lock servers and manages the user's own nominated guarantor lock server.
 */
export const LockServerProbeSection: React.FC<LockServerProbeSectionProps> = ({
    settings,
    probeList,
    passingProbes,
    onSettingsUpdated,
    onSuccess,
    onError,
    onClearFeedback,
    onRefreshSettings,
}) => {
    const { t } = useTranslation();
    const { protectAction } = useSession();

    const [probeUrl, setProbeUrl] = useState('');
    const [expectedAdminPub, setExpectedAdminPub] = useState('');
    const [isProbing, setIsProbing] = useState(false);
    const [lastProbe, setLastProbe] = useState<LockServerProbeResult | null>(null);

    const [ownServerUrl, setOwnServerUrl] = useState(settings?.ownNominatedLockServer?.writeUrl || '');
    const [ownServerAsn, setOwnServerAsn] = useState(settings?.ownNominatedLockServer?.asn || '');
    const [isSavingOwn, setIsSavingOwn] = useState(false);

    useEffect(() => {
        setOwnServerUrl(settings?.ownNominatedLockServer?.writeUrl || '');
        setOwnServerAsn(settings?.ownNominatedLockServer?.asn || '');
    }, [settings?.ownNominatedLockServer]);

    const handleProbe = async () => {
        const trimmed = probeUrl.trim();
        if (!trimmed) return;
        setIsProbing(true);
        onClearFeedback();
        try {
            const res = await protectAction(async (pwd) => {
                return await settingsService.probeLockServer(
                    trimmed,
                    expectedAdminPub.trim() || null,
                    pwd || null
                );
            });
            if (res) {
                setLastProbe(res);
                if (!res.passed && res.error) {
                    onError(res.error);
                } else if (res.passed) {
                    onSuccess(t('settings.lockProbePassedMsg', 'Server passed external verification.'));
                }
                await onRefreshSettings();
            }
        } catch (e) {
            onError(translateError(e, t));
        } finally {
            setIsProbing(false);
        }
    };

    const handleSaveOwnNominated = async (clear = false) => {
        setIsSavingOwn(true);
        onClearFeedback();
        try {
            const targetUrl = clear ? null : ownServerUrl.trim() || null;
            const targetAsn = clear ? null : ownServerAsn.trim() || null;
            const res = await protectAction(async (pwd) => {
                return await settingsService.setOwnNominatedLockServer(targetUrl, targetAsn, pwd || null);
            });
            onSettingsUpdated((s) => (s ? { ...s, ownNominatedLockServer: res ?? null } : s));
            onSuccess(
                clear
                    ? t('settings.ownLockClearedMsg', 'Own nominated lock server removed.')
                    : t('settings.ownLockSavedMsg', 'Own nominated lock server saved.')
            );
            await onRefreshSettings();
        } catch (e) {
            onError(translateError(e, t));
        } finally {
            setIsSavingOwn(false);
        }
    };

    const probeChecks = lastProbe
        ? [
              { passed: lastProbe.delegationValid, label: '?info Delegation' },
              { passed: lastProbe.wafProbePassed, label: 'WAF POST' },
              { passed: lastProbe.selftestPassed, label: '?lock_selftest' },
              { passed: lastProbe.keyLeakSafe, label: 'No Key Leak' },
          ]
        : [];

    return (
        <>
            {/* 1. External Server Probe */}
            <div className="space-y-3 p-4 rounded-xl bg-white dark:bg-slate-900 border border-theme-subtle">
                <div className="flex items-center justify-between gap-2">
                    <h4 className="text-xs font-black uppercase tracking-wider text-theme-primary flex items-center gap-1.5">
                        <RefreshCw size={13} />
                        {t('settings.lockProbeTitle', 'External Lock Server Verification')}
                    </h4>
                </div>
                <p className="text-[11px] text-theme-light leading-relaxed">
                    {t(
                        'settings.lockProbeDesc',
                        'Verifies ?info delegation, POST ?lock WAF probe, ?lock_selftest, key-leak protection, latency, and max retention quarter.'
                    )}
                </p>
                <div className="grid grid-cols-1 sm:grid-cols-3 gap-2">
                    <div className="sm:col-span-2">
                        <Input
                            value={probeUrl}
                            onChange={(e) => setProbeUrl(e.target.value)}
                            placeholder="https://sync.example.org/sync.php"
                            aria-label={t('settings.lockServerUrlLabel', 'Lock Server URL')}
                        />
                    </div>
                    <Input
                        value={expectedAdminPub}
                        onChange={(e) => setExpectedAdminPub(e.target.value)}
                        placeholder={t('settings.expectedAdminPubPlaceholder', 'Expected Admin Pub (optional hex)')}
                        aria-label={t('settings.expectedAdminPubPlaceholder', 'Expected Admin Pub (optional hex)')}
                    />
                </div>
                <div className="flex items-center justify-end">
                    <Button
                        type="button"
                        size="sm"
                        onClick={handleProbe}
                        disabled={isProbing || !probeUrl.trim()}
                    >
                        {isProbing
                            ? t('settings.probingLockServer', 'Probing...')
                            : t('settings.probeLockServerBtn', 'Verify Server')}
                    </Button>
                </div>

                {lastProbe && (
                    <div
                        data-testid="last-lock-probe-result"
                        className={`p-3 rounded-xl border text-xs space-y-1.5 ${
                            lastProbe.passed
                                ? 'bg-emerald-50/70 border-emerald-200 text-emerald-950'
                                : 'bg-rose-50/70 border-rose-200 text-rose-950'
                        }`}
                    >
                        <div className="flex items-center justify-between gap-2 font-bold">
                            <span className="truncate">{lastProbe.writeUrl}</span>
                            <span>
                                {lastProbe.latencyMs} ms · Max {formatQuarter(lastProbe.maxRetentionQuarter)}
                            </span>
                        </div>
                        <div className="flex flex-wrap gap-2 text-[10px]">
                            {probeChecks.map((chk) => (
                                <span key={chk.label} className="inline-flex items-center gap-1">
                                    {chk.passed ? <CheckCircle2 size={11} /> : <XCircle size={11} />}
                                    {chk.label}
                                </span>
                            ))}
                        </div>
                        <div className="text-[10px] flex flex-wrap items-center gap-2 opacity-80">
                            <span>Lock Pub:</span>
                            <TechId value={lastProbe.lockPub} kind="hash" />
                            {lastProbe.inodeWarning && (
                                <span className="px-1.5 py-0.5 rounded bg-amber-200 text-amber-900 font-bold">
                                    Inodes {lastProbe.usedInodes}/{lastProbe.maxInodes}
                                </span>
                            )}
                        </div>
                    </div>
                )}

                {probeList.length > 0 && (
                    <div className="space-y-1.5 pt-1">
                        <p className="text-[10px] font-bold uppercase tracking-wider text-theme-light">
                            {t('settings.cachedLockProbes', 'Verified Lock Servers')} ({passingProbes.length}/{probeList.length})
                        </p>
                        <div className="space-y-1.5 max-h-48 overflow-y-auto">
                            {probeList.map((p) => (
                                <div
                                    key={p.writeUrl}
                                    className="flex items-center justify-between gap-2 p-2 rounded-lg border border-theme-subtle/60 bg-bg-app/30 text-[11px]"
                                >
                                    <div className="min-w-0 flex-1">
                                        <div className="flex items-center gap-1.5 font-semibold truncate">
                                            {p.passed ? (
                                                <CheckCircle2 size={12} className="text-emerald-600 shrink-0" />
                                            ) : (
                                                <XCircle size={12} className="text-rose-500 shrink-0" />
                                            )}
                                            <span className="truncate">{p.writeUrl}</span>
                                        </div>
                                        <div className="text-[10px] text-theme-light">
                                            {p.latencyMs} ms · Max {formatQuarter(p.maxRetentionQuarter)} ·{' '}
                                            {t('settings.probeAgeLabel', 'Age')}: {formatAgeLabel(p.probedAtSecs)}
                                        </div>
                                    </div>
                                    {p.passed && (
                                        <button
                                            type="button"
                                            onClick={() => setOwnServerUrl(p.writeUrl)}
                                            className="text-[10px] font-bold text-theme-primary hover:underline shrink-0 cursor-pointer"
                                        >
                                            {t('settings.useAsOwnServer', 'Use as own')}
                                        </button>
                                    )}
                                </div>
                            ))}
                        </div>
                    </div>
                )}
            </div>

            {/* 2. Own Guarantor Nominated Lock Server */}
            <div className="space-y-3 p-4 rounded-xl bg-white dark:bg-slate-900 border border-theme-subtle">
                <h4 className="text-xs font-black uppercase tracking-wider text-theme-primary flex items-center gap-1.5">
                    <ShieldCheck size={13} />
                    {t('settings.ownNominatedServerTitle', 'Own Trusted Lock Server (Guarantor Check)')}
                </h4>
                <p className="text-[11px] text-theme-light leading-relaxed">
                    {t(
                        'settings.ownNominatedServerDesc',
                        'Verified server required to countersign vouched vouchers. Automatically treated as trusted at checkout.'
                    )}
                </p>
                <div className="grid grid-cols-1 sm:grid-cols-3 gap-2">
                    <div className="sm:col-span-2">
                        <Input
                            value={ownServerUrl}
                            onChange={(e) => setOwnServerUrl(e.target.value)}
                            placeholder="https://my-lock-server.example.org/sync.php"
                            aria-label={t('settings.ownLockUrlLabel', 'Own Lock Server URL')}
                        />
                    </div>
                    <Input
                        value={ownServerAsn}
                        onChange={(e) => setOwnServerAsn(e.target.value)}
                        placeholder={t('settings.asnOptionalPlaceholder', 'ASN (optional, e.g. AS24940)')}
                        aria-label={t('settings.asnOptionalPlaceholder', 'ASN (optional)')}
                    />
                </div>
                <div className="flex items-center justify-between gap-2">
                    {settings?.ownNominatedLockServer ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => handleSaveOwnNominated(true)}
                            disabled={isSavingOwn}
                        >
                            <Trash2 size={13} className="mr-1" />
                            {t('settings.clearOwnServerBtn', 'Remove')}
                        </Button>
                    ) : (
                        <span />
                    )}
                    <Button
                        type="button"
                        size="sm"
                        onClick={() => handleSaveOwnNominated(false)}
                        disabled={isSavingOwn || !ownServerUrl.trim()}
                    >
                        {t('settings.saveOwnServerBtn', 'Save Own Server')}
                    </Button>
                </div>
            </div>
        </>
    );
};
