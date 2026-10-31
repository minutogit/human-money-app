// src/components/SettingsView.tsx
import { useState, useEffect, FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { settingsService } from '../services/settingsService';
import { logger } from '../utils/log';
import { Button } from './ui/Button';
import { Card } from './ui/Card';
import { Input } from './ui/Input';
import { AppSettings, PrivacyDefault, CustomServerDto, RegionalSourceDto } from '../types';
import { useSession } from '../context/SessionContext';
import { useOptionalNavigation } from '../context/NavigationContext';
import { translateError, stringifyError } from '../utils/errorHelper';

import { ProfileSettings } from './ProfileSettings';
import { BackupSettingsCard } from './BackupSettingsCard';
import { StandardsManager } from './StandardsManager';
import { PageLayout } from './ui/PageLayout';
import { authService } from '../services/authService';
import { LanguageSelector } from './ui/LanguageSelector';
import { 
    User, 
    Settings as SettingsIcon, 
    Shield, 
    ShieldCheck,
    Eye, 
    EyeOff, 
    Clock, 
    Database,
    Save,
    CheckCircle2,
    Fingerprint,
    Globe,
    Server,
    Network,
    Gauge,
    Trash2,
    Plus,
    ChevronDown,
    ChevronUp,
    Wifi,
    WifiOff
} from 'lucide-react';

interface SettingsViewProps {
    onBack: () => void;
}

export function SettingsView({ onBack }: SettingsViewProps) {
    const { t } = useTranslation();
    const { protectAction } = useSession();
    const [settings, setSettings] = useState<AppSettings | null>(null);
    const [localInstanceId, setLocalInstanceId] = useState('');
    const [isLoading, setIsLoading] = useState(true);
    const [isSaving, setIsSaving] = useState(false);
    const [error, setError] = useState('');
    const [success, setSuccess] = useState('');
    const [activeTab, setActiveTab] = useState<'app' | 'profile'>('profile');
    const [isStandardsOpen, setIsStandardsOpen] = useState(false);

    const nav = useOptionalNavigation();
    const appVersion = nav?.appVersion || '0.3.0';

    // Dev mode hidden trigger (7 clicks)
    const [devClickCount, setDevClickCount] = useState(0);
    const [isAdvancedNetworkOpen, setIsAdvancedNetworkOpen] = useState(false);
    const [benchmarkResults, setBenchmarkResults] = useState<Array<{ url: string; latencyMs?: number; reachable: boolean }>>([]);
    const [isBenchmarking, setIsBenchmarking] = useState(false);
    const [benchmarkError, setBenchmarkError] = useState<string | null>(null);
    const [newServerName, setNewServerName] = useState('');
    const [newServerUrl, setNewServerUrl] = useState('');
    const [newServerAuthToken, setNewServerAuthToken] = useState('');
    const [newServerPeerToken, setNewServerPeerToken] = useState('');
    const [newRegionalUrl, setNewRegionalUrl] = useState('');
    const [newRegionalPublicKey, setNewRegionalPublicKey] = useState('');
    const [newRegionalBearerToken, setNewRegionalBearerToken] = useState('');
    const [isRefreshingMesh, setIsRefreshingMesh] = useState(false);
    const [meshRefreshFeedback, setMeshRefreshFeedback] = useState<string | null>(null);

    useEffect(() => {
        async function fetchSettings() {
            try {
                logger.info("SettingsView: Fetching current settings.");
                const currentSettings = await settingsService.getSettings();
                setSettings(currentSettings);
                if (currentSettings.customServers?.length || currentSettings.regionalMeshSources?.length || currentSettings.devModeUnlocked) {
                    setIsAdvancedNetworkOpen(true);
                }
                
                const deviceId = await authService.getLocalInstanceId();
                setLocalInstanceId(deviceId);
            } catch (e) {
                const msg = t('settings.loadFailed', { error: translateError(e, t) });
                logger.error(`Failed to load settings: ${stringifyError(e)}`);
                setError(msg);
            } finally {
                setIsLoading(false);
            }
        }
        fetchSettings();
    }, [t]);

    const handleDevModeClick = () => {
        if (settings?.devModeUnlocked) return;
        const next = devClickCount + 1;
        setDevClickCount(next);
        if (next >= 7) {
            setSettings(s => s ? { ...s, devModeUnlocked: true } : s);
            logger.info("SettingsView: Dev mode unlocked via 7 clicks.");
            setSuccess(t('settings.devModeUnlockedToast', 'Developer mode unlocked'));
            setTimeout(() => setSuccess(''), 3000);
        }
    };

    const handleSave = async (event: FormEvent) => {
        event.preventDefault();
        if (!settings) {
            setError(t('settings.fillAllFields'));
            return;
        }

        setIsSaving(true);
        setError('');
        setSuccess('');

        try {
            logger.info("Attempting to save settings...");
            await protectAction(async (password) => { await settingsService.saveSettings(settings, password || undefined); });
            setSuccess(t('settings.saveSuccess'));
            logger.info("Settings saved successfully.");
            setTimeout(() => setSuccess(''), 3000);
        } catch (e) {
            const msg = t('settings.saveFailed', { error: translateError(e, t) });
            logger.error(`Failed to save settings: ${stringifyError(e)}`);
            setError(msg);
        } finally {
            setIsSaving(false);
        }
    };

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
        setSettings(s => s ? { ...s, customServers: [...(s.customServers || []), newServer] } : s);
        setNewServerName('');
        setNewServerUrl('');
        setNewServerAuthToken('');
        setNewServerPeerToken('');
    };

    const handleRemoveCustomServer = (idx: number) => {
        setSettings(s => s ? { ...s, customServers: (s.customServers || []).filter((_, i) => i !== idx) } : s);
    };

    const handleAddRegionalSource = () => {
        if (!newRegionalUrl.trim() || !newRegionalPublicKey.trim()) return;
        const newSource: RegionalSourceDto = {
            url: newRegionalUrl.trim(),
            publicKey: newRegionalPublicKey.trim(),
            bearerToken: newRegionalBearerToken.trim() || undefined,
        };
        setSettings(s => s ? {
            ...s,
            regionalMeshSources: [...(s.regionalMeshSources || []), newSource]
        } : s);
        setNewRegionalUrl('');
        setNewRegionalPublicKey('');
        setNewRegionalBearerToken('');
    };

    const handleRemoveRegionalSource = (idx: number) => {
        setSettings(s => s ? { ...s, regionalMeshSources: (s.regionalMeshSources || []).filter((_, i) => i !== idx) } : s);
    };

    const handleRefreshMeshViews = async () => {
        setIsRefreshingMesh(true);
        setMeshRefreshFeedback(null);
        try {
            const success = await settingsService.refreshRegionalMeshViews();
            setMeshRefreshFeedback(success ? t('settings.meshRefreshSuccess', 'Regional mesh views refreshed successfully.') : t('settings.meshRefreshNone', 'No regional mesh sources configured.'));
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
            // Optionally sort customServers by benchmark order (reachable first, sorted by latency)
            if (settings?.customServers && settings.customServers.length > 0 && results.length > 0) {
                const orderMap = new Map(results.map((r, idx) => [r.url, idx] as const));
                const sorted = [...(settings.customServers || [])].sort((a, b) => {
                    const aIdx = orderMap.has(a.url) ? orderMap.get(a.url)! : 9999;
                    const bIdx = orderMap.has(b.url) ? orderMap.get(b.url)! : 9999;
                    if (aIdx !== bIdx) return aIdx - bIdx;
                    return a.url.localeCompare(b.url);
                });
                // Only update if order changed
                const isSameOrder = sorted.every((s, i) => s.url === (settings.customServers || [])[i]?.url);
                if (!isSameOrder) {
                    setSettings(prev => prev ? { ...prev, customServers: sorted } : prev);
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

    if (isLoading) {
        return <div className="py-20 text-center animate-pulse text-theme-light font-black uppercase tracking-[0.2em]">{t('settings.configuringEnvironment')}</div>;
    }

    const isDevUnlocked = !!settings?.devModeUnlocked;
    const hasActiveServers = Boolean(
        (settings?.customServers || []).some(s => s.isActive) ||
        (settings?.regionalMeshSources || []).length > 0 ||
        (settings?.activeNetwork === 'testnet' ? settings?.cachedPeersTestnet?.length : settings?.cachedPeersMainnet?.length)
    );

    return (
        <PageLayout 
            title={
                <span 
                    onClick={handleDevModeClick} 
                    className="select-none cursor-pointer"
                    title={isDevUnlocked ? t('settings.devModeUnlockedToast', 'Developer mode unlocked') : `Klick ${7 - devClickCount} bis Entwicklermodus`}
                    role="button"
                    tabIndex={0}
                    onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); handleDevModeClick(); }}}
                >
                    {t('settings.systemPreferencesTitle')}
                </span>
            } 
            description={t('settings.systemPreferencesDescription')}
            onBack={onBack}
        >
            <div className="max-w-4xl mx-auto">
                {/* Tab Navigation */}
                <div className="flex bg-white/30 backdrop-blur-md p-1.5 rounded-2xl border border-theme-subtle mb-10 shadow-inner-soft">
                    <button 
                        onClick={() => setActiveTab('profile')}
                        className={`flex-1 flex items-center justify-center gap-2 py-3 rounded-xl text-xs font-black uppercase tracking-widest transition-all ${activeTab === 'profile' ? 'bg-white text-theme-primary shadow-premium' : 'text-theme-light hover:text-theme-secondary hover:bg-white/40'}`}
                    >
                        <User size={14} />
                        {t('settings.tabIdentityProfile')}
                    </button>
                    <button 
                        onClick={() => setActiveTab('app')}
                        className={`flex-1 flex items-center justify-center gap-2 py-3 rounded-xl text-xs font-black uppercase tracking-widest transition-all ${activeTab === 'app' ? 'bg-white text-theme-primary shadow-premium' : 'text-theme-light hover:text-theme-secondary hover:bg-white/40'}`}
                    >
                        <SettingsIcon size={14} />
                        {t('settings.tabSecurityOps')}
                    </button>
                </div>

                {activeTab === 'profile' ? (
                    <ProfileSettings />
                ) : (
                    <form onSubmit={handleSave} className="space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500">
                        <div className="grid grid-cols-1 md:grid-cols-2 gap-8">
                            {/* Storage & Data Retention */}
                            <Card header={
                                <div className="flex items-center gap-2">
                                    <Database size={18} className="text-theme-primary" />
                                    <span className="font-black text-xs uppercase tracking-widest">{t('settings.dataStewardshipTitle')}</span>
                                </div>
                            }>
                                <div className="space-y-6">
                                    <div className="space-y-2">
                                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('settings.historyStorageLabel')}</label>
                                        <Input
                                            type="number"
                                            value={settings?.bundleRetentionDays ?? 30}
                                            onChange={(e) => setSettings(s => s ? { ...s, bundleRetentionDays: parseInt(e.target.value, 10) } : null)}
                                            min="1"
                                            className="font-bold"
                                        />
                                        <p className="text-[10px] text-theme-light/60 font-medium leading-relaxed">
                                            {t('settings.historyStorageDescription')}
                                        </p>
                                    </div>
                                </div>
                            </Card>

                            {/* Session Control */}
                            <Card header={
                                <div className="flex items-center gap-2">
                                    <Clock size={18} className="text-theme-primary" />
                                    <span className="font-black text-xs uppercase tracking-widest">{t('settings.activeSessionTitle')}</span>
                                </div>
                            }>
                                <div className="space-y-6">
                                    <div className="space-y-2">
                                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('settings.timeoutIntervalLabel')}</label>
                                        <Input
                                            type="number"
                                            value={settings ? Math.floor((settings.sessionTimeoutSeconds || 0) / 60) : 10}
                                            onChange={(e) => {
                                                const minutes = parseInt(e.target.value, 10);
                                                setSettings(s => s ? { ...s, sessionTimeoutSeconds: isNaN(minutes) ? 0 : minutes * 60 } : null);
                                            }}
                                            min="0"
                                            className="font-bold"
                                        />
                                        <p className="text-[10px] text-theme-light/60 font-medium leading-relaxed">
                                            {t('settings.timeoutIntervalDescription')}
                                        </p>
                                    </div>
                                </div>
                            </Card>
                        </div>

                        {/* Privacy Protocols */}
                        <Card header={
                            <div className="flex items-center gap-2">
                                <Shield size={18} className="text-theme-primary" />
                                <span className="font-black text-xs uppercase tracking-widest">{t('settings.privacyProtocolsTitle')}</span>
                            </div>
                        }>
                            <div className="space-y-4">
                                <p className="text-xs text-theme-light font-medium mb-4">{t('settings.privacyProtocolsDescription')}</p>
                                <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                                    {[
                                        { id: 'ask', label: t('settings.privacyModeInteractive'), icon: Fingerprint, desc: t('settings.privacyModeInteractiveDesc') },
                                        { id: 'stealth', label: t('settings.privacyModeStealth'), icon: EyeOff, desc: t('settings.privacyModeStealthDesc') },
                                        { id: 'public', label: t('settings.privacyModePublic'), icon: Eye, desc: t('settings.privacyModePublicDesc') }
                                    ].map((mode) => (
                                        <label 
                                            key={mode.id}
                                            className={`relative flex flex-col p-4 rounded-2xl border-2 transition-all cursor-pointer group ${settings?.privacyDefault === mode.id ? 'bg-theme-primary/5 border-theme-primary shadow-sm' : 'bg-white border-theme-subtle hover:border-theme-primary/30'}`}
                                        >
                                            <input
                                                type="radio"
                                                name="privacyDefault"
                                                value={mode.id}
                                                checked={settings?.privacyDefault === mode.id}
                                                onChange={(e) => setSettings(s => s ? { ...s, privacyDefault: e.target.value as PrivacyDefault } : null)}
                                                className="absolute opacity-0"
                                            />
                                            <mode.icon className={`mb-3 transition-colors ${settings?.privacyDefault === mode.id ? 'text-theme-primary' : 'text-theme-light group-hover:text-theme-secondary'}`} size={20} />
                                            <span className={`text-sm font-black tracking-tight ${settings?.privacyDefault === mode.id ? 'text-theme-primary' : 'text-theme-secondary'}`}>{mode.label}</span>
                                            <span className="text-[10px] font-medium text-theme-light mt-1">{mode.desc}</span>
                                            {settings?.privacyDefault === mode.id && (
                                                <div className="absolute top-3 right-3 text-theme-primary">
                                                    <CheckCircle2 size={16} />
                                                </div>
                                            )}
                                        </label>
                                    ))}
                                </div>
                            </div>
                        </Card>

                        {/* Encrypted Wallet Backups */}
                        <BackupSettingsCard settings={settings} onSettingsUpdated={setSettings} />

                        {/* Network & L2 Protection */}
                        <Card header={
                            <div className="flex items-center gap-2">
                                <Network size={18} className="text-theme-primary" />
                                <span className="font-black text-xs uppercase tracking-widest">{t('settings.l2ProtectionTitle', 'Network & L2 Protection')}</span>
                                {isDevUnlocked && (
                                    <span className="ml-2 text-[9px] font-bold px-1.5 py-0.5 rounded bg-amber-100 text-amber-800 border border-amber-200">DEV</span>
                                )}
                            </div>
                        }>
                            <div className="space-y-6">
                                {/* Network Status Overview Box */}
                                <div className={`p-4 rounded-2xl border flex items-start gap-3.5 ${
                                    hasActiveServers 
                                        ? 'bg-emerald-500/10 border-emerald-500/20 text-emerald-950'
                                        : 'bg-slate-500/10 border-slate-500/20 text-slate-900'
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
                                                    ? t('settings.l2ModeConnectedTitle', 'Layer 2 Gateway Network')
                                                    : t('settings.l2ModePureOfflineTitle', 'Pure Offline P2P Mode')}
                                            </span>
                                            <span className={`text-[10px] font-bold px-2 py-0.5 rounded-full ${
                                                hasActiveServers ? 'bg-emerald-100 text-emerald-800' : 'bg-slate-200 text-slate-700'
                                            }`}>
                                                {hasActiveServers ? 'Online' : 'Offline'}
                                            </span>
                                        </div>
                                        <p className="text-xs text-theme-secondary/80 leading-relaxed font-medium">
                                            {hasActiveServers
                                                ? t('settings.l2ModeConnectedDesc', 'Active servers provide blind collision lock registry queries, fast PoS checkouts, and WalletSeal backup anchors.')
                                                : t('settings.l2ModePureOfflineDesc', 'No public Human Money Node servers are active yet. Your wallet works 100% offline; transaction locks and WalletSeal state protection are ready and activate automatically as soon as servers become available.')}
                                        </p>
                                    </div>
                                </div>

                                {/* Advanced Server & PoS Collapsible */}
                                <div className="border border-theme-subtle rounded-2xl overflow-hidden">
                                    <button
                                        type="button"
                                        onClick={() => setIsAdvancedNetworkOpen(v => !v)}
                                        className="w-full flex items-center justify-between p-4 bg-slate-50/70 hover:bg-slate-100/70 transition-colors text-left"
                                    >
                                        <div className="flex items-center gap-2">
                                            <Server size={14} className="text-theme-primary" />
                                            <span className="text-xs font-bold text-theme-secondary">
                                                {t('settings.advancedNetworkToggle', 'Advanced Server & PoS Configuration')}
                                            </span>
                                        </div>
                                        {isAdvancedNetworkOpen ? <ChevronUp size={16} className="text-theme-light" /> : <ChevronDown size={16} className="text-theme-light" />}
                                    </button>

                                    {isAdvancedNetworkOpen && (
                                        <div className="p-4 space-y-6 border-t border-theme-subtle">
                                            {/* PoS Mode Toggle */}
                                            <label className="flex items-start gap-3 p-3 bg-white border border-theme-subtle rounded-xl cursor-pointer hover:border-theme-primary/40 transition-colors">
                                                <input
                                                    type="checkbox"
                                                    checked={!!settings?.enablePosMode}
                                                    onChange={(e) => setSettings(s => s ? { ...s, enablePosMode: e.target.checked } : null)}
                                                    className="mt-0.5 accent-theme-primary"
                                                />
                                                <span className="text-xs font-medium text-theme-secondary leading-relaxed">
                                                    {t('settings.enablePosMode')}
                                                    <span className="block text-[10px] text-theme-light/60 font-normal">
                                                        {t('settings.enablePosModeDesc')}
                                                    </span>
                                                </span>
                                            </label>

                                            {/* Network Selection */}
                                            <div className="space-y-3">
                                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('settings.activeNetworkLabel', 'Active Network')}</label>
                                                <div className="flex gap-3">
                                                    <label className={`flex items-center gap-2 px-4 py-2.5 rounded-xl border-2 cursor-pointer transition-all ${settings?.activeNetwork !== 'testnet' ? 'bg-theme-primary text-white border-theme-primary shadow-sm' : 'bg-white border-theme-subtle hover:border-theme-primary/30 text-theme-secondary'}`}>
                                                        <input
                                                            type="radio"
                                                            name="activeNetwork"
                                                            value="mainnet"
                                                            checked={settings?.activeNetwork !== 'testnet'}
                                                            onChange={() => setSettings(s => s ? { ...s, activeNetwork: 'mainnet' } : null)}
                                                            className="sr-only"
                                                        />
                                                        <span className="text-sm">{settings?.activeNetwork !== 'testnet' ? '● Mainnet' : '○ Mainnet'}</span>
                                                    </label>
                                                    <label className={`flex items-center gap-2 px-4 py-2.5 rounded-xl border-2 cursor-pointer transition-all ${settings?.activeNetwork === 'testnet' ? 'bg-orange-500 text-white border-orange-500 shadow-sm' : 'bg-white border-theme-subtle hover:border-orange-300 text-theme-secondary'}`}>
                                                        <input
                                                            type="radio"
                                                            name="activeNetwork"
                                                            value="testnet"
                                                            checked={settings?.activeNetwork === 'testnet'}
                                                            onChange={() => setSettings(s => s ? { ...s, activeNetwork: 'testnet' } : null)}
                                                            className="sr-only"
                                                        />
                                                        <span className="text-sm">{settings?.activeNetwork === 'testnet' ? '● Testnet' : '○ Testnet'}</span>
                                                    </label>
                                                </div>
                                                <p className="text-[10px] text-theme-light/60 font-medium">{t('settings.activeNetworkDesc', 'Select between production Mainnet and Testnet for development tests.')}</p>
                                            </div>

                                            {/* Custom Servers */}
                                            <div className="space-y-3">
                                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-2">
                                                    <Server size={12} /> {t('settings.customServersTitle', 'Custom Servers & Trusted Peers')}
                                                </label>
                                                {(settings?.customServers || []).length > 0 ? (
                                                    <div className="space-y-2">
                                                        {(settings?.customServers || []).map((srv, idx) => (
                                                            <div key={`${srv.url}-${idx}`} className="flex items-center gap-2 p-3 bg-slate-50 border border-theme-subtle rounded-xl">
                                                                <div className="flex-1 min-w-0">
                                                                    <div className="text-xs font-bold text-theme-secondary truncate">{srv.name || srv.url}</div>
                                                                    <div className="text-[10px] font-mono text-theme-light truncate">{srv.url}</div>
                                                                    {(srv.authToken || srv.peerToken) && (
                                                                        <div className="text-[10px] text-theme-light/60 truncate">Tokens: {srv.authToken ? 'auth ' : ''}{srv.peerToken ? 'peer' : ''}</div>
                                                                    )}
                                                                </div>
                                                                <Button
                                                                    type="button"
                                                                    variant="outline"
                                                                    onClick={() => handleRemoveCustomServer(idx)}
                                                                    className="px-2 py-1 text-xs shrink-0"
                                                                    title={t('common.remove', 'Remove')}
                                                                >
                                                                    <Trash2 size={14} />
                                                                </Button>
                                                            </div>
                                                        ))}
                                                    </div>
                                                ) : (
                                                    <p className="text-xs text-theme-light/60 italic">{t('settings.noCustomServers', 'No custom servers registered yet.')}</p>
                                                )}

                                                {/* Add new server */}
                                                <div className="grid grid-cols-1 md:grid-cols-2 gap-3 p-3 bg-white border border-dashed border-theme-subtle rounded-xl">
                                                    <Input
                                                        placeholder={t('settings.serverNamePlaceholder', 'Name (e.g. Peer A)')}
                                                        value={newServerName}
                                                        onChange={(e) => setNewServerName(e.target.value)}
                                                        className="text-xs"
                                                    />
                                                    <Input
                                                        placeholder={t('settings.serverUrlPlaceholder', 'URL (https://...)')}
                                                        value={newServerUrl}
                                                        onChange={(e) => setNewServerUrl(e.target.value)}
                                                        className="text-xs font-mono"
                                                    />
                                                    <Input
                                                        placeholder={t('settings.authTokenPlaceholder', 'Auth Token (optional)')}
                                                        value={newServerAuthToken}
                                                        onChange={(e) => setNewServerAuthToken(e.target.value)}
                                                        className="text-xs font-mono"
                                                    />
                                                    <Input
                                                        placeholder={t('settings.peerTokenPlaceholder', 'Peer Token (optional)')}
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
                                                            <Plus size={14} /> {t('settings.addServerButton', 'Add Server')}
                                                        </Button>
                                                    </div>
                                                </div>
                                            </div>

                                            {/* Exclusive Checkbox */}
                                            <label className="flex items-start gap-3 p-3 bg-amber-50/50 border border-amber-200 rounded-xl cursor-pointer hover:bg-amber-50 transition-colors">
                                                <input
                                                    type="checkbox"
                                                    checked={!!settings?.exclusiveCustomServers}
                                                    onChange={(e) => setSettings(s => s ? { ...s, exclusiveCustomServers: e.target.checked } : null)}
                                                    className="mt-0.5 accent-theme-primary"
                                                />
                                                <span className="text-xs font-medium text-theme-secondary leading-relaxed">
                                                    {t('settings.exclusiveServersLabel', 'Exclusively use custom servers (Strictly private)')}
                                                    <span className="block text-[10px] text-theme-light/60 font-normal">{t('settings.exclusiveServersDesc', 'When active, only the custom servers above are used — no public seed peers.')}</span>
                                                </span>
                                            </label>

                                            {/* Regional Observation Sources (Regional Mesh) */}
                                            <div className="space-y-3 pt-4 border-t border-theme-subtle">
                                                <div className="flex items-center justify-between">
                                                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5">
                                                        <ShieldCheck size={12} /> {t('settings.regionalMeshTitle', 'Regional Observation Sources (Regional Mesh)')}
                                                    </label>
                                                    <Button
                                                        type="button"
                                                        variant="outline"
                                                        onClick={handleRefreshMeshViews}
                                                        disabled={isRefreshingMesh}
                                                        className="text-[10px] px-2.5 py-1 gap-1"
                                                    >
                                                        {isRefreshingMesh ? <Clock className="animate-spin" size={12} /> : <Globe size={12} />}
                                                        {isRefreshingMesh ? t('common.processing', 'Processing...') : t('settings.coldRefreshButton', 'Cold Refresh')}
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
                                                            <div key={`${src.url}-${idx}`} className="flex items-center gap-2 p-3 bg-slate-50 border border-theme-subtle rounded-xl">
                                                                <div className="flex-1 min-w-0">
                                                                    <div className="text-xs font-bold text-theme-secondary truncate">{src.url}</div>
                                                                    <div className="text-[10px] font-mono text-theme-light truncate">Key: {src.publicKey}</div>
                                                                    {src.bearerToken && (
                                                                        <div className="text-[10px] text-theme-light/60 truncate">Bearer Token configured</div>
                                                                    )}
                                                                </div>
                                                                <Button
                                                                    type="button"
                                                                    variant="outline"
                                                                    onClick={() => handleRemoveRegionalSource(idx)}
                                                                    className="px-2 py-1 text-xs shrink-0"
                                                                    title={t('common.remove', 'Remove')}
                                                                >
                                                                    <Trash2 size={14} />
                                                                </Button>
                                                            </div>
                                                        ))}
                                                    </div>
                                                ) : (
                                                    <p className="text-xs text-theme-light/60 italic">{t('settings.noRegionalMesh', 'No regional observation sources configured yet.')}</p>
                                                )}

                                                {/* Add new regional source */}
                                                <div className="grid grid-cols-1 md:grid-cols-2 gap-3 p-3 bg-white border border-dashed border-theme-subtle rounded-xl">
                                                    <Input
                                                        placeholder={t('settings.serverUrlPlaceholder', 'URL (https://...)')}
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
                                                        <Input
                                                            placeholder={t('settings.authTokenPlaceholder', 'Bearer Token (optional)')}
                                                            value={newRegionalBearerToken}
                                                            onChange={(e) => setNewRegionalBearerToken(e.target.value)}
                                                            className="text-xs font-mono"
                                                        />
                                                    </div>
                                                    <div className="md:col-span-2">
                                                        <Button
                                                            type="button"
                                                            variant="outline"
                                                            onClick={handleAddRegionalSource}
                                                            disabled={!newRegionalUrl.trim() || !newRegionalPublicKey.trim()}
                                                            className="w-full gap-2"
                                                        >
                                                            <Plus size={14} /> {t('settings.addRegionalSourceButton', 'Add Regional Source')}
                                                        </Button>
                                                    </div>
                                                </div>
                                            </div>

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
                                                    {isBenchmarking ? t('settings.testing', 'Testing...') : t('settings.benchmarkButton', 'Test & Sort Servers by Latency')}
                                                </Button>
                                                {benchmarkError && (
                                                    <p className="text-xs text-rose-600 font-medium">{benchmarkError}</p>
                                                )}
                                                {benchmarkResults.length > 0 && (
                                                    <div className="space-y-2">
                                                        {(() => {
                                                            const maxLatency = Math.max(...benchmarkResults.filter(r => r.reachable && r.latencyMs !== undefined).map(r => r.latencyMs || 0), 0);
                                                            return benchmarkResults.map((res) => (
                                                                <div key={res.url} className="p-2.5 bg-white border border-theme-subtle rounded-xl space-y-1.5">
                                                                    <div className="flex items-center justify-between gap-2">
                                                                        <span className="text-[11px] font-mono text-theme-secondary truncate flex-1" title={res.url}>{res.url}</span>
                                                                        <span className={`text-[11px] font-bold shrink-0 ${!res.reachable ? 'text-slate-400' : (res.latencyMs !== undefined && res.latencyMs < 300 ? 'text-emerald-600' : res.latencyMs !== undefined && res.latencyMs < 600 ? 'text-orange-600' : 'text-rose-600')}`}>
                                                                            {res.reachable && res.latencyMs !== undefined ? `${res.latencyMs} ms` : t('settings.unreachable', 'unreachable')}
                                                                        </span>
                                                                    </div>
                                                                    <div className="h-2 bg-slate-100 rounded-full overflow-hidden">
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
                                        </div>
                                    )}
                                </div>
                            </div>
                        </Card>

                        {/* Language Selector */}
                        <Card 
                            className="!overflow-visible"
                            header={
                                <div className="flex items-center gap-2">
                                    <Globe size={18} className="text-theme-primary" />
                                    <span className="font-black text-xs uppercase tracking-widest">{t('settings.languageTitle')}</span>
                                </div>
                            }
                        >
                            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
                                <p className="text-xs text-theme-light font-medium">{t('settings.languageDescription')}</p>
                                <LanguageSelector variant="compact" />
                            </div>
                        </Card>

                        {/* Device Cryptographic Binding */}
                        <Card header={
                            <div className="flex items-center gap-2">
                                <Fingerprint size={18} className="text-theme-primary" />
                                <span className="font-black text-xs uppercase tracking-widest">{t('settings.deviceBindingTitle')}</span>
                            </div>
                        }>
                            <div className="space-y-4">
                                <div className="space-y-2">
                                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('settings.deviceBindingLabel')}</label>
                                    <div className="flex gap-2">
                                        <input
                                            type="text"
                                            readOnly
                                            value={localInstanceId || t('settings.retrievingId')}
                                            className="w-full bg-slate-50 border border-theme-subtle rounded-2xl px-5 py-4 text-xs font-mono text-theme-secondary select-all"
                                        />
                                        <Button 
                                            type="button" 
                                            onClick={() => {
                                                navigator.clipboard.writeText(localInstanceId);
                                            }}
                                            variant="outline"
                                            className="px-4 text-xs font-black uppercase tracking-widest"
                                        >
                                            {t('settings.copyButton')}
                                        </Button>
                                    </div>
                                    <p className="text-[10px] text-theme-light/60 font-medium leading-relaxed">
                                        {t('settings.deviceBindingDescription')}
                                    </p>
                                </div>
                            </div>
                        </Card>

                        {/* Voucher Standards Manager */}
                        <div className="border border-theme-subtle rounded-2xl overflow-hidden bg-white/50">
                            <button
                                type="button"
                                onClick={() => setIsStandardsOpen(v => !v)}
                                className="w-full flex items-center justify-between p-5 bg-slate-50/70 hover:bg-slate-100/70 transition-colors text-left"
                            >
                                <div className="flex items-center gap-3">
                                    <ShieldCheck size={18} className="text-theme-primary" />
                                    <div>
                                        <span className="font-black text-xs uppercase tracking-widest text-theme-secondary block">
                                            {t('standards.title')}
                                        </span>
                                        <p className="text-[10px] text-theme-light/70 font-medium">
                                            {t('standards.manageStandardsDescription')}
                                        </p>
                                    </div>
                                </div>
                                {isStandardsOpen ? <ChevronUp size={18} className="text-theme-light" /> : <ChevronDown size={18} className="text-theme-light" />}
                            </button>

                            {isStandardsOpen && (
                                <div className="p-6 border-t border-theme-subtle bg-white">
                                    <StandardsManager />
                                </div>
                            )}
                        </div>

                        <div className="pt-6 flex flex-col items-center gap-4">
                            <Button type="submit" disabled={isSaving} className="min-w-[240px] gap-2 rounded-2xl py-4 shadow-lg shadow-theme-primary/20">
                                {isSaving ? <Clock className="animate-spin" size={18} /> : <Save size={18} />}
                                {isSaving ? t('settings.saving') : t('settings.saveButton')}
                            </Button>
                            {error && <p className="text-sm font-bold text-rose-500 animate-bounce">{error}</p>}
                            {success && <p className="text-sm font-bold text-emerald-500 flex items-center gap-2"><CheckCircle2 size={16}/> {success}</p>}
                            {/* Hidden version trigger */}
                            <span
                                onClick={handleDevModeClick}
                                className="text-[10px] font-mono text-theme-light/40 select-none cursor-pointer hover:text-theme-light/70 transition-colors"
                                title={isDevUnlocked ? "Entwicklermodus aktiv" : `Klick ${Math.max(0, 7 - devClickCount)} bis Entwicklermodus`}
                                role="button"
                                tabIndex={0}
                                onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); handleDevModeClick(); }}}
                            >
                                v{appVersion} • {isDevUnlocked ? 'DEV freigeschaltet' : `${devClickCount}/7`}
                            </span>
                        </div>
                    </form>
                )}
            </div>
        </PageLayout>
    );
}
