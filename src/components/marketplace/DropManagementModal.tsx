// src/components/marketplace/DropManagementModal.tsx
import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import {
    Cloud,
    Server,
    RefreshCw,
    Upload,
    CheckCircle2,
    XCircle,
    AlertCircle,
    Copy,
    Check,
    Trash2,
    Lock,
    FileArchive,
    ArrowRight,
    ArrowLeft,
    Plus,
} from 'lucide-react';
import {
    FrontendDropAdminStatus,
    FrontendDiscoveredDrop,
    FrontendDropDescriptor,
    FrontendDropSyncResult,
    FrontendDropSetupDraft,
    FrontendMemberPolicyDto,
    FrontendDropConnectionTestResult,
} from '../../types';
import { marketService } from '../../services/marketService';
import { ModalShell } from '../ui/ModalShell';
import { SegmentedTabs, SegmentedTabItem } from '../ui/SegmentedTabs';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { Card } from '../ui/Card';
import { AlertBanner } from '../ui/AlertBanner';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { copyToClipboard } from '../../utils/clipboard';
import { translateError } from '../../utils/errorHelper';
import { logger } from '../../utils/log';
import { save } from '@tauri-apps/plugin-dialog';

export interface DropManagementModalProps {
    isOpen: boolean;
    onClose: () => void;
    onSyncComplete?: (importedCards: number) => void;
}

type TabType = 'use_drops' | 'own_drop';

export function DropManagementModal({
    isOpen,
    onClose,
    onSyncComplete,
}: DropManagementModalProps) {
    const { t } = useTranslation();

    const [activeTab, setActiveTab] = useState<TabType>('use_drops');
    const [isLoading, setIsLoading] = useState<boolean>(false);
    const [error, setError] = useState<string | null>(null);
    const [statusMsg, setStatusMsg] = useState<{ type: 'success' | 'error'; message: string } | null>(null);

    // Tab 1 state: Use Drops (Sync & Upload Targets)
    const [isSyncing, setIsSyncing] = useState<boolean>(false);
    const [lastSyncResult, setLastSyncResult] = useState<FrontendDropSyncResult | null>(null);
    const [discoveredDrops, setDiscoveredDrops] = useState<FrontendDiscoveredDrop[]>([]);
    const [selectedUploadUrls, setSelectedUploadUrls] = useState<Set<string>>(new Set());
    const [isSavingTargets, setIsSavingTargets] = useState<boolean>(false);

    // Tab 2 state: Own Drop (Admin & Setup Wizard)
    const [adminStatus, setAdminStatus] = useState<FrontendDropAdminStatus | null>(null);
    const [wizardStep, setWizardStep] = useState<1 | 2>(1);
    const [setupDraft, setSetupDraft] = useState<FrontendDropSetupDraft | null>(null);
    const [wizardPolicy, setWizardPolicy] = useState<FrontendMemberPolicyDto>({
        directMin: 2,
        hop2Enabled: true,
        bridgeMin: 2,
        hop2Min: 2,
    });
    const [wizardMaxMembers, setWizardMaxMembers] = useState<number>(300);
    const [wizardBaseUrl, setWizardBaseUrl] = useState<string>('');
    const [testResult, setTestResult] = useState<FrontendDropConnectionTestResult | null>(null);
    const [isTestingConn, setIsTestingConn] = useState<boolean>(false);
    const [isFinalizing, setIsFinalizing] = useState<boolean>(false);
    const [isCreatingDraft, setIsCreatingDraft] = useState<boolean>(false);
    const [isSavingZip, setIsSavingZip] = useState<boolean>(false);

    // Admin editable policy & lists
    const [adminPolicy, setAdminPolicy] = useState<FrontendMemberPolicyDto>({
        directMin: 2,
        hop2Enabled: true,
        bridgeMin: 2,
        hop2Min: 2,
    });
    const [adminMaxMembers, setAdminMaxMembers] = useState<number>(300);
    const [adminAllow, setAdminAllow] = useState<string[]>([]);
    const [adminDeny, setAdminDeny] = useState<string[]>([]);
    const [newAllowDid, setNewAllowDid] = useState<string>('');
    const [newDenyDid, setNewDenyDid] = useState<string>('');
    const [manualBlockAccessId, setManualBlockAccessId] = useState<string>('');
    const [isUpdatingAdmin, setIsUpdatingAdmin] = useState<boolean>(false);
    const [isBlockingSlot, setIsBlockingSlot] = useState<boolean>(false);
    const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState<boolean>(false);
    const [isDeletingAdmin, setIsDeletingAdmin] = useState<boolean>(false);
    const [copiedUrl, setCopiedUrl] = useState<string | null>(null);

    // Load initial data
    const loadData = useCallback(async () => {
        setIsLoading(true);
        setError(null);
        try {
            const [status, discovered] = await Promise.all([
                marketService.getDropAdminStatus().catch(() => null),
                marketService.discoverAvailableDrops().catch(() => []),
            ]);

            setAdminStatus(status);
            if (status) {
                setAdminPolicy(status.cardPolicy || {
                    directMin: 2,
                    hop2Enabled: true,
                    bridgeMin: 2,
                    hop2Min: 2,
                });
                setAdminMaxMembers(status.maxMembers || 300);
                setAdminAllow(status.allow || []);
                setAdminDeny(status.deny || []);
            }

            setDiscoveredDrops(discovered);
            const selected = new Set<string>();
            for (const d of discovered) {
                if (d.isSelected) {
                    selected.add(d.descriptor.readUrl);
                }
            }
            setSelectedUploadUrls(selected);
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsLoading(false);
        }
    }, [t]);

    useEffect(() => {
        if (isOpen) {
            loadData();
            setStatusMsg(null);
            setLastSyncResult(null);
            setWizardStep(1);
            setSetupDraft(null);
            setTestResult(null);
        }
    }, [isOpen, loadData]);

    const handleCopy = async (text: string) => {
        const ok = await copyToClipboard(text);
        if (ok) {
            setCopiedUrl(text);
            setTimeout(() => setCopiedUrl(null), 2000);
        }
    };

    // --- Tab 1 Actions: Sync & Upload Targets ---

    const handleSync = async (forceUpload: boolean = false) => {
        setIsSyncing(true);
        setError(null);
        setStatusMsg(null);
        try {
            const res = await marketService.syncMarketDrops({ forceUpload });
            setLastSyncResult(res);
            if (res.importedCards > 0 && onSyncComplete) {
                onSyncComplete(res.importedCards);
            }
            // Refresh discovered drops
            const discovered = await marketService.discoverAvailableDrops().catch(() => []);
            setDiscoveredDrops(discovered);
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsSyncing(false);
        }
    };

    const handleToggleTarget = (readUrl: string, isAuthorized: boolean) => {
        if (!isAuthorized) return;
        const next = new Set(selectedUploadUrls);
        if (next.has(readUrl)) {
            next.delete(readUrl);
        } else {
            if (next.size >= 3) {
                setError(t('marketplace.maxUploadDropsReached'));
                return;
            }
            next.add(readUrl);
            setError(null);
        }
        setSelectedUploadUrls(next);
    };

    const handleSaveUploadTargets = async () => {
        setIsSavingTargets(true);
        setError(null);
        setStatusMsg(null);
        try {
            const targetsToSave: FrontendDropDescriptor[] = [];
            for (const d of discoveredDrops) {
                if (selectedUploadUrls.has(d.descriptor.readUrl)) {
                    targetsToSave.push(d.descriptor);
                }
            }
            await marketService.setSelectedUploadDrops(targetsToSave);
            setStatusMsg({ type: 'success', message: t('marketplace.cardSavedSuccess') });
            await loadData();
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsSavingTargets(false);
        }
    };

    // --- Tab 2 Setup Wizard Actions ---

    const handlePrepareSetup = async () => {
        setIsCreatingDraft(true);
        setError(null);
        try {
            const draft = await marketService.prepareDropSetup({
                policy: wizardPolicy,
                maxMembers: wizardMaxMembers,
            });
            setSetupDraft(draft);
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsCreatingDraft(false);
        }
    };

    const downloadBase64ZipFallback = (base64Data: string, filename: string) => {
        try {
            const byteCharacters = window.atob(base64Data);
            const byteNumbers = new Array(byteCharacters.length);
            for (let i = 0; i < byteCharacters.length; i++) {
                byteNumbers[i] = byteCharacters.charCodeAt(i);
            }
            const byteArray = new Uint8Array(byteNumbers);
            const blob = new Blob([byteArray], { type: 'application/zip' });
            const url = URL.createObjectURL(blob);
            const a = document.createElement('a');
            a.href = url;
            a.download = filename;
            document.body.appendChild(a);
            a.click();
            document.body.removeChild(a);
            URL.revokeObjectURL(url);
        } catch (e) {
            logger.error('Failed to trigger base64 download: ' + String(e));
        }
    };

    const handleSaveZip = async () => {
        if (!setupDraft) return;
        setIsSavingZip(true);
        setError(null);
        try {
            let savedPath: string | null = null;
            try {
                savedPath = await save({
                    defaultPath: 'humoco-drop.zip',
                    filters: [{ name: 'ZIP Archive', extensions: ['zip'] }],
                });
            } catch {
                savedPath = null;
            }

            if (savedPath) {
                await marketService.saveDropSetupZip(savedPath);
                setStatusMsg({ type: 'success', message: t('marketplace.cardFileSaved') });
            } else if (setupDraft.zipBase64) {
                downloadBase64ZipFallback(setupDraft.zipBase64, 'humoco-drop.zip');
                setStatusMsg({ type: 'success', message: t('marketplace.cardFileDownloaded') });
            }
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsSavingZip(false);
        }
    };

    const handleTestConnection = async () => {
        if (!wizardBaseUrl.trim()) return;
        setIsTestingConn(true);
        setError(null);
        setTestResult(null);
        try {
            const res = await marketService.testDropConnection(wizardBaseUrl.trim());
            setTestResult(res);
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsTestingConn(false);
        }
    };

    const handleFinalizeSetup = async () => {
        if (!wizardBaseUrl.trim()) return;
        setIsFinalizing(true);
        setError(null);
        try {
            const status = await marketService.finalizeDropSetup(wizardBaseUrl.trim());
            setAdminStatus(status);
            setStatusMsg({ type: 'success', message: t('marketplace.connectionTestSuccess') });
            await loadData();
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsFinalizing(false);
        }
    };

    // --- Tab 2 Admin Actions ---

    const handleUpdateAdminConfig = async () => {
        setIsUpdatingAdmin(true);
        setError(null);
        setStatusMsg(null);
        try {
            const status = await marketService.updateDropAdminConfig(
                {
                    cardPolicy: adminPolicy,
                    maxMembers: adminMaxMembers,
                    allow: adminAllow,
                    deny: adminDeny,
                },
                true
            );
            setAdminStatus(status);
            setStatusMsg({ type: 'success', message: t('marketplace.cardSavedSuccess') });
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsUpdatingAdmin(false);
        }
    };

    const handleBlockSlot = async (accessId: string) => {
        if (!accessId.trim()) return;
        setIsBlockingSlot(true);
        setError(null);
        try {
            const status = await marketService.blockDropSlot(accessId.trim());
            setAdminStatus(status);
            setManualBlockAccessId('');
            setStatusMsg({ type: 'success', message: t('marketplace.cardSavedSuccess') });
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsBlockingSlot(false);
        }
    };

    const handleUnblockSlot = async (accessId: string) => {
        if (!adminStatus) return;
        setIsUpdatingAdmin(true);
        setError(null);
        try {
            const updatedBlocked = adminStatus.blockedAccessIds.filter((id) => id !== accessId);
            const status = await marketService.updateDropAdminConfig(
                { blockedAccessIds: updatedBlocked },
                true
            );
            setAdminStatus(status);
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsUpdatingAdmin(false);
        }
    };

    const handleDeleteAdmin = async () => {
        setIsDeletingAdmin(true);
        try {
            await marketService.deleteDropAdmin();
            setAdminStatus(null);
            setSetupDraft(null);
            setWizardStep(1);
            setIsDeleteConfirmOpen(false);
            await loadData();
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsDeletingAdmin(false);
        }
    };

    const tabs: SegmentedTabItem<TabType>[] = [
        {
            id: 'use_drops',
            label: t('marketplace.tabUseDrops'),
            icon: <Cloud size={14} />,
        },
        {
            id: 'own_drop',
            label: t('marketplace.tabOwnDrop'),
            icon: <Server size={14} />,
        },
    ];

    return (
        <>
            <ModalShell
                isOpen={isOpen}
                onClose={onClose}
                maxWidth="2xl"
                title={
                    <div className="flex items-center gap-2.5">
                        <div className="w-9 h-9 rounded-xl bg-theme-primary/10 text-theme-primary flex items-center justify-center">
                            <Server size={18} />
                        </div>
                        <div>
                            <h2 className="text-base font-extrabold text-theme-secondary">
                                {t('marketplace.dropsModalTitle')}
                            </h2>
                            <p className="text-[11px] text-theme-light">
                                {t('marketplace.dropsModalSubtitle')}
                            </p>
                        </div>
                    </div>
                }
            >
                <div className="space-y-4">
                    {/* Tabs navigation */}
                    <SegmentedTabs
                        tabs={tabs}
                        activeTab={activeTab}
                        onChange={(tab) => {
                            setActiveTab(tab);
                            setError(null);
                            setStatusMsg(null);
                        }}
                        fullWidth
                    />

                    {error && (
                        <AlertBanner
                            variant="error"
                            onDismiss={() => setError(null)}
                        >
                            {error}
                        </AlertBanner>
                    )}

                    {statusMsg && (
                        <AlertBanner
                            variant={statusMsg.type === 'success' ? 'success' : 'error'}
                            onDismiss={() => setStatusMsg(null)}
                        >
                            {statusMsg.message}
                        </AlertBanner>
                    )}

                    {/* TAB 1: Use Drops (Sync & Upload Targets) */}
                    {activeTab === 'use_drops' && (
                        <div className="space-y-4">
                            {/* Sync Actions Card */}
                            <Card className="p-4 space-y-3">
                                <div className="flex items-center justify-between">
                                    <div className="flex items-center gap-2">
                                        <RefreshCw size={16} className={`text-theme-primary ${isSyncing ? 'animate-spin' : ''}`} />
                                        <h3 className="text-xs font-bold text-theme-secondary uppercase tracking-wider">
                                            {t('marketplace.syncStatus')}
                                        </h3>
                                    </div>
                                    <div className="flex items-center gap-2">
                                        <Button
                                            size="sm"
                                            variant="secondary"
                                            disabled={isSyncing}
                                            onClick={() => handleSync(true)}
                                            className="text-xs gap-1.5"
                                        >
                                            <Upload size={13} />
                                            <span>{t('marketplace.forceUploadBtn')}</span>
                                        </Button>
                                        <Button
                                            size="sm"
                                            disabled={isSyncing}
                                            onClick={() => handleSync(false)}
                                            className="text-xs gap-1.5"
                                        >
                                            <RefreshCw size={13} className={isSyncing ? 'animate-spin' : ''} />
                                            <span>{t('marketplace.syncNowBtn')}</span>
                                        </Button>
                                    </div>
                                </div>

                                {lastSyncResult && (
                                    <div className="pt-2 border-t border-theme-subtle/50 text-xs">
                                        {lastSyncResult.skippedWifiOnly ? (
                                            <div className="p-2.5 bg-amber-50 rounded-xl text-amber-800 border border-amber-200 flex items-center gap-2">
                                                <AlertCircle size={15} />
                                                <span>{t('marketplace.syncSkippedWifi')}</span>
                                            </div>
                                        ) : (
                                            <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
                                                <div className="p-2.5 bg-bg-app rounded-xl border border-theme-subtle/60 text-center">
                                                    <div className="text-base font-extrabold text-theme-secondary">
                                                        {lastSyncResult.uploadedTo}
                                                    </div>
                                                    <div className="text-[10px] text-theme-light">
                                                        {t('marketplace.syncUploadedTo', { count: lastSyncResult.uploadedTo })}
                                                    </div>
                                                </div>
                                                <div className="p-2.5 bg-bg-app rounded-xl border border-theme-subtle/60 text-center">
                                                    <div className="text-base font-extrabold text-theme-secondary">
                                                        {lastSyncResult.polledDrops}
                                                    </div>
                                                    <div className="text-[10px] text-theme-light">
                                                        {t('marketplace.syncPolledDrops', { count: lastSyncResult.polledDrops })}
                                                    </div>
                                                </div>
                                                <div className="p-2.5 bg-bg-app rounded-xl border border-theme-subtle/60 text-center">
                                                    <div className="text-base font-extrabold text-theme-secondary">
                                                        {lastSyncResult.decryptedSlots}
                                                    </div>
                                                    <div className="text-[10px] text-theme-light">
                                                        {t('marketplace.syncDecryptedSlots', { count: lastSyncResult.decryptedSlots })}
                                                    </div>
                                                </div>
                                                <div className="p-2.5 bg-emerald-50 rounded-xl border border-emerald-200 text-center">
                                                    <div className="text-base font-extrabold text-emerald-800">
                                                        {lastSyncResult.importedCards}
                                                    </div>
                                                    <div className="text-[10px] text-emerald-700">
                                                        {t('marketplace.syncImportedCards', { count: lastSyncResult.importedCards })}
                                                    </div>
                                                </div>
                                            </div>
                                        )}
                                        {lastSyncResult.errors.length > 0 && (
                                            <div className="mt-2 p-2 bg-rose-50 rounded-xl border border-rose-200 text-rose-800 text-[11px] space-y-1">
                                                {lastSyncResult.errors.map((e, idx) => (
                                                    <div key={idx} className="flex items-start gap-1.5">
                                                        <XCircle size={13} className="shrink-0 mt-0.5" />
                                                        <span>{e}</span>
                                                    </div>
                                                ))}
                                            </div>
                                        )}
                                    </div>
                                )}
                            </Card>

                            {/* Upload Targets Discovery Card */}
                            <Card className="p-4 space-y-3">
                                <div className="flex items-center justify-between">
                                    <div>
                                        <h3 className="text-xs font-bold text-theme-secondary uppercase tracking-wider">
                                            {t('marketplace.availableDropsTitle')}
                                        </h3>
                                        <p className="text-[11px] text-theme-light mt-0.5">
                                            {t('marketplace.availableDropsDesc')}
                                        </p>
                                    </div>
                                    <Button
                                        size="sm"
                                        variant="secondary"
                                        disabled={isLoading}
                                        onClick={loadData}
                                        className="text-xs gap-1.5"
                                    >
                                        <RefreshCw size={13} className={isLoading ? 'animate-spin' : ''} />
                                        <span>{t('marketplace.checkDropsBtn')}</span>
                                    </Button>
                                </div>

                                {discoveredDrops.length === 0 ? (
                                    <div className="p-6 text-center text-xs text-theme-light italic bg-bg-app rounded-xl border border-dashed border-theme-subtle">
                                        {t('marketplace.noDiscoveredDrops')}
                                    </div>
                                ) : (
                                    <div className="space-y-2">
                                        {discoveredDrops.map((drop) => {
                                            const isSelected = selectedUploadUrls.has(drop.descriptor.readUrl);
                                            return (
                                                <div
                                                    key={drop.descriptor.readUrl}
                                                    className={`p-3 rounded-xl border transition-all flex items-center justify-between gap-3 ${
                                                        isSelected
                                                            ? 'bg-theme-primary/5 border-theme-primary/40 shadow-xs'
                                                            : 'bg-white border-theme-subtle/70 hover:border-theme-subtle'
                                                    }`}
                                                >
                                                    <div className="min-w-0 flex-1 space-y-0.5">
                                                        <div className="flex items-center gap-2">
                                                            <span className="text-xs font-bold text-theme-secondary truncate">
                                                                {drop.sourceLabel}
                                                            </span>
                                                            <span
                                                                className={`px-1.5 py-0.2 rounded-md text-[9px] font-extrabold uppercase tracking-wider ${
                                                                    drop.isAuthorized
                                                                        ? 'bg-emerald-100 text-emerald-800'
                                                                        : 'bg-slate-100 text-slate-600'
                                                                }`}
                                                            >
                                                                {drop.isAuthorized
                                                                    ? t('marketplace.authorizedBadge')
                                                                    : t('marketplace.readOnlyBadge')}
                                                            </span>
                                                        </div>
                                                        <p className="text-[10px] font-mono text-theme-light truncate">
                                                            {drop.descriptor.readUrl}
                                                        </p>
                                                    </div>

                                                    <input
                                                        type="checkbox"
                                                        disabled={!drop.isAuthorized}
                                                        checked={isSelected}
                                                        onChange={() => handleToggleTarget(drop.descriptor.readUrl, drop.isAuthorized)}
                                                        className="h-4 w-4 rounded accent-theme-primary cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
                                                    />
                                                </div>
                                            );
                                        })}

                                        <div className="pt-2 flex justify-end">
                                            <Button
                                                size="sm"
                                                disabled={isSavingTargets}
                                                onClick={handleSaveUploadTargets}
                                                className="text-xs"
                                            >
                                                {t('marketplace.saveUploadDropsBtn')}
                                            </Button>
                                        </div>
                                    </div>
                                )}
                            </Card>
                        </div>
                    )}

                    {/* TAB 2: Own Drop (Setup Wizard or Admin View) */}
                    {activeTab === 'own_drop' && (
                        <div>
                            {!adminStatus || !adminStatus.descriptor.writeUrl ? (
                                /* SETUP WIZARD */
                                <div className="space-y-4">
                                    <div className="flex items-center justify-between p-3 bg-bg-app rounded-xl border border-theme-subtle text-xs font-bold">
                                        <span className={wizardStep === 1 ? 'text-theme-primary' : 'text-theme-light'}>
                                            {t('marketplace.wizardStep1Title')}
                                        </span>
                                        <ArrowRight size={14} className="text-theme-light" />
                                        <span className={wizardStep === 2 ? 'text-theme-primary' : 'text-theme-light'}>
                                            {t('marketplace.wizardStep2Title')}
                                        </span>
                                    </div>

                                    {wizardStep === 1 && (
                                        <Card className="p-4 space-y-4">
                                            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                                                <div>
                                                    <label className="block text-[11px] font-bold text-theme-secondary mb-1">
                                                        {t('marketplace.policyDirectMin')}
                                                    </label>
                                                    <select
                                                        value={wizardPolicy.directMin}
                                                        onChange={(e) => setWizardPolicy({ ...wizardPolicy, directMin: Number(e.target.value) })}
                                                        className="w-full h-8 px-2.5 bg-bg-app border border-theme-subtle rounded-lg text-xs font-semibold text-theme-secondary outline-none focus:ring-2 focus:ring-theme-primary/20"
                                                    >
                                                        <option value={0}>Stufe 0 (Alle)</option>
                                                        <option value={1}>Stufe 1 (Bekannt)</option>
                                                        <option value={2}>Stufe 2 (Vertraut)</option>
                                                        <option value={3}>Stufe 3 (Kern-Partner)</option>
                                                    </select>
                                                </div>

                                                <div>
                                                    <label className="block text-[11px] font-bold text-theme-secondary mb-1">
                                                        {t('marketplace.policyMaxMembers')}
                                                    </label>
                                                    <Input
                                                        type="number"
                                                        value={wizardMaxMembers}
                                                        onChange={(e) => setWizardMaxMembers(Math.max(1, Number(e.target.value)))}
                                                    />
                                                </div>

                                                <div className="sm:col-span-2 flex items-center gap-2 pt-1">
                                                    <input
                                                        type="checkbox"
                                                        id="wizardHop2Enabled"
                                                        checked={wizardPolicy.hop2Enabled}
                                                        onChange={(e) => setWizardPolicy({ ...wizardPolicy, hop2Enabled: e.target.checked })}
                                                        className="h-4 w-4 rounded accent-theme-primary"
                                                    />
                                                    <label htmlFor="wizardHop2Enabled" className="text-xs font-medium text-theme-secondary cursor-pointer">
                                                        {t('marketplace.policyHop2Enabled')}
                                                    </label>
                                                </div>

                                                {wizardPolicy.hop2Enabled && (
                                                    <>
                                                        <div>
                                                            <label className="block text-[11px] font-bold text-theme-secondary mb-1">
                                                                {t('marketplace.policyBridgeMin')}
                                                            </label>
                                                            <select
                                                                value={wizardPolicy.bridgeMin}
                                                                onChange={(e) => setWizardPolicy({ ...wizardPolicy, bridgeMin: Number(e.target.value) })}
                                                                className="w-full h-8 px-2.5 bg-bg-app border border-theme-subtle rounded-lg text-xs font-semibold text-theme-secondary outline-none focus:ring-2 focus:ring-theme-primary/20"
                                                            >
                                                                <option value={1}>Stufe 1</option>
                                                                <option value={2}>Stufe 2</option>
                                                                <option value={3}>Stufe 3</option>
                                                            </select>
                                                        </div>

                                                        <div>
                                                            <label className="block text-[11px] font-bold text-theme-secondary mb-1">
                                                                {t('marketplace.policyHop2Min')}
                                                            </label>
                                                            <select
                                                                value={wizardPolicy.hop2Min}
                                                                onChange={(e) => setWizardPolicy({ ...wizardPolicy, hop2Min: Number(e.target.value) })}
                                                                className="w-full h-8 px-2.5 bg-bg-app border border-theme-subtle rounded-lg text-xs font-semibold text-theme-secondary outline-none focus:ring-2 focus:ring-theme-primary/20"
                                                            >
                                                                <option value={1}>Stufe 1</option>
                                                                <option value={2}>Stufe 2</option>
                                                                <option value={3}>Stufe 3</option>
                                                            </select>
                                                        </div>
                                                    </>
                                                )}
                                            </div>

                                            <div className="pt-2 flex items-center justify-between border-t border-theme-subtle/50">
                                                <Button
                                                    size="sm"
                                                    disabled={isCreatingDraft}
                                                    onClick={handlePrepareSetup}
                                                    className="text-xs gap-1.5"
                                                >
                                                    <FileArchive size={13} />
                                                    <span>{t('marketplace.generateConfigBtn')}</span>
                                                </Button>

                                                {setupDraft && (
                                                    <Button
                                                        size="sm"
                                                        variant="secondary"
                                                        onClick={() => setWizardStep(2)}
                                                        className="text-xs gap-1.5"
                                                    >
                                                        <span>{t('marketplace.nextToStep2Btn')}</span>
                                                        <ArrowRight size={13} />
                                                    </Button>
                                                )}
                                            </div>

                                            {setupDraft && (
                                                <div className="p-3 bg-emerald-50 rounded-xl border border-emerald-200 text-xs text-emerald-900 space-y-2.5">
                                                    <div className="flex items-center justify-between">
                                                        <div className="font-bold flex items-center gap-1.5">
                                                            <CheckCircle2 size={15} className="text-emerald-700" />
                                                            <span>Konfiguration & ZIP erzeugt!</span>
                                                        </div>
                                                        <Button
                                                            size="sm"
                                                            disabled={isSavingZip}
                                                            onClick={handleSaveZip}
                                                            className="text-xs gap-1.5 shadow-xs"
                                                        >
                                                            <FileArchive size={13} />
                                                            <span>{t('marketplace.saveZipBtn')}</span>
                                                        </Button>
                                                    </div>
                                                    <p className="text-[11px] leading-relaxed text-emerald-800">
                                                        {t('marketplace.zipInstructions')}
                                                    </p>
                                                    <div className="text-[10px] font-mono text-emerald-700 bg-white/70 p-2 rounded-lg border border-emerald-200/50">
                                                        Daten-Verzeichnis: {setupDraft.dataDir}
                                                    </div>
                                                </div>
                                            )}
                                        </Card>
                                    )}

                                    {wizardStep === 2 && (
                                        <Card className="p-4 space-y-4">
                                            <div className="space-y-2">
                                                <label className="block text-[11px] font-bold text-theme-secondary">
                                                    {t('marketplace.dropBaseUrlLabel')}
                                                </label>
                                                <Input
                                                    type="url"
                                                    value={wizardBaseUrl}
                                                    onChange={(e) => setWizardBaseUrl(e.target.value)}
                                                    placeholder={t('marketplace.dropBaseUrlPlaceholder')}
                                                />
                                            </div>

                                            <div className="flex items-center gap-2">
                                                <Button
                                                    size="sm"
                                                    variant="secondary"
                                                    disabled={!wizardBaseUrl.trim() || isTestingConn}
                                                    onClick={handleTestConnection}
                                                    className="text-xs gap-1.5"
                                                >
                                                    <RefreshCw size={13} className={isTestingConn ? 'animate-spin' : ''} />
                                                    <span>{t('marketplace.testConnectionBtn')}</span>
                                                </Button>

                                                <Button
                                                    size="sm"
                                                    disabled={!wizardBaseUrl.trim() || isFinalizing}
                                                    onClick={handleFinalizeSetup}
                                                    className="text-xs gap-1.5"
                                                >
                                                    <CheckCircle2 size={13} />
                                                    <span>{t('marketplace.finalizeSetupBtn')}</span>
                                                </Button>
                                            </div>

                                            {testResult && (
                                                <div
                                                    className={`p-3 rounded-xl border text-xs ${
                                                        testResult.success
                                                            ? 'bg-emerald-50 border-emerald-200 text-emerald-900'
                                                            : 'bg-rose-50 border-rose-200 text-rose-900'
                                                    }`}
                                                >
                                                    <div className="flex items-center gap-1.5 font-bold mb-1">
                                                        {testResult.success ? (
                                                            <CheckCircle2 size={15} className="text-emerald-700" />
                                                        ) : (
                                                            <XCircle size={15} className="text-rose-700" />
                                                        )}
                                                        <span>
                                                            {testResult.success
                                                                ? t('marketplace.connectionTestSuccess')
                                                                : t('marketplace.connectionTestFailed')}
                                                        </span>
                                                    </div>
                                                    <p className="text-[11px] leading-relaxed">
                                                        {testResult.message}
                                                    </p>
                                                    <div className="mt-1 flex gap-2 text-[10px] font-semibold">
                                                        <span>Schreiben: {testResult.writeOk ? '✓ OK' : '✕ Fehlgeschlagen'}</span>
                                                        <span>·</span>
                                                        <span>Lesen: {testResult.readOk ? '✓ OK' : '✕ Fehlgeschlagen'}</span>
                                                    </div>
                                                </div>
                                            )}

                                            <div className="pt-2 border-t border-theme-subtle/50">
                                                <Button
                                                    size="sm"
                                                    variant="secondary"
                                                    onClick={() => setWizardStep(1)}
                                                    className="text-xs gap-1.5"
                                                >
                                                    <ArrowLeft size={13} />
                                                    <span>{t('marketplace.backToStep1Btn')}</span>
                                                </Button>
                                            </div>
                                        </Card>
                                    )}
                                </div>
                            ) : (
                                /* ADMIN VIEW */
                                <div className="space-y-4">
                                    {/* URLs & DataDir */}
                                    <Card className="p-4 space-y-2.5">
                                        <div className="flex items-center justify-between">
                                            <h3 className="text-xs font-bold text-theme-secondary uppercase tracking-wider">
                                                {t('marketplace.adminTitle')}
                                            </h3>
                                            <Button
                                                size="sm"
                                                variant="danger"
                                                onClick={() => setIsDeleteConfirmOpen(true)}
                                                className="text-[10px] gap-1 px-2 py-1"
                                            >
                                                <Trash2 size={11} />
                                                <span>{t('marketplace.deleteAdminBtn')}</span>
                                            </Button>
                                        </div>

                                        <div className="space-y-1.5 text-xs">
                                            <div className="flex items-center justify-between p-2 bg-bg-app rounded-lg border border-theme-subtle/60">
                                                <div className="min-w-0 flex-1 pr-2">
                                                    <span className="text-[10px] text-theme-light block font-semibold">
                                                        {t('marketplace.adminReadUrl')}
                                                    </span>
                                                    <span className="font-mono text-[11px] truncate block text-theme-secondary">
                                                        {adminStatus.descriptor.readUrl}
                                                    </span>
                                                </div>
                                                <button
                                                    type="button"
                                                    onClick={() => handleCopy(adminStatus.descriptor.readUrl)}
                                                    className="p-1.5 text-theme-light hover:text-theme-primary transition-colors"
                                                >
                                                    {copiedUrl === adminStatus.descriptor.readUrl ? <Check size={14} className="text-emerald-600" /> : <Copy size={14} />}
                                                </button>
                                            </div>

                                            <div className="flex items-center justify-between p-2 bg-bg-app rounded-lg border border-theme-subtle/60">
                                                <div className="min-w-0 flex-1 pr-2">
                                                    <span className="text-[10px] text-theme-light block font-semibold">
                                                        {t('marketplace.adminWriteUrl')}
                                                    </span>
                                                    <span className="font-mono text-[11px] truncate block text-theme-secondary">
                                                        {adminStatus.descriptor.writeUrl}
                                                    </span>
                                                </div>
                                                <button
                                                    type="button"
                                                    onClick={() => handleCopy(adminStatus.descriptor.writeUrl)}
                                                    className="p-1.5 text-theme-light hover:text-theme-primary transition-colors"
                                                >
                                                    {copiedUrl === adminStatus.descriptor.writeUrl ? <Check size={14} className="text-emerald-600" /> : <Copy size={14} />}
                                                </button>
                                            </div>
                                        </div>
                                    </Card>

                                    {/* Member Counters */}
                                    <div className="grid grid-cols-3 gap-2">
                                        <Card className="p-3 text-center">
                                            <div className="text-base font-extrabold text-theme-secondary">
                                                {adminStatus.directMemberCount}
                                            </div>
                                            <div className="text-[10px] text-theme-light font-medium">
                                                {t('marketplace.counterDirect')}
                                            </div>
                                        </Card>
                                        <Card className="p-3 text-center">
                                            <div className="text-base font-extrabold text-theme-secondary">
                                                {adminStatus.hop2MemberCount}
                                            </div>
                                            <div className="text-[10px] text-theme-light font-medium">
                                                {t('marketplace.counterHop2')}
                                            </div>
                                        </Card>
                                        <Card className="p-3 text-center">
                                            <div className="text-base font-extrabold text-theme-primary">
                                                {adminStatus.totalMemberCount} / {adminStatus.maxMembers}
                                            </div>
                                            <div className="text-[10px] text-theme-light font-medium">
                                                {t('marketplace.counterTotal')}
                                            </div>
                                        </Card>
                                    </div>

                                    {/* Policy & Allow/Deny List Configuration */}
                                    <Card className="p-4 space-y-3">
                                        <h3 className="text-xs font-bold text-theme-secondary uppercase tracking-wider">
                                            Freigabe-Richtlinie (Channel „card“)
                                        </h3>

                                        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                                            <div>
                                                <label className="block text-[11px] font-bold text-theme-secondary mb-1">
                                                    {t('marketplace.policyDirectMin')}
                                                </label>
                                                <select
                                                    value={adminPolicy.directMin}
                                                    onChange={(e) => setAdminPolicy({ ...adminPolicy, directMin: Number(e.target.value) })}
                                                    className="w-full h-8 px-2.5 bg-bg-app border border-theme-subtle rounded-lg text-xs font-semibold text-theme-secondary outline-none focus:ring-2 focus:ring-theme-primary/20"
                                                >
                                                    <option value={0}>Stufe 0 (Alle)</option>
                                                    <option value={1}>Stufe 1 (Bekannt)</option>
                                                    <option value={2}>Stufe 2 (Vertraut)</option>
                                                    <option value={3}>Stufe 3 (Kern-Partner)</option>
                                                </select>
                                            </div>

                                            <div>
                                                <label className="block text-[11px] font-bold text-theme-secondary mb-1">
                                                    {t('marketplace.policyMaxMembers')}
                                                </label>
                                                <Input
                                                    type="number"
                                                    value={adminMaxMembers}
                                                    onChange={(e) => setAdminMaxMembers(Math.max(1, Number(e.target.value)))}
                                                />
                                            </div>

                                            <div className="sm:col-span-2 flex items-center gap-2 pt-1">
                                                <input
                                                    type="checkbox"
                                                    id="adminHop2Enabled"
                                                    checked={adminPolicy.hop2Enabled}
                                                    onChange={(e) => setAdminPolicy({ ...adminPolicy, hop2Enabled: e.target.checked })}
                                                    className="h-4 w-4 rounded accent-theme-primary"
                                                />
                                                <label htmlFor="adminHop2Enabled" className="text-xs font-medium text-theme-secondary cursor-pointer">
                                                    {t('marketplace.policyHop2Enabled')}
                                                </label>
                                            </div>

                                            {adminPolicy.hop2Enabled && (
                                                <>
                                                    <div>
                                                        <label className="block text-[11px] font-bold text-theme-secondary mb-1">
                                                            {t('marketplace.policyBridgeMin')}
                                                        </label>
                                                        <select
                                                            value={adminPolicy.bridgeMin}
                                                            onChange={(e) => setAdminPolicy({ ...adminPolicy, bridgeMin: Number(e.target.value) })}
                                                            className="w-full h-8 px-2.5 bg-bg-app border border-theme-subtle rounded-lg text-xs font-semibold text-theme-secondary outline-none focus:ring-2 focus:ring-theme-primary/20"
                                                        >
                                                            <option value={1}>Stufe 1</option>
                                                            <option value={2}>Stufe 2</option>
                                                            <option value={3}>Stufe 3</option>
                                                        </select>
                                                    </div>

                                                    <div>
                                                        <label className="block text-[11px] font-bold text-theme-secondary mb-1">
                                                            {t('marketplace.policyHop2Min')}
                                                        </label>
                                                        <select
                                                            value={adminPolicy.hop2Min}
                                                            onChange={(e) => setAdminPolicy({ ...adminPolicy, hop2Min: Number(e.target.value) })}
                                                            className="w-full h-8 px-2.5 bg-bg-app border border-theme-subtle rounded-lg text-xs font-semibold text-theme-secondary outline-none focus:ring-2 focus:ring-theme-primary/20"
                                                        >
                                                            <option value={1}>Stufe 1</option>
                                                            <option value={2}>Stufe 2</option>
                                                            <option value={3}>Stufe 3</option>
                                                        </select>
                                                    </div>
                                                </>
                                            )}
                                        </div>

                                        {/* Allow and Deny Lists */}
                                        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-2">
                                            {/* Allow List */}
                                            <div className="space-y-1.5">
                                                <label className="block text-[10px] font-bold text-theme-light uppercase tracking-wider">
                                                    {t('marketplace.allowListLabel')}
                                                </label>
                                                <div className="flex gap-1.5">
                                                    <Input
                                                        type="text"
                                                        placeholder="did:key:..."
                                                        value={newAllowDid}
                                                        onChange={(e) => setNewAllowDid(e.target.value)}
                                                        className="text-xs font-mono"
                                                    />
                                                    <Button
                                                        size="sm"
                                                        variant="secondary"
                                                        disabled={!newAllowDid.trim()}
                                                        onClick={() => {
                                                            if (newAllowDid.trim() && !adminAllow.includes(newAllowDid.trim())) {
                                                                setAdminAllow([...adminAllow, newAllowDid.trim()]);
                                                                setNewAllowDid('');
                                                            }
                                                        }}
                                                    >
                                                        <Plus size={13} />
                                                    </Button>
                                                </div>
                                                {adminAllow.length > 0 && (
                                                    <div className="max-h-24 overflow-y-auto space-y-1 p-1 bg-bg-app rounded-lg border border-theme-subtle/50 text-[10px] font-mono">
                                                        {adminAllow.map((did) => (
                                                            <div key={did} className="flex items-center justify-between px-1.5 py-0.5 rounded bg-white">
                                                                <span className="truncate pr-1">{did}</span>
                                                                <button
                                                                    type="button"
                                                                    onClick={() => setAdminAllow(adminAllow.filter((d) => d !== did))}
                                                                    className="text-rose-500 hover:text-rose-700"
                                                                >
                                                                    <Trash2 size={11} />
                                                                </button>
                                                            </div>
                                                        ))}
                                                    </div>
                                                )}
                                            </div>

                                            {/* Deny List */}
                                            <div className="space-y-1.5">
                                                <label className="block text-[10px] font-bold text-theme-light uppercase tracking-wider">
                                                    {t('marketplace.denyListLabel')}
                                                </label>
                                                <div className="flex gap-1.5">
                                                    <Input
                                                        type="text"
                                                        placeholder="did:key:..."
                                                        value={newDenyDid}
                                                        onChange={(e) => setNewDenyDid(e.target.value)}
                                                        className="text-xs font-mono"
                                                    />
                                                    <Button
                                                        size="sm"
                                                        variant="secondary"
                                                        disabled={!newDenyDid.trim()}
                                                        onClick={() => {
                                                            if (newDenyDid.trim() && !adminDeny.includes(newDenyDid.trim())) {
                                                                setAdminDeny([...adminDeny, newDenyDid.trim()]);
                                                                setNewDenyDid('');
                                                            }
                                                        }}
                                                    >
                                                        <Plus size={13} />
                                                    </Button>
                                                </div>
                                                {adminDeny.length > 0 && (
                                                    <div className="max-h-24 overflow-y-auto space-y-1 p-1 bg-bg-app rounded-lg border border-theme-subtle/50 text-[10px] font-mono">
                                                        {adminDeny.map((did) => (
                                                            <div key={did} className="flex items-center justify-between px-1.5 py-0.5 rounded bg-white">
                                                                <span className="truncate pr-1">{did}</span>
                                                                <button
                                                                    type="button"
                                                                    onClick={() => setAdminDeny(adminDeny.filter((d) => d !== did))}
                                                                    className="text-rose-500 hover:text-rose-700"
                                                                >
                                                                    <Trash2 size={11} />
                                                                </button>
                                                            </div>
                                                        ))}
                                                    </div>
                                                )}
                                            </div>
                                        </div>

                                        <div className="pt-2 flex justify-end">
                                            <Button
                                                size="sm"
                                                disabled={isUpdatingAdmin}
                                                onClick={handleUpdateAdminConfig}
                                                className="text-xs gap-1.5"
                                            >
                                                <Upload size={13} />
                                                <span>{t('marketplace.saveAndPushBtn')}</span>
                                            </Button>
                                        </div>
                                    </Card>

                                    {/* Slot Blocking & Management */}
                                    <Card className="p-4 space-y-3">
                                        <div>
                                            <h3 className="text-xs font-bold text-theme-secondary uppercase tracking-wider">
                                                {t('marketplace.blockSlotTitle')}
                                            </h3>
                                            <p className="text-[11px] text-theme-light mt-0.5">
                                                {t('marketplace.blockSlotDesc')}
                                            </p>
                                        </div>

                                        {/* Manual Access-ID blocking */}
                                        <div className="flex gap-2">
                                            <Input
                                                type="text"
                                                placeholder={t('marketplace.manualAccessIdPlaceholder')}
                                                value={manualBlockAccessId}
                                                onChange={(e) => setManualBlockAccessId(e.target.value.toLowerCase().trim())}
                                                className="text-xs font-mono"
                                            />
                                            <Button
                                                size="sm"
                                                variant="danger"
                                                disabled={!manualBlockAccessId.trim() || isBlockingSlot}
                                                onClick={() => handleBlockSlot(manualBlockAccessId)}
                                                className="text-xs gap-1"
                                            >
                                                <Lock size={12} />
                                                <span>{t('marketplace.blockBtn')}</span>
                                            </Button>
                                        </div>

                                        {/* Active Slots List */}
                                        {adminStatus.activeSlotIds.length > 0 && (
                                            <div className="space-y-1.5 pt-1">
                                                <span className="text-[10px] font-bold text-theme-light uppercase tracking-wider block">
                                                    {t('marketplace.activeSlotsLabel')} ({adminStatus.activeSlotIds.length})
                                                </span>
                                                <div className="max-h-32 overflow-y-auto space-y-1 p-1 bg-bg-app rounded-xl border border-theme-subtle/50 text-[11px] font-mono">
                                                    {adminStatus.activeSlotIds.map((slotId) => (
                                                        <div
                                                            key={slotId}
                                                            className="flex items-center justify-between p-1.5 rounded-lg bg-white border border-theme-subtle/40"
                                                        >
                                                            <span className="truncate pr-2">{slotId}</span>
                                                            <button
                                                                type="button"
                                                                onClick={() => handleBlockSlot(slotId)}
                                                                className="px-2 py-0.5 rounded text-[10px] font-bold text-rose-600 hover:bg-rose-50 border border-rose-200 transition-colors"
                                                            >
                                                                {t('marketplace.blockBtn')}
                                                            </button>
                                                        </div>
                                                    ))}
                                                </div>
                                            </div>
                                        )}

                                        {/* Blocked Slots List */}
                                        {adminStatus.blockedAccessIds.length > 0 && (
                                            <div className="space-y-1.5 pt-1">
                                                <span className="text-[10px] font-bold text-rose-600 uppercase tracking-wider block">
                                                    {t('marketplace.blockedSlotsLabel')} ({adminStatus.blockedAccessIds.length})
                                                </span>
                                                <div className="max-h-28 overflow-y-auto space-y-1 p-1 bg-rose-50/50 rounded-xl border border-rose-200/60 text-[11px] font-mono">
                                                    {adminStatus.blockedAccessIds.map((accessId) => (
                                                        <div
                                                            key={accessId}
                                                            className="flex items-center justify-between p-1.5 rounded-lg bg-white border border-rose-200/50"
                                                        >
                                                            <span className="truncate pr-2 text-rose-900">{accessId}</span>
                                                            <button
                                                                type="button"
                                                                onClick={() => handleUnblockSlot(accessId)}
                                                                className="px-2 py-0.5 rounded text-[10px] font-bold text-teal-700 hover:bg-teal-50 border border-teal-200 transition-colors"
                                                            >
                                                                {t('marketplace.unblockBtn')}
                                                            </button>
                                                        </div>
                                                    ))}
                                                </div>
                                            </div>
                                        )}
                                    </Card>
                                </div>
                            )}
                        </div>
                    )}
                </div>
            </ModalShell>

            {/* Delete Admin Confirmation Modal */}
            <ConfirmationModal
                isOpen={isDeleteConfirmOpen}
                title={t('marketplace.deleteAdminConfirmTitle')}
                description={t('marketplace.deleteAdminConfirmDesc')}
                confirmVariant="danger"
                confirmText={t('common.remove')}
                onConfirm={handleDeleteAdmin}
                onCancel={() => setIsDeleteConfirmOpen(false)}
                isProcessing={isDeletingAdmin}
            />
        </>
    );
}
