// src/components/marketplace/DropManagementModal.tsx
import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import {
    Server,
    Globe,
    Users,
    RefreshCw,
    Upload,
    CheckCircle2,
    XCircle,
    AlertCircle,
    AlertTriangle,
    Copy,
    Check,
    Trash2,
    Lock,
    FileArchive,
    ArrowRight,
    ArrowLeft,
    Plus,
    ExternalLink,
    Shield,
    HardDrive,
    KeyRound,
} from 'lucide-react';
import {
    FrontendDropAdminStatus,
    FrontendDiscoveredDrop,
    FrontendDropDescriptor,
    FrontendDropSyncResult,
    FrontendDropSetupDraft,
    FrontendMemberPolicyDto,
    FrontendDropConnectionTestResult,
    FrontendPublicDropInfo,
    StoredPublicDrop,
} from '../../types';
import { marketService } from '../../services/marketService';
import { settingsService } from '../../services/settingsService';
import { notifyDropStatusUpdated } from '../../utils/dropEvents';
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
import { openUrl } from '@tauri-apps/plugin-opener';
import { backupService } from '../../services/backupService';

export type DropModalTab = 'friend_drops' | 'public_drops' | 'own_drop';

export interface DropManagementModalProps {
    isOpen: boolean;
    onClose: () => void;
    onSyncComplete?: (importedCards: number) => void;
    initialTab?: DropModalTab;
}

export function DropManagementModal({
    isOpen,
    onClose,
    onSyncComplete,
    initialTab = 'friend_drops',
}: DropManagementModalProps) {
    const { t } = useTranslation();

    const [activeTab, setActiveTab] = useState<DropModalTab>(initialTab);
    const [isLoading, setIsLoading] = useState<boolean>(false);
    const [error, setError] = useState<string | null>(null);
    const [statusMsg, setStatusMsg] = useState<{ type: 'success' | 'error'; message: string } | null>(null);

    // Tab 1 state: Friends' Drops (Sync & Upload Targets)
    const [isSyncing, setIsSyncing] = useState<boolean>(false);
    const [lastSyncResult, setLastSyncResult] = useState<FrontendDropSyncResult | null>(null);
    const [discoveredDrops, setDiscoveredDrops] = useState<FrontendDiscoveredDrop[]>([]);
    const [selectedUploadUrls, setSelectedUploadUrls] = useState<Set<string>>(new Set());
    const [isSavingTargets, setIsSavingTargets] = useState<boolean>(false);

    // Tab 2 state: Public Drops
    const [publicDropInputUrl, setPublicDropInputUrl] = useState<string>('');
    const [isProbingPublic, setIsProbingPublic] = useState<boolean>(false);
    const [probedPublicDrop, setProbedPublicDrop] = useState<FrontendPublicDropInfo | null>(null);
    const [configuredPublicDrops, setConfiguredPublicDrops] = useState<StoredPublicDrop[]>([]);
    const [isSavingPublic, setIsSavingPublic] = useState<boolean>(false);
    const [isCheckingAuth, setIsCheckingAuth] = useState<boolean>(false);
    const [deletePublicTarget, setDeletePublicTarget] = useState<StoredPublicDrop | null>(null);
    const [deleteRemoteSlots, setDeleteRemoteSlots] = useState<boolean>(false);
    const [isDeletingPublic, setIsDeletingPublic] = useState<boolean>(false);

    // Tab 3 state: Own Drop (Admin & Setup Wizard)
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
    const [wizardMaxStorageMb, setWizardMaxStorageMb] = useState<number>(50);
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
    const [adminMaxStorageMb, setAdminMaxStorageMb] = useState<number>(50);
    const [adminAllow, setAdminAllow] = useState<string[]>([]);
    const [adminDeny, setAdminDeny] = useState<string[]>([]);
    const [newAllowDid, setNewAllowDid] = useState<string>('');
    const [newDenyDid, setNewDenyDid] = useState<string>('');
    const [manualBlockPubkey, setManualBlockPubkey] = useState<string>('');
    const [isUpdatingAdmin, setIsUpdatingAdmin] = useState<boolean>(false);
    const [isBlockingSlot, setIsBlockingSlot] = useState<boolean>(false);
    const [isRotateConfirmOpen, setIsRotateConfirmOpen] = useState<boolean>(false);
    const [isRotatingKey, setIsRotatingKey] = useState<boolean>(false);
    const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState<boolean>(false);
    const [isDeletingAdmin, setIsDeletingAdmin] = useState<boolean>(false);
    const [copiedUrl, setCopiedUrl] = useState<string | null>(null);

    // Load initial data
    const loadData = useCallback(async () => {
        setIsLoading(true);
        setError(null);
        try {
            const [status, discovered, settings] = await Promise.all([
                marketService.getDropAdminStatus().catch(() => null),
                marketService.discoverAvailableDrops().catch(() => []),
                settingsService.getSettings().catch(() => null),
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
                setAdminMaxStorageMb(status.maxTotalStorageMb || 50);
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

            if (settings?.publicDrops) {
                setConfiguredPublicDrops(settings.publicDrops);
            }
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsLoading(false);
        }
    }, [t]);

    useEffect(() => {
        if (isOpen) {
            setActiveTab(initialTab);
            loadData();
            setStatusMsg(null);
            setLastSyncResult(null);
            setWizardStep(1);
            setSetupDraft(null);
            setTestResult(null);
            setProbedPublicDrop(null);
        }
    }, [isOpen, initialTab, loadData]);

    const handleCopy = async (text: string) => {
        const ok = await copyToClipboard(text);
        if (ok) {
            setCopiedUrl(text);
            setTimeout(() => setCopiedUrl(null), 2000);
        }
    };

    // --- Tab 1 Actions: Friends' Drops ---

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
            notifyDropStatusUpdated();
            const discovered = await marketService.discoverAvailableDrops().catch(() => []);
            setDiscoveredDrops(discovered);
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsSyncing(false);
        }
    };

    const [isDropBackupSyncing, setIsDropBackupSyncing] = useState(false);

    const handleSyncDropBackup = async () => {
        setIsDropBackupSyncing(true);
        setError(null);
        setStatusMsg(null);
        try {
            const res = await backupService.syncDropBackup();
            if (res.uploadedTo > 0) {
                setStatusMsg({
                    type: 'success',
                    message: t('settings.dropBackupSuccess', `Sicherung erfolgreich in ${res.uploadedTo} Ablage(n) hinterlegt (${Math.round(res.paddedBytes / 1024)} KB).`),
                });
            } else if (res.authorizedTargets === 0) {
                setStatusMsg({
                    type: 'success',
                    message: t('settings.noDropAuthorizedBackup', 'Keine verbundene Ablage hat den Backup-Kanal freigegeben.'),
                });
            } else {
                setError(t('settings.dropBackupFailed', 'Sicherung in Ablage fehlgeschlagen: ' + res.errors.join(', ')));
            }
            notifyDropStatusUpdated();
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsDropBackupSyncing(false);
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
            notifyDropStatusUpdated();
            await loadData();
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsSavingTargets(false);
        }
    };

    // --- Tab 2 Actions: Public Drop with Registration ---

    const handleProbePublic = async () => {
        if (!publicDropInputUrl.trim()) return;
        setIsProbingPublic(true);
        setError(null);
        try {
            const info = await marketService.probePublicDrop(publicDropInputUrl.trim());
            setProbedPublicDrop(info);
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsProbingPublic(false);
        }
    };

    const handleCheckPublicAuth = async () => {
        if (!probedPublicDrop) return;
        setIsCheckingAuth(true);
        setError(null);
        try {
            const info = await marketService.probePublicDrop(probedPublicDrop.writeUrl);
            setProbedPublicDrop(info);
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsCheckingAuth(false);
        }
    };

    const handleSavePublicDrop = async () => {
        if (!probedPublicDrop) return;
        setIsSavingPublic(true);
        setError(null);
        try {
            await marketService.savePublicDrop({
                writeUrl: probedPublicDrop.writeUrl,
                readUrl: probedPublicDrop.readUrl,
                dropPub: probedPublicDrop.dropPub,
                registerUrl: probedPublicDrop.registerUrl,
                enabled: true,
            });
            setStatusMsg({ type: 'success', message: t('marketplace.publicDropSavedSuccess') });
            setProbedPublicDrop(null);
            setPublicDropInputUrl('');
            notifyDropStatusUpdated();
            await loadData();
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsSavingPublic(false);
        }
    };

    const handleTogglePublicDropEnabled = async (drop: StoredPublicDrop) => {
        try {
            await marketService.savePublicDrop({
                writeUrl: drop.writeUrl,
                readUrl: drop.readUrl,
                dropPub: drop.dropPub,
                registerUrl: drop.registerUrl,
                enabled: !drop.enabled,
            });
            notifyDropStatusUpdated();
            await loadData();
        } catch (err) {
            setError(translateError(err, t));
        }
    };

    const handleConfirmRemovePublicDrop = async () => {
        if (!deletePublicTarget) return;
        setIsDeletingPublic(true);
        try {
            await marketService.removePublicDrop(deletePublicTarget.writeUrl, deleteRemoteSlots);
            setDeletePublicTarget(null);
            setDeleteRemoteSlots(false);
            setStatusMsg({ type: 'success', message: t('marketplace.publicDropRemovedSuccess') });
            notifyDropStatusUpdated();
            await loadData();
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsDeletingPublic(false);
        }
    };

    // --- Tab 3 Actions: Own Drop & Wizard ---

    const handlePrepareSetup = async () => {
        setIsCreatingDraft(true);
        setError(null);
        try {
            const draft = await marketService.prepareDropSetup({
                policy: wizardPolicy,
                maxMembers: wizardMaxMembers,
                maxTotalStorageMb: wizardMaxStorageMb,
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
            notifyDropStatusUpdated();
            await loadData();
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsFinalizing(false);
        }
    };

    const handleUpdateAdminConfig = async () => {
        setIsUpdatingAdmin(true);
        setError(null);
        setStatusMsg(null);
        try {
            const status = await marketService.updateDropAdminConfig(
                {
                    cardPolicy: adminPolicy,
                    maxMembers: adminMaxMembers,
                    maxTotalStorageMb: adminMaxStorageMb,
                    allow: adminAllow,
                    deny: adminDeny,
                },
                true
            );
            setAdminStatus(status);
            setStatusMsg({ type: 'success', message: t('marketplace.cardSavedSuccess') });
            notifyDropStatusUpdated();
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsUpdatingAdmin(false);
        }
    };

    const handleRotateKey = async () => {
        setIsRotatingKey(true);
        setError(null);
        try {
            const status = await marketService.rotateDropAdminKey();
            setAdminStatus(status);
            setIsRotateConfirmOpen(false);
            setStatusMsg({ type: 'success', message: t('marketplace.rotateKeySuccess') });
            notifyDropStatusUpdated();
            await loadData();
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsRotatingKey(false);
        }
    };

    const handleBlockSlot = async (pubkeyHex64: string) => {
        if (!pubkeyHex64.trim()) return;
        setIsBlockingSlot(true);
        setError(null);
        try {
            const status = await marketService.blockDropSlot(pubkeyHex64.trim());
            setAdminStatus(status);
            setManualBlockPubkey('');
            setStatusMsg({ type: 'success', message: t('marketplace.cardSavedSuccess') });
            notifyDropStatusUpdated();
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsBlockingSlot(false);
        }
    };

    const handleUnblockSlot = async (pubkeyHex64: string) => {
        if (!adminStatus) return;
        setIsUpdatingAdmin(true);
        setError(null);
        try {
            const updatedBlocked = adminStatus.blockedPubkeys.filter((id) => id !== pubkeyHex64);
            const status = await marketService.updateDropAdminConfig(
                { blockedPubkeys: updatedBlocked },
                true
            );
            setAdminStatus(status);
            notifyDropStatusUpdated();
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
            notifyDropStatusUpdated();
            await loadData();
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsDeletingAdmin(false);
        }
    };

    const tabs: SegmentedTabItem<DropModalTab>[] = [
        {
            id: 'friend_drops',
            label: t('marketplace.tabFriendDrops', "Ablagen von Freunden"),
            icon: <Users size={14} />,
        },
        {
            id: 'public_drops',
            label: t('marketplace.tabPublicDrops', "Öffentliche Ablage"),
            icon: <Globe size={14} />,
        },
        {
            id: 'own_drop',
            label: t('marketplace.tabOwnDrop', "Eigene Ablage betreiben"),
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
                    {/* 3-Section Tabs Navigation */}
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

                    {/* SECTION 1: Ablagen von Freunden */}
                    {activeTab === 'friend_drops' && (
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
                                        <Button
                                            size="sm"
                                            variant="outline"
                                            disabled={isDropBackupSyncing}
                                            onClick={handleSyncDropBackup}
                                            className="text-xs gap-1.5"
                                            title={t('settings.syncDropNowButton', 'Jetzt in Ablage sichern')}
                                        >
                                            <HardDrive size={13} className={isDropBackupSyncing ? 'animate-spin' : ''} />
                                            <span>{isDropBackupSyncing ? t('settings.syncing', 'Sichere...') : t('settings.syncDropNowButton', 'Jetzt in Ablage sichern')}</span>
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
                                            <div className="grid grid-cols-2 sm:grid-cols-5 gap-2">
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
                                                <div className="p-2.5 bg-indigo-50 rounded-xl border border-indigo-200 text-center">
                                                    <div className="text-base font-extrabold text-indigo-800">
                                                        {lastSyncResult.importedWotLists}
                                                    </div>
                                                    <div className="text-[10px] text-indigo-700">
                                                        {t('marketplace.syncImportedWotLists', { count: lastSyncResult.importedWotLists })}
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

                    {/* SECTION 2: Öffentliche Ablage (mit Registrierung) */}
                    {activeTab === 'public_drops' && (
                        <div className="space-y-4">
                            {/* Probe & Setup New Public Drop */}
                            <Card className="p-4 space-y-3.5">
                                <div>
                                    <h3 className="text-xs font-bold text-theme-secondary uppercase tracking-wider">
                                        {t('marketplace.publicDropTitle', "Öffentliche Ablage hinzufügen")}
                                    </h3>
                                    <p className="text-[11px] text-theme-light mt-0.5">
                                        {t('marketplace.publicDropSubtitle', "Verbinde dich mit einer öffentlichen Ablage über einen anonymen Schlüssel")}
                                    </p>
                                </div>

                                <div className="space-y-2">
                                    <label className="block text-[11px] font-bold text-theme-secondary">
                                        {t('marketplace.publicDropUrlLabel', "Anbieter-Adresse oder URL")}
                                    </label>
                                    <div className="flex gap-2">
                                        <Input
                                            type="text"
                                            placeholder={t('marketplace.publicDropUrlPlaceholder', "z. B. ablage.verein.de oder https://ablage.verein.de")}
                                            value={publicDropInputUrl}
                                            onChange={(e) => setPublicDropInputUrl(e.target.value)}
                                            className="text-xs"
                                        />
                                        <Button
                                            size="sm"
                                            disabled={!publicDropInputUrl.trim() || isProbingPublic}
                                            onClick={handleProbePublic}
                                            className="text-xs gap-1.5 shrink-0"
                                        >
                                            <RefreshCw size={13} className={isProbingPublic ? 'animate-spin' : ''} />
                                            <span>{isProbingPublic ? t('marketplace.probingDrop', "Prüfe...") : t('marketplace.probePublicDropBtn', "Ablage prüfen")}</span>
                                        </Button>
                                    </div>
                                </div>

                                {probedPublicDrop && (
                                    <div className="p-3.5 bg-bg-app rounded-xl border border-theme-subtle space-y-3 animate-in fade-in duration-150">
                                        <div className="flex items-center justify-between">
                                            <span className="text-xs font-bold text-theme-primary">
                                                {probedPublicDrop.writeUrl}
                                            </span>
                                            <div className="flex items-center gap-1.5">
                                                {['card', 'wot', 'backup', 'post'].map((ch) => {
                                                    const isAuth = probedPublicDrop.authorizedChannels.includes(ch);
                                                    return (
                                                        <span
                                                            key={ch}
                                                            className={`px-1.5 py-0.5 rounded text-[9px] font-bold uppercase ${
                                                                isAuth
                                                                    ? 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950/60 dark:text-emerald-300'
                                                                    : 'bg-slate-200 text-slate-600 dark:bg-slate-800 dark:text-slate-400'
                                                            }`}
                                                        >
                                                            {ch}
                                                        </span>
                                                    );
                                                })}
                                            </div>
                                        </div>

                                        {/* Anonymous Access Key R */}
                                        <div className="space-y-1">
                                            <span className="text-[10px] font-bold text-theme-light uppercase tracking-wider block">
                                                {t('marketplace.anonymousAccessKeyLabel', "Dein anonymer Zugangs-Schlüssel (R)")}
                                            </span>
                                            <div className="flex items-center justify-between p-2 bg-card rounded-lg border border-theme-subtle font-mono text-[11px] text-theme-secondary">
                                                <span className="truncate pr-2 select-all">{probedPublicDrop.accessPubkey}</span>
                                                <button
                                                    type="button"
                                                    onClick={() => handleCopy(probedPublicDrop.accessPubkey)}
                                                    className="p-1 text-theme-light hover:text-theme-primary transition-colors shrink-0"
                                                    title={t('common.copy', "Kopieren")}
                                                >
                                                    {copiedUrl === probedPublicDrop.accessPubkey ? (
                                                        <Check size={14} className="text-emerald-600" />
                                                    ) : (
                                                        <Copy size={14} />
                                                    )}
                                                </button>
                                            </div>
                                        </div>

                                        {/* Actions */}
                                        <div className="flex flex-wrap items-center justify-between gap-2 pt-1 border-t border-theme-subtle/50">
                                            <div className="flex items-center gap-2">
                                                {probedPublicDrop.registerUrl && (
                                                    <Button
                                                        size="sm"
                                                        variant="secondary"
                                                        onClick={() => openUrl(probedPublicDrop.registerUrl!)}
                                                        className="text-xs gap-1.5"
                                                    >
                                                        <ExternalLink size={13} />
                                                        <span>{t('marketplace.goToRegisterBtn', "Zur Registrierung beim Anbieter")}</span>
                                                    </Button>
                                                )}
                                                <Button
                                                    size="sm"
                                                    variant="secondary"
                                                    disabled={isCheckingAuth}
                                                    onClick={handleCheckPublicAuth}
                                                    className="text-xs gap-1.5"
                                                >
                                                    <RefreshCw size={13} className={isCheckingAuth ? 'animate-spin' : ''} />
                                                    <span>{t('marketplace.checkAuthorizationBtn', "Freischaltung prüfen")}</span>
                                                </Button>
                                            </div>

                                            <Button
                                                size="sm"
                                                variant="primary"
                                                disabled={isSavingPublic}
                                                onClick={handleSavePublicDrop}
                                                className="text-xs gap-1.5"
                                            >
                                                <CheckCircle2 size={13} />
                                                <span>{t('marketplace.savePublicDropBtn', "Ablage speichern")}</span>
                                            </Button>
                                        </div>
                                    </div>
                                )}
                            </Card>

                            {/* Configured Public Drops */}
                            <Card className="p-4 space-y-3">
                                <h3 className="text-xs font-bold text-theme-secondary uppercase tracking-wider">
                                    {t('marketplace.savedPublicDropsTitle', "Gespeicherte öffentliche Ablagen")}
                                </h3>

                                {configuredPublicDrops.length === 0 ? (
                                    <div className="p-6 text-center text-xs text-theme-light italic bg-bg-app rounded-xl border border-dashed border-theme-subtle">
                                        {t('marketplace.noPublicDropsConfigured', "Noch keine öffentlichen Ablagen hinterlegt.")}
                                    </div>
                                ) : (
                                    <div className="space-y-2">
                                        {configuredPublicDrops.map((drop) => (
                                            <div
                                                key={drop.writeUrl}
                                                className="p-3 bg-white rounded-xl border border-theme-subtle/70 flex flex-col sm:flex-row sm:items-center justify-between gap-2.5"
                                            >
                                                <div className="min-w-0 space-y-1">
                                                    <div className="flex items-center gap-2">
                                                        <span className="text-xs font-bold text-theme-secondary truncate">
                                                            {drop.writeUrl}
                                                        </span>
                                                        <span className="flex items-center gap-1">
                                                            {['card', 'wot', 'backup', 'post'].map((ch) => {
                                                                const isAuth = drop.authorizedChannels.includes(ch);
                                                                return (
                                                                    <span
                                                                        key={ch}
                                                                        className={`px-1 py-0.2 rounded text-[8px] font-bold uppercase ${
                                                                            isAuth
                                                                                ? 'bg-emerald-100 text-emerald-800'
                                                                                : 'bg-slate-100 text-slate-500'
                                                                        }`}
                                                                    >
                                                                        {ch}
                                                                    </span>
                                                                );
                                                            })}
                                                        </span>
                                                    </div>
                                                    <p className="text-[10px] font-mono text-theme-light truncate">
                                                        Schlüssel: {drop.accessPubkey.substring(0, 16)}...
                                                    </p>
                                                </div>

                                                <div className="flex items-center gap-2 shrink-0 self-end sm:self-center">
                                                    <label className="flex items-center gap-1.5 text-xs text-theme-secondary cursor-pointer mr-1">
                                                        <input
                                                            type="checkbox"
                                                            checked={drop.enabled}
                                                            onChange={() => handleTogglePublicDropEnabled(drop)}
                                                            className="h-3.5 w-3.5 rounded accent-theme-primary cursor-pointer"
                                                        />
                                                        <span className="text-[11px] font-medium">Aktiv</span>
                                                    </label>

                                                    <Button
                                                        size="sm"
                                                        variant="danger"
                                                        onClick={() => setDeletePublicTarget(drop)}
                                                        className="text-[10px] p-1.5"
                                                        title={t('common.remove', "Entfernen")}
                                                    >
                                                        <Trash2 size={12} />
                                                    </Button>
                                                </div>
                                            </div>
                                        ))}
                                    </div>
                                )}
                            </Card>
                        </div>
                    )}

                    {/* SECTION 3: Eigene Ablage betreiben */}
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

                                                <div>
                                                    <label className="block text-[11px] font-bold text-theme-secondary mb-1">
                                                        {t('marketplace.maxTotalStorageMbLabel', "Max. Gesamtspeicher (MB)")}
                                                    </label>
                                                    <Input
                                                        type="number"
                                                        value={wizardMaxStorageMb}
                                                        onChange={(e) => setWizardMaxStorageMb(Math.max(1, Number(e.target.value)))}
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

                                    {/* Storage Quota & Soft Warning */}
                                    <Card className="p-4 space-y-2.5">
                                        <div className="flex items-center justify-between">
                                            <div className="flex items-center gap-2">
                                                <HardDrive size={15} className="text-theme-primary" />
                                                <h3 className="text-xs font-bold text-theme-secondary uppercase tracking-wider">
                                                    {t('marketplace.dropStorageLabel', "Speicher-Kontingent")}
                                                </h3>
                                            </div>
                                            <span className="text-xs font-semibold text-theme-secondary">
                                                {adminStatus.maxTotalStorageMb} MB
                                            </span>
                                        </div>

                                        {adminStatus.softWarning && (
                                            <div className="p-2.5 bg-amber-50 dark:bg-amber-950/40 border border-amber-200 dark:border-amber-800/60 rounded-xl text-amber-800 dark:text-amber-200 text-xs flex items-start gap-2">
                                                <AlertTriangle size={15} className="shrink-0 mt-0.5 text-amber-600" />
                                                <span>{t('marketplace.dropStorageWarning', "Warnung: Der Speicherplatz deiner Ablage ist zu mindestens 80% belegt!")}</span>
                                            </div>
                                        )}

                                        {adminStatus.usedBytes != null && adminStatus.maxBytes != null && (
                                            <div className="space-y-1">
                                                <div className="w-full bg-bg-app rounded-full h-2 border border-theme-subtle overflow-hidden">
                                                    <div
                                                        className={`h-full transition-all ${
                                                            adminStatus.softWarning ? 'bg-amber-500' : 'bg-theme-primary'
                                                        }`}
                                                        style={{
                                                            width: `${Math.min(100, Math.round((adminStatus.usedBytes / adminStatus.maxBytes) * 100))}%`,
                                                        }}
                                                    />
                                                </div>
                                                <div className="flex justify-between text-[10px] text-theme-light font-mono">
                                                    <span>{(adminStatus.usedBytes / (1024 * 1024)).toFixed(1)} MB</span>
                                                    <span>{(adminStatus.maxBytes / (1024 * 1024)).toFixed(1)} MB ({Math.round((adminStatus.usedBytes / adminStatus.maxBytes) * 100)}%)</span>
                                                </div>
                                            </div>
                                        )}
                                    </Card>

                                    {/* 180-Day Key Rotation */}
                                    <Card className="p-4 space-y-3">
                                        <div className="flex items-center justify-between">
                                            <div className="flex items-center gap-2">
                                                <KeyRound size={15} className="text-theme-primary" />
                                                <h3 className="text-xs font-bold text-theme-secondary uppercase tracking-wider">
                                                    {t('marketplace.rotationTitle', "Admin-Schlüsselrotation")}
                                                </h3>
                                            </div>
                                            <span className="text-[10px] font-bold px-2 py-0.5 rounded-full bg-theme-primary/10 text-theme-primary">
                                                {t('marketplace.generationLabel', "Generation")} {adminStatus.generation}
                                            </span>
                                        </div>

                                        <div className="p-2 bg-bg-app rounded-lg border border-theme-subtle/60 text-xs space-y-1">
                                            <span className="text-[10px] font-bold text-theme-light block">
                                                {t('marketplace.adminPubkeyLabel', "Admin-Public-Key (Ed25519)")}:
                                            </span>
                                            <div className="flex items-center justify-between font-mono text-[11px] text-theme-secondary">
                                                <span className="truncate pr-2">{adminStatus.adminPubkey}</span>
                                                <button
                                                    type="button"
                                                    onClick={() => handleCopy(adminStatus.adminPubkey)}
                                                    className="p-1 text-theme-light hover:text-theme-primary"
                                                >
                                                    {copiedUrl === adminStatus.adminPubkey ? <Check size={13} className="text-emerald-600" /> : <Copy size={13} />}
                                                </button>
                                            </div>
                                        </div>

                                        {adminStatus.rotationRetireAt && (
                                            <div className="p-2 rounded-lg bg-blue-50 dark:bg-blue-950/40 border border-blue-200 dark:border-blue-800/60 text-blue-800 dark:text-blue-200 text-xs flex items-center gap-1.5">
                                                <Shield size={14} className="text-blue-600 shrink-0" />
                                                <span>
                                                    {t('marketplace.rotationGraceActive', "Schlüsselrotation aktiv (Übergangsfrist läuft bis: {{date}})", {
                                                        date: new Date(adminStatus.rotationRetireAt * 1000).toLocaleDateString(),
                                                    })}
                                                </span>
                                            </div>
                                        )}

                                        <div className="flex justify-end pt-1">
                                            <Button
                                                size="sm"
                                                variant="secondary"
                                                onClick={() => setIsRotateConfirmOpen(true)}
                                                className="text-xs gap-1.5"
                                            >
                                                <RefreshCw size={13} />
                                                <span>{t('marketplace.rotateKeyBtn', "Admin-Schlüssel rotieren")}</span>
                                            </Button>
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

                                            <div>
                                                <label className="block text-[11px] font-bold text-theme-secondary mb-1">
                                                    {t('marketplace.maxTotalStorageMbLabel', "Max. Gesamtspeicher (MB)")}
                                                </label>
                                                <Input
                                                    type="number"
                                                    value={adminMaxStorageMb}
                                                    onChange={(e) => setAdminMaxStorageMb(Math.max(1, Number(e.target.value)))}
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

                                        {/* Manual 64-hex Pubkey blocking */}
                                        <div className="flex gap-2">
                                            <Input
                                                type="text"
                                                placeholder={t('marketplace.manualPubkeyHexPlaceholder', "64-stelliger Hex Ed25519 Public Key...")}
                                                value={manualBlockPubkey}
                                                onChange={(e) => setManualBlockPubkey(e.target.value.toLowerCase().trim())}
                                                className="text-xs font-mono"
                                            />
                                            <Button
                                                size="sm"
                                                variant="danger"
                                                disabled={!manualBlockPubkey.trim() || isBlockingSlot}
                                                onClick={() => handleBlockSlot(manualBlockPubkey)}
                                                className="text-xs gap-1 shrink-0"
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
                                        {adminStatus.blockedPubkeys.length > 0 && (
                                            <div className="space-y-1.5 pt-1">
                                                <span className="text-[10px] font-bold text-rose-600 uppercase tracking-wider block">
                                                    {t('marketplace.blockedSlotsLabel')} ({adminStatus.blockedPubkeys.length})
                                                </span>
                                                <div className="max-h-28 overflow-y-auto space-y-1 p-1 bg-rose-50/50 rounded-xl border border-rose-200/60 text-[11px] font-mono">
                                                    {adminStatus.blockedPubkeys.map((pubkey) => (
                                                        <div
                                                            key={pubkey}
                                                            className="flex items-center justify-between p-1.5 rounded-lg bg-white border border-rose-200/50"
                                                        >
                                                            <span className="truncate pr-2 text-rose-900">{pubkey}</span>
                                                            <button
                                                                type="button"
                                                                onClick={() => handleUnblockSlot(pubkey)}
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

            {/* Key Rotation Confirmation Modal */}
            <ConfirmationModal
                isOpen={isRotateConfirmOpen}
                title={t('marketplace.rotateKeyConfirmTitle', "Admin-Schlüssel jetzt rotieren?")}
                description={t('marketplace.rotateKeyConfirmDesc', "Ein neues Ed25519-Schlüsselpaar wird erzeugt. Während der 180-tägigen Übergangsfrist akzeptiert die Ablage sowohl den alten als auch den neuen Schlüssel. Fortfahren?")}
                confirmVariant="primary"
                confirmText={t('marketplace.rotateKeyBtn', "Rotieren")}
                onConfirm={handleRotateKey}
                onCancel={() => setIsRotateConfirmOpen(false)}
                isProcessing={isRotatingKey}
            />

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

            {/* Remove Public Drop Confirmation Modal */}
            <ConfirmationModal
                isOpen={!!deletePublicTarget}
                title={t('marketplace.removePublicDropConfirmTitle', "Öffentliche Ablage entfernen?")}
                description={
                    <div className="space-y-3">
                        <p>{t('marketplace.removePublicDropConfirmDesc', "Möchtest du diese öffentliche Ablage aus deinen Einstellungen entfernen?")}</p>
                        <label className="flex items-center gap-2 p-2 bg-bg-app rounded-lg border border-theme-subtle text-xs cursor-pointer">
                            <input
                                type="checkbox"
                                checked={deleteRemoteSlots}
                                onChange={(e) => setDeleteRemoteSlots(e.target.checked)}
                                className="h-4 w-4 rounded accent-theme-primary cursor-pointer"
                            />
                            <span>{t('marketplace.deleteRemoteSlotsLabel', "Auch meine Daten auf der Ablage löschen")}</span>
                        </label>
                    </div>
                }
                confirmVariant="danger"
                confirmText={t('common.remove')}
                onConfirm={handleConfirmRemovePublicDrop}
                onCancel={() => {
                    setDeletePublicTarget(null);
                    setDeleteRemoteSlots(false);
                }}
                isProcessing={isDeletingPublic}
            />
        </>
    );
}

export default DropManagementModal;
