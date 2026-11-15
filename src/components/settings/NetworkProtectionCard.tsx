// src/components/settings/NetworkProtectionCard.tsx
import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
    Network,
    Server,
    Wifi,
    WifiOff,
    ShieldCheck,
    Globe,
    Clock,
    Plus,
    Trash2,
    Gauge
} from 'lucide-react';
import { AppSettings, CustomServerDto, RegionalSourceDto } from '../../types';
import { settingsService } from '../../services/settingsService';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { HelpIcon } from '../ui/HelpIcon';
import { AdvancedDisclosure } from '../ui/AdvancedDisclosure';
import { translateError, stringifyError } from '../../utils/errorHelper';
import { logger } from '../../utils/log';

export interface NetworkProtectionCardProps {
    settings: AppSettings | null;
    onSettingsUpdated: (updater: AppSettings | ((prev: AppSettings | null) => AppSettings | null)) => void;
    isDevUnlocked: boolean;
    initialOpen?: boolean;
}

export const NetworkProtectionCard: React.FC<NetworkProtectionCardProps> = ({
    settings,
    onSettingsUpdated,
    isDevUnlocked,
    initialOpen = false,
}) => {
    const { t } = useTranslation();

    const [newServerName, setNewServerName] = useState('');
    const [newServerUrl, setNewServerUrl] = useState('');
    const [newServerAuthToken, setNewServerAuthToken] = useState('');
    const [newServerPeerToken, setNewServerPeerToken] = useState('');
    const [newRegionalUrl, setNewRegionalUrl] = useState('');
    const [newRegionalPublicKey, setNewRegionalPublicKey] = useState('');
    const [newRegionalBearerToken, setNewRegionalBearerToken] = useState('');

    const [isRefreshingMesh, setIsRefreshingMesh] = useState(false);
    const [meshRefreshFeedback, setMeshRefreshFeedback] = useState<string | null>(null);

    const [benchmarkResults, setBenchmarkResults] = useState<Array<{ url: string; latencyMs?: number; reachable: boolean }>>([]);
    const [isBenchmarking, setIsBenchmarking] = useState(false);
    const [benchmarkError, setBenchmarkError] = useState<string | null>(null);

    const hasActiveServers = Boolean(
        (settings?.customServers || []).some(s => s.isActive) ||
        (settings?.regionalMeshSources || []).length > 0 ||
        (settings?.activeNetwork === 'testnet' ? settings?.cachedPeersTestnet?.length : settings?.cachedPeersMainnet?.length)
    );

    const handleAddCustomServer = () => {
        const trimmedUrl = newServerUrl.trim();
        if (!trimmedUrl) return;
        const newServer: CustomServerDto = {
            url: trimmedUrl,
            name: newServerName.trim() || trimmedUrl,
            authToken: newServerAuthToken.trim() || undefined,
            peerToken: newServerPeerToken.trim() || undefined,
            isActive: true,
        };
        onSettingsUpdated(s => s ? { ...s, customServers: [...(s.customServers || []), newServer] } : s);
        setNewServerName('');
        setNewServerUrl('');
        setNewServerAuthToken('');
        setNewServerPeerToken('');
    };

    const handleRemoveCustomServer = (idx: number) => {
        onSettingsUpdated(s => s ? { ...s, customServers: (s.customServers || []).filter((_, i) => i !== idx) } : s);
    };

    const handleAddRegionalSource = () => {
        if (!newRegionalUrl.trim() || !newRegionalPublicKey.trim()) return;
        const newSource: RegionalSourceDto = {
            url: newRegionalUrl.trim(),
            publicKey: newRegionalPublicKey.trim(),
            bearerToken: newRegionalBearerToken.trim() || undefined,
        };
        onSettingsUpdated(s => s ? {
            ...s,
            regionalMeshSources: [...(s.regionalMeshSources || []), newSource]
        } : s);
        setNewRegionalUrl('');
        setNewRegionalPublicKey('');
        setNewRegionalBearerToken('');
    };

    const handleRemoveRegionalSource = (idx: number) => {
        onSettingsUpdated(s => s ? { ...s, regionalMeshSources: (s.regionalMeshSources || []).filter((_, i) => i !== idx) } : s);
    };

    const handleRefreshMeshViews = async () => {
        setIsRefreshingMesh(true);
        setMeshRefreshFeedback(null);
        try {
            const success = await settingsService.refreshRegionalMeshViews();
            setMeshRefreshFeedback(success ? t('settings.meshRefreshSuccess') : t('settings.meshRefreshNone'));
        } catch (e) {
            setMeshRefreshFeedback(translateError(e, t));
        } finally {
            setIsRefreshingMesh(false);
            setTimeout(() => setMeshRefreshFeedback(null), 4000);
        }
    };

    const handleBenchmark = async () => {
        setIsBenchmarking(true);
        setBenchmarkError(null);
        try {
            const urls = settings?.customServers?.map(s => s.url).filter(Boolean);
            const results = await settingsService.benchmarkL2Servers(urls && urls.length > 0 ? urls : undefined);
            setBenchmarkResults(results);
            if (settings?.customServers && settings.customServers.length > 0 && results.length > 0) {
                const orderMap = new Map(results.map((r, idx) => [r.url, idx] as const));
                const sorted = [...(settings.customServers || [])].sort((a, b) => {
                    const aIdx = orderMap.has(a.url) ? orderMap.get(a.url)! : 9999;
                    const bIdx = orderMap.has(b.url) ? orderMap.get(b.url)! : 9999;
                    if (aIdx !== bIdx) return aIdx - bIdx;
                    return a.url.localeCompare(b.url);
                });
                const isSameOrder = sorted.every((s, i) => s.url === (settings.customServers || [])[i]?.url);
                if (!isSameOrder) {
                    onSettingsUpdated(prev => prev ? { ...prev, customServers: sorted } : prev);
                }
            }
        } catch (e) {
            const msg = translateError(e, t);
            setBenchmarkError(msg);
            logger.error(`Benchmark failed: ${stringifyError(e)}`);
        } finally {
            setIsBenchmarking(false);
        }
    };

    const getLatencyColor = (latencyMs?: number, reachable?: boolean) => {
        if (!reachable || latencyMs === undefined || latencyMs === null) return 'bg-slate-300 dark:bg-slate-600';
        if (latencyMs < 100) return 'bg-emerald-500';
        if (latencyMs < 300) return 'bg-yellow-500';
        if (latencyMs < 600) return 'bg-orange-500';
        return 'bg-rose-500';
    };

    const getLatencyWidth = (latencyMs?: number, reachable?: boolean, maxLatency?: number) => {
        if (!reachable || latencyMs === undefined || latencyMs === null) return '10%';
        const max = maxLatency && maxLatency > 0 ? maxLatency : 1000;
        const pct = Math.min(100, Math.max(12, (latencyMs / max) * 100));
        return `${pct}%`;
    };

    return (
        <div id="settings-network-section">
            <Card header={
                <div className="flex items-center gap-2">
                    <Network size={18} className="text-theme-primary" />
                    <span className="font-black text-xs uppercase tracking-widest">{t('settings.l2ProtectionTitle')}</span>
                    <HelpIcon topic="l2Protection" size={14} />
                    {isDevUnlocked && (
                        <span className="ml-2 text-[9px] font-bold px-1.5 py-0.5 rounded bg-amber-100 text-amber-800 border border-amber-200">DEV</span>
                    )}
                </div>
            }>
                <div className="space-y-6">
                    {/* Network Status Overview Box */}
                    <div className={`p-4 rounded-2xl border flex items-start gap-3.5 ${
                        hasActiveServers 
                            ? 'bg-emerald-500/10 border-emerald-500/20 text-emerald-950 dark:text-emerald-200'
                            : 'bg-slate-500/10 border-slate-500/20 text-slate-900 dark:text-slate-200'
                    }`}>
                        {hasActiveServers ? (
                            <Wifi size={20} className="text-emerald-600 shrink-0 mt-0.5" />
                        ) : (
                            <WifiOff size={20} className="text-slate-600 shrink-0 mt-0.5" />
                        )}
                        <div className="space-y-1">
                            <div className="flex items-center gap-2">
                                <span className="text-xs font-black uppercase tracking-wider">
                                    {hasActiveServers
                                        ? t('settings.l2ModeConnectedTitle')
                                        : t('settings.l2ModePureOfflineTitle')}
                                </span>
                                <span className={`text-[10px] font-bold px-2 py-0.5 rounded-full ${
                                    hasActiveServers ? 'bg-emerald-100 text-emerald-800' : 'bg-slate-200 text-slate-700'
                                }`}>
                                    {hasActiveServers ? 'Online' : 'Offline'}
                                </span>
                            </div>
                            <p className="text-xs text-theme-secondary/80 leading-relaxed font-medium">
                                {hasActiveServers
                                    ? t('settings.l2ModeConnectedDesc')
                                    : t('settings.l2ModePureOfflineDesc')}
                            </p>
                        </div>
                    </div>

                    {/* Advanced Server & PoS Collapsible */}
                    <AdvancedDisclosure
                        icon={<Server size={14} className="text-theme-primary" />}
                        title={
                            <span className="text-xs font-bold text-theme-secondary">
                                {t('settings.advancedNetworkToggle')}
                            </span>
                        }
                        defaultOpen={initialOpen || Boolean(settings?.customServers?.length || settings?.regionalMeshSources?.length)}
                        isCustom={Boolean(settings?.enablePosMode || settings?.customServers?.length || settings?.regionalMeshSources?.length)}
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
                                        onChange={(e) => onSettingsUpdated(s => s ? { ...s, enablePosMode: e.target.checked } : null)}
                                        className="mt-0.5 accent-theme-primary"
                                    />
                                    <div className="flex-1">
                                        <div className="flex items-center gap-2">
                                            <label htmlFor="enablePosMode" className="text-xs font-medium text-theme-secondary leading-relaxed cursor-pointer">
                                                {t('settings.enablePosMode')}
                                            </label>
                                            <HelpIcon topic="posMode" size={14} />
                                        </div>
                                        <label htmlFor="enablePosMode" className="block text-[10px] text-theme-light/60 font-normal cursor-pointer">
                                            {t('settings.enablePosModeDesc')}
                                        </label>
                                    </div>
                                </div>

                                {/* Network Selection */}
                                <div className="space-y-3">
                                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('settings.activeNetworkLabel')}</label>
                                    <div className="flex gap-3">
                                        <label className={`flex items-center gap-2 px-4 py-2.5 rounded-xl border-2 cursor-pointer transition-all ${settings?.activeNetwork !== 'testnet' ? 'bg-theme-primary text-white border-theme-primary shadow-sm' : 'bg-white dark:bg-slate-900 border-theme-subtle hover:border-theme-primary/30 text-theme-secondary'}`}>
                                            <input
                                                type="radio"
                                                name="activeNetwork"
                                                value="mainnet"
                                                checked={settings?.activeNetwork !== 'testnet'}
                                                onChange={() => onSettingsUpdated(s => s ? { ...s, activeNetwork: 'mainnet' } : null)}
                                                className="sr-only"
                                            />
                                            <span className="text-sm">{settings?.activeNetwork !== 'testnet' ? '● Mainnet' : '○ Mainnet'}</span>
                                        </label>
                                        <label className={`flex items-center gap-2 px-4 py-2.5 rounded-xl border-2 cursor-pointer transition-all ${settings?.activeNetwork === 'testnet' ? 'bg-orange-500 text-white border-orange-500 shadow-sm' : 'bg-white dark:bg-slate-900 border-theme-subtle hover:border-orange-300 text-theme-secondary'}`}>
                                            <input
                                                type="radio"
                                                name="activeNetwork"
                                                value="testnet"
                                                checked={settings?.activeNetwork === 'testnet'}
                                                onChange={() => onSettingsUpdated(s => s ? { ...s, activeNetwork: 'testnet' } : null)}
                                                className="sr-only"
                                            />
                                            <span className="text-sm">{settings?.activeNetwork === 'testnet' ? '● Testnet' : '○ Testnet'}</span>
                                        </label>
                                    </div>
                                    <p className="text-[10px] text-theme-light/60 font-medium">{t('settings.activeNetworkDesc')}</p>
                                </div>

                                {/* Custom Servers */}
                                <div className="space-y-3">
                                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-2">
                                        <Server size={12} /> {t('settings.customServersTitle')}
                                        <HelpIcon topic="gatewayTokens" size={14} />
                                    </label>
                                    {(settings?.customServers || []).length > 0 ? (
                                        <div className="space-y-2">
                                            {(settings?.customServers || []).map((srv, idx) => (
                                                <div key={`${srv.url}-${idx}`} className="flex items-center gap-2 p-3 bg-slate-50 dark:bg-slate-800 border border-theme-subtle rounded-xl">
                                                    <div className="flex-1 min-w-0">
                                                        <div className="text-xs font-bold text-theme-secondary truncate">{srv.name || srv.url}</div>
                                                        <div className="text-[10px] font-mono text-theme-light truncate">{srv.url}</div>
                                                    </div>
                                                    <Button
                                                        type="button"
                                                        variant="outline"
                                                        onClick={() => handleRemoveCustomServer(idx)}
                                                        className="px-2 py-1 text-xs shrink-0"
                                                        title={t('common.remove')}
                                                    >
                                                        <Trash2 size={14} />
                                                    </Button>
                                                </div>
                                            ))}
                                        </div>
                                    ) : (
                                        <p className="text-xs text-theme-light/60 italic">{t('settings.noCustomServers')}</p>
                                    )}

                                    {/* Add new server */}
                                    <div className="grid grid-cols-1 md:grid-cols-2 gap-3 p-3 bg-white dark:bg-slate-900 border border-dashed border-theme-subtle rounded-xl">
                                        <Input
                                            placeholder={t('settings.serverNamePlaceholder')}
                                            value={newServerName}
                                            onChange={(e) => setNewServerName(e.target.value)}
                                            className="text-xs"
                                        />
                                        <Input
                                            placeholder={t('settings.serverUrlPlaceholder')}
                                            value={newServerUrl}
                                            onChange={(e) => setNewServerUrl(e.target.value)}
                                            className="text-xs font-mono"
                                        />
                                        <Input
                                            placeholder={t('settings.authTokenPlaceholder')}
                                            value={newServerAuthToken}
                                            onChange={(e) => setNewServerAuthToken(e.target.value)}
                                            className="text-xs font-mono"
                                        />
                                        <Input
                                            placeholder={t('settings.peerTokenPlaceholder')}
                                            value={newServerPeerToken}
                                            onChange={(e) => setNewServerPeerToken(e.target.value)}
                                            className="text-xs font-mono"
                                        />
                                        <div className="md:col-span-2">
                                            <Button
                                                type="button"
                                                variant="outline"
                                                onClick={handleAddCustomServer}
                                                disabled={!newServerUrl.trim()}
                                                className="w-full gap-2"
                                            >
                                                <Plus size={14} /> {t('settings.addServerButton')}
                                            </Button>
                                        </div>
                                    </div>
                                </div>

                                {/* Exclusive Checkbox */}
                                <div className="flex items-start gap-3 p-3 bg-amber-50/50 dark:bg-amber-950/20 border border-amber-200 dark:border-amber-800 rounded-xl hover:bg-amber-50 transition-colors">
                                    <input
                                        type="checkbox"
                                        id="exclusiveCustomServers"
                                        checked={!!settings?.exclusiveCustomServers}
                                        onChange={(e) => onSettingsUpdated(s => s ? { ...s, exclusiveCustomServers: e.target.checked } : null)}
                                        className="mt-0.5 accent-theme-primary"
                                    />
                                    <div className="flex-1">
                                        <div className="flex items-center gap-2">
                                            <label htmlFor="exclusiveCustomServers" className="text-xs font-medium text-theme-secondary leading-relaxed cursor-pointer">
                                                {t('settings.exclusiveServersLabel')}
                                            </label>
                                            <HelpIcon topic="exclusiveGateways" size={14} />
                                        </div>
                                        <label htmlFor="exclusiveCustomServers" className="block text-[10px] text-theme-light/60 font-normal cursor-pointer">
                                            {t('settings.exclusiveServersDesc')}
                                        </label>
                                    </div>
                                </div>

                                {/* Regional Mesh (Dev-only) */}
                                {isDevUnlocked && (
                                    <div className="space-y-3 pt-4 border-t border-theme-subtle">
                                        <div className="flex flex-wrap items-center justify-between gap-2">
                                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5">
                                                <ShieldCheck size={12} /> {t('settings.regionalMeshTitle')}
                                            </label>
                                            <Button
                                                type="button"
                                                variant="outline"
                                                onClick={handleRefreshMeshViews}
                                                disabled={isRefreshingMesh}
                                                className="text-[10px] px-2.5 py-1 gap-1"
                                            >
                                                {isRefreshingMesh ? <Clock className="animate-spin" size={12} /> : <Globe size={12} />}
                                                {isRefreshingMesh ? t('common.processing') : t('settings.coldRefreshButton')}
                                            </Button>
                                        </div>
                                        {meshRefreshFeedback && (
                                            <p className={`text-xs font-medium ${meshRefreshFeedback.includes('Fehler') || meshRefreshFeedback.includes('error') ? 'text-rose-600' : 'text-emerald-600'}`}>
                                                {meshRefreshFeedback}
                                            </p>
                                        )}
                                        {(settings?.regionalMeshSources || []).length > 0 ? (
                                            <div className="space-y-2">
                                                {(settings?.regionalMeshSources || []).map((src, idx) => (
                                                    <div key={`${src.url}-${idx}`} className="flex items-center gap-2 p-3 bg-slate-50 dark:bg-slate-800 border border-theme-subtle rounded-xl">
                                                        <div className="flex-1 min-w-0">
                                                            <div className="text-xs font-bold text-theme-secondary truncate">{src.url}</div>
                                                            <div className="text-[10px] font-mono text-theme-light truncate">Key: {src.publicKey}</div>
                                                        </div>
                                                        <Button
                                                            type="button"
                                                            variant="outline"
                                                            onClick={() => handleRemoveRegionalSource(idx)}
                                                            className="px-2 py-1 text-xs shrink-0"
                                                            title={t('common.remove')}
                                                        >
                                                            <Trash2 size={14} />
                                                        </Button>
                                                    </div>
                                                ))}
                                            </div>
                                        ) : (
                                            <p className="text-xs text-theme-light/60 italic">{t('settings.noRegionalMesh')}</p>
                                        )}

                                        {/* Add regional source */}
                                        <div className="grid grid-cols-1 md:grid-cols-2 gap-3 p-3 bg-white dark:bg-slate-900 border border-dashed border-theme-subtle rounded-xl">
                                            <Input
                                                placeholder={t('settings.serverUrlPlaceholder')}
                                                value={newRegionalUrl}
                                                onChange={(e) => setNewRegionalUrl(e.target.value)}
                                                className="text-xs font-mono"
                                            />
                                            <Input
                                                placeholder="Public Key (Ed25519 Hex / Base58)"
                                                value={newRegionalPublicKey}
                                                onChange={(e) => setNewRegionalPublicKey(e.target.value)}
                                                className="text-xs font-mono"
                                            />
                                            <div className="md:col-span-2">
                                                <Button
                                                    type="button"
                                                    variant="outline"
                                                    onClick={handleAddRegionalSource}
                                                    disabled={!newRegionalUrl.trim() || !newRegionalPublicKey.trim()}
                                                    className="w-full gap-2"
                                                >
                                                    <Plus size={14} /> {t('settings.addRegionalSourceButton')}
                                                </Button>
                                            </div>
                                        </div>
                                    </div>
                                )}

                                {/* Benchmark */}
                                <div className="space-y-3">
                                    <Button
                                        type="button"
                                        variant="outline"
                                        onClick={handleBenchmark}
                                        disabled={isBenchmarking}
                                        className="w-full gap-2"
                                    >
                                        {isBenchmarking ? <Clock className="animate-spin" size={16} /> : <Gauge size={16} />}
                                        {isBenchmarking ? t('settings.testing') : t('settings.benchmarkButton')}
                                    </Button>
                                    {benchmarkError && (
                                        <p className="text-xs text-rose-600 font-medium">{benchmarkError}</p>
                                    )}
                                    {benchmarkResults.length > 0 && (
                                        <div className="space-y-2">
                                            {(() => {
                                                const maxLatency = Math.max(...benchmarkResults.filter(r => r.reachable && r.latencyMs !== undefined).map(r => r.latencyMs || 0), 0);
                                                return benchmarkResults.map((res) => (
                                                    <div key={res.url} className="p-2.5 bg-white dark:bg-slate-900 border border-theme-subtle rounded-xl space-y-1.5">
                                                        <div className="flex items-center justify-between gap-2">
                                                            <span className="text-[11px] font-mono text-theme-secondary truncate flex-1" title={res.url}>{res.url}</span>
                                                            <span className={`text-[11px] font-bold shrink-0 ${!res.reachable ? 'text-slate-400' : (res.latencyMs !== undefined && res.latencyMs < 300 ? 'text-emerald-600' : res.latencyMs !== undefined && res.latencyMs < 600 ? 'text-orange-600' : 'text-rose-600')}`}>
                                                                {res.reachable && res.latencyMs !== undefined ? `${res.latencyMs} ms` : t('settings.unreachable')}
                                                            </span>
                                                        </div>
                                                        <div className="h-2 bg-slate-100 dark:bg-slate-800 rounded-full overflow-hidden">
                                                            <div
                                                                className={`h-full rounded-full transition-all ${getLatencyColor(res.latencyMs, res.reachable)}`}
                                                                style={{ width: getLatencyWidth(res.latencyMs, res.reachable, maxLatency) }}
                                                            />
                                                        </div>
                                                    </div>
                                                ));
                                            })()}
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
