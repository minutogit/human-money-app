// src/components/settings/NetworkProtectionCard.tsx
import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import {
    Network,
    Server,
    ShieldCheck,
    ShieldAlert,
    Plus,
    Trash2,
    RefreshCw,
    Wifi,
    WifiOff,
    CheckCircle2,
    XCircle,
} from 'lucide-react';
import {
    AppSettings,
    LockServerProbeResult,
    AnchorServerInputDto,
} from '../../types';
import { settingsService } from '../../services/settingsService';
import { useSession } from '../../context/SessionContext';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { HelpIcon } from '../ui/HelpIcon';
import { AdvancedDisclosure } from '../ui/AdvancedDisclosure';
import { TechId } from '../ui/TechId';
import { translateError, stringifyError } from '../../utils/errorHelper';
import { logger } from '../../utils/log';
import { defaultQuarterYearsAhead, formatQuarter, parseQuarterStr } from '../../utils/retentionQuarter';

export interface NetworkProtectionCardProps {
    settings: AppSettings | null;
    onSettingsUpdated: (updater: AppSettings | ((prev: AppSettings | null) => AppSettings | null)) => void;
    isDevUnlocked: boolean;
    initialOpen?: boolean;
}

interface DraftServerSelection {
    writeUrl: string;
    nominatedBy: 'creator' | 'guarantor';
    asn: string;
}

function formatAgeLabel(tsSecs: number | null | undefined): string {
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

export const NetworkProtectionCard: React.FC<NetworkProtectionCardProps> = ({
    settings,
    onSettingsUpdated,
    isDevUnlocked,
    initialOpen = false,
}) => {
    const { t } = useTranslation();
    const { protectAction } = useSession();

    const [probeUrl, setProbeUrl] = useState('');
    const [expectedAdminPub, setExpectedAdminPub] = useState('');
    const [isProbing, setIsProbing] = useState(false);
    const [lastProbe, setLastProbe] = useState<LockServerProbeResult | null>(null);
    const [actionError, setActionError] = useState<string | null>(null);
    const [actionSuccess, setActionSuccess] = useState<string | null>(null);

    const [ownServerUrl, setOwnServerUrl] = useState(settings?.ownNominatedLockServer?.writeUrl || '');
    const [ownServerAsn, setOwnServerAsn] = useState(settings?.ownNominatedLockServer?.asn || '');
    const [isSavingOwn, setIsSavingOwn] = useState(false);

    const [tplLabel, setTplLabel] = useState('');
    const [tplQuarter, setTplQuarter] = useState(() => defaultQuarterYearsAhead(5));
    const [tplThreshold, setTplThreshold] = useState('');
    const [tplGuarantorKeys, setTplGuarantorKeys] = useState('');
    const [selectedServers, setSelectedServers] = useState<DraftServerSelection[]>([]);
    const [isAssembling, setIsAssembling] = useState(false);

    useEffect(() => {
        setOwnServerUrl(settings?.ownNominatedLockServer?.writeUrl || '');
        setOwnServerAsn(settings?.ownNominatedLockServer?.asn || '');
    }, [settings?.ownNominatedLockServer]);

    const probeList = Object.values(settings?.l2ProbeCache || {}).sort(
        (a, b) => (b.probedAtSecs || 0) - (a.probedAtSecs || 0)
    );
    const passingProbes = probeList.filter((p) => p.passed);
    const templates = settings?.l2AnchorTemplatesMeta || [];
    const hasActiveLockConfig = Boolean(templates.length > 0 || settings?.ownNominatedLockServer);

    const refreshSettings = async () => {
        try {
            const fresh = await settingsService.getSettings();
            if (fresh) {
                onSettingsUpdated(fresh);
            }
        } catch (e) {
            logger.warn(`Failed to reload settings after lock update: ${stringifyError(e)}`);
        }
    };

    const handleProbe = async () => {
        const trimmed = probeUrl.trim();
        if (!trimmed) return;
        setIsProbing(true);
        setActionError(null);
        setActionSuccess(null);
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
                    setActionError(res.error);
                } else if (res.passed) {
                    setActionSuccess(t('settings.lockProbePassedMsg', 'Server passed external verification.'));
                }
                await refreshSettings();
            }
        } catch (e) {
            setActionError(translateError(e, t));
        } finally {
            setIsProbing(false);
        }
    };

    const handleSaveOwnNominated = async (clear = false) => {
        setIsSavingOwn(true);
        setActionError(null);
        setActionSuccess(null);
        try {
            const targetUrl = clear ? null : ownServerUrl.trim() || null;
            const targetAsn = clear ? null : ownServerAsn.trim() || null;
            const res = await protectAction(async (pwd) => {
                return await settingsService.setOwnNominatedLockServer(targetUrl, targetAsn, pwd || null);
            });
            onSettingsUpdated((s) => (s ? { ...s, ownNominatedLockServer: res ?? null } : s));
            setActionSuccess(
                clear
                    ? t('settings.ownLockClearedMsg', 'Own nominated lock server removed.')
                    : t('settings.ownLockSavedMsg', 'Own nominated lock server saved.')
            );
            await refreshSettings();
        } catch (e) {
            setActionError(translateError(e, t));
        } finally {
            setIsSavingOwn(false);
        }
    };

    const toggleServerInDraft = (writeUrl: string) => {
        setSelectedServers((prev) => {
            const exists = prev.some((s) => s.writeUrl === writeUrl);
            if (exists) {
                return prev.filter((s) => s.writeUrl !== writeUrl);
            }
            if (prev.length >= 5) return prev;
            const isOwn = settings?.ownNominatedLockServer?.writeUrl === writeUrl;
            return [
                ...prev,
                {
                    writeUrl,
                    nominatedBy: isOwn ? 'guarantor' : 'creator',
                    asn: isOwn ? settings?.ownNominatedLockServer?.asn || '' : '',
                },
            ];
        });
    };

    const updateDraftServer = (
        writeUrl: string,
        patch: Partial<DraftServerSelection>
    ) => {
        setSelectedServers((prev) =>
            prev.map((s) => (s.writeUrl === writeUrl ? { ...s, ...patch } : s))
        );
    };

    const handleAssembleTemplate = async () => {
        if (selectedServers.length === 0) return;
        const parsedQ = parseQuarterStr(tplQuarter);
        if (parsedQ === null) {
            setActionError(t('settings.invalidQuarterFormat', 'Invalid quarter format. Expected YYYY-QN (e.g. 2031-Q4).'));
            return;
        }
        setIsAssembling(true);
        setActionError(null);
        setActionSuccess(null);
        try {
            let guarantorIdx = 0;
            const serverInputs: AnchorServerInputDto[] = selectedServers.map((s) => ({
                writeUrl: s.writeUrl,
                nominatedBy: s.nominatedBy,
                guarantorIndex: s.nominatedBy === 'guarantor' ? guarantorIdx++ : null,
                asn: s.asn.trim() || null,
            }));
            const guarantorKeys = tplGuarantorKeys
                .split(/[\s,;]+/)
                .map((k) => k.trim())
                .filter(Boolean);
            const parsedThresh = tplThreshold.trim()
                ? Number.parseInt(tplThreshold.trim(), 10)
                : null;

            await protectAction(async (pwd) => {
                return await settingsService.assembleAnchorTemplate({
                    label: tplLabel.trim() || `Set (${selectedServers.length})`,
                    retainUntilQuarterStr: tplQuarter.trim().toUpperCase(),
                    servers: serverInputs,
                    guarantorMigPubsHex: guarantorKeys,
                    threshold: Number.isFinite(parsedThresh as number) ? parsedThresh : null,
                    password: pwd || null,
                });
            });

            setTplLabel('');
            setTplThreshold('');
            setTplGuarantorKeys('');
            setSelectedServers([]);
            setActionSuccess(t('settings.anchorTemplateSavedMsg', 'Anchor set template saved.'));
            await refreshSettings();
        } catch (e) {
            setActionError(translateError(e, t));
        } finally {
            setIsAssembling(false);
        }
    };

    return (
        <div id="settings-network-section">
            <Card
                header={
                    <div className="flex items-center gap-2">
                        <Network size={18} className="text-theme-primary" />
                        <span className="font-black text-xs uppercase tracking-widest">
                            {t('settings.l2ProtectionTitle')}
                        </span>
                        <HelpIcon topic="l2Protection" size={14} />
                        {isDevUnlocked && (
                            <span className="ml-2 text-[9px] font-bold px-1.5 py-0.5 rounded bg-amber-100 text-amber-800 border border-amber-200">
                                DEV
                            </span>
                        )}
                    </div>
                }
            >
                <div className="space-y-6">
                    {/* Overview Banner */}
                    <div
                        className={`p-4 rounded-2xl border flex items-start gap-3.5 ${
                            hasActiveLockConfig
                                ? 'bg-emerald-500/10 border-emerald-500/20 text-emerald-950 dark:text-emerald-200'
                                : 'bg-slate-500/10 border-slate-500/20 text-slate-900 dark:text-slate-200'
                        }`}
                    >
                        {hasActiveLockConfig ? (
                            <Wifi size={20} className="text-emerald-600 shrink-0 mt-0.5" />
                        ) : (
                            <WifiOff size={20} className="text-slate-600 shrink-0 mt-0.5" />
                        )}
                        <div className="space-y-1">
                            <div className="flex items-center gap-2">
                                <span className="text-xs font-black uppercase tracking-wider">
                                    {hasActiveLockConfig
                                        ? t('settings.l2ModeConnectedTitle')
                                        : t('settings.l2ModePureOfflineTitle')}
                                </span>
                                <span
                                    className={`text-[10px] font-bold px-2 py-0.5 rounded-full ${
                                        hasActiveLockConfig
                                            ? 'bg-emerald-100 text-emerald-800'
                                            : 'bg-slate-200 text-slate-700'
                                    }`}
                                >
                                    {hasActiveLockConfig ? 'Sync-Locks' : 'N = 0'}
                                </span>
                            </div>
                            <p className="text-xs text-theme-secondary/80 leading-relaxed font-medium">
                                {hasActiveLockConfig
                                    ? t('settings.l2ModeConnectedDesc')
                                    : t('settings.l2ModePureOfflineDesc')}
                            </p>
                        </div>
                    </div>

                    {actionError && (
                        <div className="p-3 rounded-xl bg-rose-50 border border-rose-200 text-xs text-rose-800 font-medium flex items-start gap-2">
                            <ShieldAlert size={16} className="shrink-0 mt-0.5 text-rose-600" />
                            <span>{actionError}</span>
                        </div>
                    )}
                    {actionSuccess && (
                        <div className="p-3 rounded-xl bg-emerald-50 border border-emerald-200 text-xs text-emerald-800 font-medium flex items-start gap-2">
                            <CheckCircle2 size={16} className="shrink-0 mt-0.5 text-emerald-600" />
                            <span>{actionSuccess}</span>
                        </div>
                    )}

                    <AdvancedDisclosure
                        icon={<Server size={14} className="text-theme-primary" />}
                        title={
                            <span className="text-xs font-bold text-theme-secondary">
                                {t('settings.advancedNetworkToggle')}
                            </span>
                        }
                        defaultOpen={initialOpen || hasActiveLockConfig || probeList.length > 0}
                        isCustom={Boolean(settings?.enablePosMode || hasActiveLockConfig)}
                        className="border border-theme-subtle rounded-2xl overflow-hidden"
                        buttonClassName="w-full flex items-center justify-between gap-2 p-4 bg-slate-50/70 dark:bg-slate-800/40 hover:bg-slate-100/70 transition-colors text-left cursor-pointer"
                        contentClassName="p-4 space-y-6 border-t border-theme-subtle"
                    >
                        {/* PoS Mode Toggle */}
                        <div className="flex items-start gap-3 p-3 bg-white dark:bg-slate-900 border border-theme-subtle rounded-xl hover:border-theme-primary/40 transition-colors">
                            <input
                                type="checkbox"
                                id="enablePosMode"
                                checked={!!settings?.enablePosMode}
                                onChange={(e) =>
                                    onSettingsUpdated((s) =>
                                        s ? { ...s, enablePosMode: e.target.checked } : null
                                    )
                                }
                                className="mt-0.5 accent-theme-primary"
                            />
                            <div className="flex-1">
                                <div className="flex items-center gap-2">
                                    <label
                                        htmlFor="enablePosMode"
                                        className="text-xs font-medium text-theme-secondary leading-relaxed cursor-pointer"
                                    >
                                        {t('settings.enablePosMode')}
                                    </label>
                                    <HelpIcon topic="posMode" size={14} />
                                </div>
                                <label
                                    htmlFor="enablePosMode"
                                    className="block text-[10px] text-theme-light/60 font-normal cursor-pointer"
                                >
                                    {t('settings.enablePosModeDesc')}
                                </label>
                            </div>
                        </div>

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
                                        <span className="inline-flex items-center gap-1">
                                            {lastProbe.delegationValid ? <CheckCircle2 size={11} /> : <XCircle size={11} />}
                                            ?info Delegation
                                        </span>
                                        <span className="inline-flex items-center gap-1">
                                            {lastProbe.wafProbePassed ? <CheckCircle2 size={11} /> : <XCircle size={11} />}
                                            WAF POST
                                        </span>
                                        <span className="inline-flex items-center gap-1">
                                            {lastProbe.selftestPassed ? <CheckCircle2 size={11} /> : <XCircle size={11} />}
                                            ?lock_selftest
                                        </span>
                                        <span className="inline-flex items-center gap-1">
                                            {lastProbe.keyLeakSafe ? <CheckCircle2 size={11} /> : <XCircle size={11} />}
                                            No Key Leak
                                        </span>
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

                        {/* 3. Anchor Set Templates (1..=5 Servers) */}
                        <div className="space-y-3 p-4 rounded-xl bg-white dark:bg-slate-900 border border-theme-subtle">
                            <h4 className="text-xs font-black uppercase tracking-wider text-theme-primary flex items-center gap-1.5">
                                <Plus size={13} />
                                {t('settings.anchorTemplatesTitle', 'Lock Committee Templates (1–5 Servers)')}
                            </h4>
                            <p className="text-[11px] text-theme-light leading-relaxed">
                                {t(
                                    'settings.anchorTemplatesDesc',
                                    'Combine 1 to 5 independently verified servers into a reusable double-spend protection template for new vouchers.'
                                )}
                            </p>

                            <div className="grid grid-cols-1 sm:grid-cols-3 gap-2">
                                <Input
                                    value={tplLabel}
                                    onChange={(e) => setTplLabel(e.target.value)}
                                    placeholder={t('settings.anchorLabelPlaceholder', 'Template name (e.g. Regional 2-of-3)')}
                                />
                                <Input
                                    value={tplQuarter}
                                    onChange={(e) => setTplQuarter(e.target.value)}
                                    placeholder="2031-Q4"
                                    aria-label={t('settings.retainUntilQuarterLabel', 'Retain until quarter')}
                                />
                                <Input
                                    value={tplThreshold}
                                    onChange={(e) => setTplThreshold(e.target.value)}
                                    placeholder={t('settings.quorumOptionalPlaceholder', 'Threshold Q (default majority)')}
                                />
                            </div>

                            {passingProbes.length > 0 ? (
                                <div className="space-y-1.5">
                                    <p className="text-[10px] font-bold uppercase tracking-wider text-theme-light">
                                        {t('settings.selectCommitteeServers', 'Select 1–5 verified servers')}
                                    </p>
                                    {passingProbes.map((p) => {
                                        const draft = selectedServers.find((s) => s.writeUrl === p.writeUrl);
                                        return (
                                            <div
                                                key={p.writeUrl}
                                                className="p-2 rounded-lg border border-theme-subtle/70 flex flex-wrap items-center justify-between gap-2 text-xs"
                                            >
                                                <label className="flex items-center gap-2 cursor-pointer min-w-0 flex-1">
                                                    <input
                                                        type="checkbox"
                                                        checked={Boolean(draft)}
                                                        onChange={() => toggleServerInDraft(p.writeUrl)}
                                                        className="accent-theme-primary"
                                                    />
                                                    <span className="truncate font-medium">{p.writeUrl}</span>
                                                    <span className="text-[10px] text-theme-light shrink-0">
                                                        (≤ {formatQuarter(p.maxRetentionQuarter)})
                                                    </span>
                                                </label>
                                                {draft && (
                                                    <div className="flex items-center gap-1.5">
                                                        <select
                                                            value={draft.nominatedBy}
                                                            onChange={(e) =>
                                                                updateDraftServer(p.writeUrl, {
                                                                    nominatedBy: e.target.value as 'creator' | 'guarantor',
                                                                })
                                                            }
                                                            className="text-[11px] bg-white dark:bg-slate-800 border border-theme-subtle rounded-lg px-2 py-1"
                                                        >
                                                            <option value="creator">Creator</option>
                                                            <option value="guarantor">Guarantor</option>
                                                        </select>
                                                        <input
                                                            type="text"
                                                            value={draft.asn}
                                                            onChange={(e) =>
                                                                updateDraftServer(p.writeUrl, { asn: e.target.value })
                                                            }
                                                            placeholder="ASN"
                                                            className="w-20 text-[11px] bg-white dark:bg-slate-800 border border-theme-subtle rounded-lg px-2 py-1"
                                                        />
                                                    </div>
                                                )}
                                            </div>
                                        );
                                    })}
                                </div>
                            ) : (
                                <p className="text-[11px] italic text-theme-light">
                                    {t('settings.probeServersFirstHint', 'Verify at least one server above before assembling a template.')}
                                </p>
                            )}

                            <Input
                                value={tplGuarantorKeys}
                                onChange={(e) => setTplGuarantorKeys(e.target.value)}
                                placeholder={t('settings.guarantorMigKeysPlaceholder', 'Guarantor rotation keys (optional, comma-separated 64-hex)')}
                            />

                            <div className="flex justify-end">
                                <Button
                                    type="button"
                                    size="sm"
                                    onClick={handleAssembleTemplate}
                                    disabled={isAssembling || selectedServers.length === 0}
                                >
                                    {isAssembling
                                        ? t('common.saving', 'Saving...')
                                        : t('settings.createAnchorTemplateBtn', 'Assemble Anchor Set')}
                                </Button>
                            </div>

                            {templates.length > 0 && (
                                <div className="space-y-2 pt-2 border-t border-theme-subtle/50">
                                    <p className="text-[10px] font-bold uppercase tracking-wider text-theme-light">
                                        {t('settings.savedAnchorTemplates', 'Saved Anchor Sets')} ({templates.length})
                                    </p>
                                    {templates.map((tpl) => (
                                        <div
                                            key={tpl.anchorId}
                                            className="p-3 rounded-xl border border-theme-subtle bg-bg-app/30 text-xs space-y-1"
                                        >
                                            <div className="flex items-center justify-between gap-2 font-bold">
                                                <span>{tpl.label}</span>
                                                <span className="text-[11px] font-mono">
                                                    {tpl.threshold}/{tpl.servers.length} · ≥ {formatQuarter(tpl.retainUntilQuarter)}
                                                </span>
                                            </div>
                                            <div className="flex flex-wrap items-center gap-2 text-[10px] text-theme-light">
                                                <span>Anchor:</span>
                                                <TechId value={tpl.anchorId} kind="hash" />
                                                <span>· {t('settings.probeAgeLabel', 'Age')}: {formatAgeLabel(tpl.createdAtSecs)}</span>
                                            </div>
                                        </div>
                                    ))}
                                </div>
                            )}
                        </div>
                    </AdvancedDisclosure>
                </div>
            </Card>
        </div>
    );
};

export default NetworkProtectionCard;
