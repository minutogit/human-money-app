// src/components/settings/SyncServiceSettingsCard.tsx
import React, { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import {
    Cloud,
    Server,
    Users,
    Globe,
    RefreshCw,
    CheckCircle2,
    AlertTriangle,
    Trash2,
    Copy,
    Check,
    Shield,
    HardDrive,
    KeyRound,
    FileArchive,
    HelpCircle,
} from 'lucide-react';
import { save } from '@tauri-apps/plugin-dialog';
import {
    AppSettings,
    FrontendDiscoveredDrop,
    FrontendDropAdminStatus,
    FrontendMemberPolicyDto,
    StoredPublicDrop,
} from '../../types';
import { marketService } from '../../services/marketService';
import { settingsService } from '../../services/settingsService';
import { useSession } from '../../context/SessionContext';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { SegmentedTabs } from '../ui/SegmentedTabs';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { copyToClipboard } from '../../utils/clipboard';
import { notifyDropStatusUpdated } from '../../utils/dropEvents';
import { translateError } from '../../utils/errorHelper';

interface PolicyHelpPopoverProps {
    title: string;
    content: string;
}

const PolicyHelpPopover: React.FC<PolicyHelpPopoverProps> = ({ title, content }) => {
    const [isOpen, setIsOpen] = useState(false);
    return (
        <span className="relative inline-flex items-center ml-1">
            <button
                type="button"
                onClick={() => setIsOpen(!isOpen)}
                onMouseEnter={() => setIsOpen(true)}
                onMouseLeave={() => setIsOpen(false)}
                className="text-theme-light hover:text-theme-primary transition-colors focus:outline-none p-0.5 rounded-full hover:bg-theme-primary/5 cursor-pointer"
                title={title}
                aria-label={title}
            >
                <HelpCircle size={14} />
            </button>
            {isOpen && (
                <span className="absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 w-64 p-2.5 bg-slate-900 text-white text-[11px] font-normal leading-relaxed rounded-xl shadow-lg z-30 pointer-events-none animate-in fade-in zoom-in-95 duration-150">
                    <span className="font-bold text-slate-100 block mb-0.5">{title}</span>
                    <span className="opacity-90 block">{content}</span>
                </span>
            )}
        </span>
    );
};

interface PolicyEditorProps {
    policy: FrontendMemberPolicyDto;
    onChange: (updater: (prev: FrontendMemberPolicyDto) => FrontendMemberPolicyDto) => void;
    idPrefix: string;
}

const PolicyEditor: React.FC<PolicyEditorProps> = ({ policy, onChange, idPrefix }) => {
    const { t } = useTranslation();

    return (
        <div className="space-y-4 text-xs">
            {/* Direct Contacts Selection */}
            <div>
                <label className="font-bold text-theme-secondary block mb-1.5">
                    {t('settings.syncHostDirectMinLabel', 'Welche deiner Kontakte dürfen den Dienst nutzen?')}
                </label>
                <div className="space-y-1.5">
                    <label className="flex items-center gap-2 cursor-pointer">
                        <input
                            type="radio"
                            name={`${idPrefix}-directMin`}
                            value={2}
                            checked={policy.directMin === 2}
                            onChange={() => onChange(p => ({ ...p, directMin: 2 }))}
                            className="text-theme-primary focus:ring-theme-primary"
                        />
                        <span className="text-theme-secondary font-medium">
                            {t('settings.syncHostDirectMinLevel2', 'Ab Vertrauensstufe „Vertraut“ (Stufe 2 – empfohlen)')}
                        </span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                        <input
                            type="radio"
                            name={`${idPrefix}-directMin`}
                            value={1}
                            checked={policy.directMin === 1}
                            onChange={() => onChange(p => ({ ...p, directMin: 1 }))}
                            className="text-theme-primary focus:ring-theme-primary"
                        />
                        <span className="text-theme-secondary font-medium">
                            {t('settings.syncHostDirectMinLevel1', 'Ab Vertrauensstufe „Bekannt“ (Stufe 1)')}
                        </span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                        <input
                            type="radio"
                            name={`${idPrefix}-directMin`}
                            value={3}
                            checked={policy.directMin === 3}
                            onChange={() => onChange(p => ({ ...p, directMin: 3 }))}
                            className="text-theme-primary focus:ring-theme-primary"
                        />
                        <span className="text-theme-secondary font-medium">
                            {t('settings.syncHostDirectMinLevel3', 'Nur „Engster Kreis“ (Stufe 3)')}
                        </span>
                    </label>
                </div>
            </div>

            {/* Hop-2 Friends of Friends */}
            <div className="pt-2 border-t border-theme-subtle/50 space-y-3">
                <label className="flex items-center gap-2 cursor-pointer">
                    <input
                        type="checkbox"
                        checked={policy.hop2Enabled}
                        onChange={(e) => onChange(p => ({ ...p, hop2Enabled: e.target.checked }))}
                        className="rounded text-theme-primary focus:ring-theme-primary"
                    />
                    <span className="font-bold text-theme-secondary">
                        {t('settings.syncHostHop2Label', 'Freunden erlauben, ihren eigenen vertrauten Kreis mitzubringen')}
                    </span>
                </label>

                {policy.hop2Enabled && (
                    <div className="pl-6 space-y-3 pt-1 animate-in fade-in duration-150">
                        {/* bridgeMin selection */}
                        <div className="space-y-1">
                            <label className="font-medium text-theme-secondary flex items-center">
                                <span>{t('settings.syncHostBridgeMinLabel', 'Welche deiner Freunde dürfen Personen mitbringen?')}</span>
                                <PolicyHelpPopover
                                    title={t('settings.syncHostBridgeMinTitle', 'Eigene Freunde (Hop 1)')}
                                    content={t('settings.syncHostBridgeMinDesc', 'Nur Freunde, denen du mindestens mit dieser Stufe vertraust (z. B. Alice), dürfen den Dienst für ihren Kreis freigeben.')}
                                />
                            </label>
                            <select
                                value={policy.bridgeMin}
                                onChange={(e) => {
                                    const val = Number(e.target.value);
                                    onChange(p => ({ ...p, bridgeMin: val }));
                                }}
                                className="w-full sm:w-64 rounded-xl border border-theme-subtle bg-bg-app px-2.5 py-1.5 text-xs text-theme-secondary focus:outline-none focus:ring-1 focus:ring-theme-primary"
                            >
                                <option value={1}>{t('settings.trustLevel1Short', 'Stufe 1 (Bekannt)')}</option>
                                <option value={2}>{t('settings.trustLevel2Short', 'Stufe 2 (Vertraut)')}</option>
                                <option value={3}>{t('settings.trustLevel3Short', 'Stufe 3 (Engster Kreis)')}</option>
                            </select>
                        </div>

                        {/* hop2Min selection */}
                        <div className="space-y-1">
                            <label className="font-medium text-theme-secondary flex items-center">
                                <span>{t('settings.syncHostHop2MinLabel', 'Wie gut muss dein Freund die Person kennen?')}</span>
                                <PolicyHelpPopover
                                    title={t('settings.syncHostHop2MinTitle', 'Freunde von Freunden (Hop 2)')}
                                    content={t('settings.syncHostHop2MinDesc', 'Alice muss Bob mindestens mit dieser Stufe vertrauen, damit Bob deinen Dienst nutzen darf.')}
                                />
                            </label>
                            <select
                                value={policy.hop2Min}
                                onChange={(e) => {
                                    const val = Number(e.target.value);
                                    onChange(p => ({ ...p, hop2Min: val }));
                                }}
                                className="w-full sm:w-64 rounded-xl border border-theme-subtle bg-bg-app px-2.5 py-1.5 text-xs text-theme-secondary focus:outline-none focus:ring-1 focus:ring-theme-primary"
                            >
                                <option value={1}>{t('settings.trustLevel1Short', 'Stufe 1 (Bekannt)')}</option>
                                <option value={2}>{t('settings.trustLevel2Short', 'Stufe 2 (Vertraut)')}</option>
                                <option value={3}>{t('settings.trustLevel3Short', 'Stufe 3 (Engster Kreis)')}</option>
                            </select>
                        </div>
                    </div>
                )}
            </div>
        </div>
    );
};

interface SyncServiceSettingsCardProps {
    settings: AppSettings | null;
    onSettingsUpdated?: (newSettings: AppSettings) => void;
}

export const SyncServiceSettingsCard: React.FC<SyncServiceSettingsCardProps> = ({
    settings,
    onSettingsUpdated,
}) => {
    const { t } = useTranslation();
    const { protectAction } = useSession();

    const [activeTab, setActiveTab] = useState<'connected' | 'host'>('connected');
    const [isLoading, setIsLoading] = useState(false);
    const [feedback, setFeedback] = useState<{ type: 'success' | 'error'; message: string } | null>(null);

    // Tab 1 state: Connected services & features
    const [discoveredDrops, setDiscoveredDrops] = useState<FrontendDiscoveredDrop[]>([]);
    const [configuredPublicDrops, setConfiguredPublicDrops] = useState<StoredPublicDrop[]>([]);
    const [inputUrl, setInputUrl] = useState('');
    const [isProbing, setIsProbing] = useState(false);
    const [probeResult, setProbeResult] = useState<{ status: 'success' | 'unauthorized' | 'error'; message: string } | null>(null);

    // Feature toggles
    const [postAcceptFromContacts, setPostAcceptFromContacts] = useState<boolean>(settings?.postAcceptFromContacts ?? true);
    const [marketSendCardOnPayment, setMarketSendCardOnPayment] = useState<boolean>(settings?.marketSendCardOnPayment ?? true);
    const [wotShareViaDrop, setWotShareViaDrop] = useState<boolean>(settings?.wotShareViaDrop ?? true);
    const [dropBackupEnabled, setDropBackupEnabled] = useState<boolean>(settings?.dropBackupEnabled ?? true);

    // Tab 2 state: Host / Own Sync Service
    const [adminStatus, setAdminStatus] = useState<FrontendDropAdminStatus | null>(null);
    const [hostPolicy, setHostPolicy] = useState<FrontendMemberPolicyDto>({
        directMin: 2,
        hop2Enabled: true,
        bridgeMin: 2,
        hop2Min: 2,
    });
    const hostMaxMembers = 300;
    const hostMaxStorageMb = 50;
    const [hostWebspaceUrl, setHostWebspaceUrl] = useState('');
    const [isDownloadingZip, setIsDownloadingZip] = useState(false);
    const [isTestingHost, setIsTestingHost] = useState(false);
    const [isUpdatingPolicy, setIsUpdatingPolicy] = useState(false);
    const [isRotatingKey, setIsRotatingKey] = useState(false);
    const [isDeletingAdmin, setIsDeletingAdmin] = useState(false);
    const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
    const [copiedLink, setCopiedLink] = useState(false);

    // Synchronize local toggles when settings prop changes
    useEffect(() => {
        if (settings) {
            setPostAcceptFromContacts(settings.postAcceptFromContacts ?? true);
            setMarketSendCardOnPayment(settings.marketSendCardOnPayment ?? true);
            setWotShareViaDrop(settings.wotShareViaDrop ?? true);
            setDropBackupEnabled(settings.dropBackupEnabled ?? true);
            if (Array.isArray(settings.publicDrops)) {
                setConfiguredPublicDrops(settings.publicDrops);
            }
        }
    }, [settings]);

    const loadData = useCallback(async () => {
        setIsLoading(true);
        try {
            const [discovered, admin, s] = await Promise.all([
                marketService.discoverAvailableDrops().catch(() => []),
                marketService.getDropAdminStatus().catch(() => null),
                settingsService.getSettings().catch(() => null),
            ]);
            setDiscoveredDrops(Array.isArray(discovered) ? discovered : []);
            setAdminStatus(admin || null);
            if (admin?.cardPolicy) {
                setHostPolicy(admin.cardPolicy);
            }
            if (Array.isArray(s?.publicDrops)) {
                setConfiguredPublicDrops(s.publicDrops);
            } else if (!settings?.publicDrops) {
                setConfiguredPublicDrops([]);
            }
        } catch {
            // Silently retain last known data
        } finally {
            setIsLoading(false);
        }
    }, [settings]);

    useEffect(() => {
        loadData();
    }, [loadData]);

    const updateFeatureToggle = async (key: keyof AppSettings, value: boolean) => {
        try {
            const current = settings || await settingsService.getSettings();
            const updated: AppSettings = {
                ...current,
                [key]: value,
            };
            await settingsService.saveSettings(updated);
            if (onSettingsUpdated) {
                onSettingsUpdated(updated);
            }
            notifyDropStatusUpdated();
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    // --- Tab 1 Actions ---

    const handleProbeAndAdd = async () => {
        const trimmed = inputUrl.trim();
        if (!trimmed) return;
        setIsProbing(true);
        setProbeResult(null);
        try {
            const info = await marketService.probePublicDrop(trimmed);
            if (info) {
                // Check if key is authorized
                const hasAuth = info.authorizedChannels.length > 0;
                await marketService.savePublicDrop({
                    writeUrl: info.writeUrl,
                    readUrl: info.readUrl,
                    dropPub: info.dropPub,
                    registerUrl: info.registerUrl,
                    enabled: true,
                });
                setInputUrl('');
                const s = await settingsService.getSettings();
                if (s.publicDrops) {
                    setConfiguredPublicDrops(s.publicDrops);
                }
                if (onSettingsUpdated) {
                    onSettingsUpdated(s);
                }
                notifyDropStatusUpdated();

                if (hasAuth) {
                    setProbeResult({
                        status: 'success',
                        message: t('settings.syncProbeSuccess', 'Verbunden & freigeschaltet!'),
                    });
                } else {
                    setProbeResult({
                        status: 'unauthorized',
                        message: t('settings.syncProbeUnauthorized', 'Server erreichbar, aber dein Schlüssel ist noch nicht freigeschaltet. Bitte den Betreiber, dich in seinem Adressbuch als vertrauten Kontakt einzustufen.'),
                    });
                }
            }
        } catch (err) {
            setProbeResult({
                status: 'error',
                message: `${t('settings.syncProbeError', 'Verbindung fehlgeschlagen')}: ${translateError(err, t)}`,
            });
        } finally {
            setIsProbing(false);
        }
    };

    const handleTogglePublicDrop = async (drop: StoredPublicDrop) => {
        try {
            await marketService.savePublicDrop({
                writeUrl: drop.writeUrl,
                readUrl: drop.readUrl,
                dropPub: drop.dropPub,
                registerUrl: drop.registerUrl,
                enabled: !drop.enabled,
            });
            const s = await settingsService.getSettings();
            if (s.publicDrops) {
                setConfiguredPublicDrops(s.publicDrops);
            }
            if (onSettingsUpdated) {
                onSettingsUpdated(s);
            }
            notifyDropStatusUpdated();
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    const handleDeletePublicDrop = async (writeUrl: string) => {
        try {
            await marketService.removePublicDrop(writeUrl, false);
            const s = await settingsService.getSettings();
            if (s.publicDrops) {
                setConfiguredPublicDrops(s.publicDrops);
            }
            if (onSettingsUpdated) {
                onSettingsUpdated(s);
            }
            notifyDropStatusUpdated();
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    const handleToggleDiscoveredDrop = async (discovered: FrontendDiscoveredDrop) => {
        try {
            const currentSelected = discoveredDrops.filter(d => d.isSelected).map(d => d.descriptor);
            const isCurrentlySelected = discovered.isSelected;
            let updatedList = [];
            if (isCurrentlySelected) {
                updatedList = currentSelected.filter(d => d.readUrl !== discovered.descriptor.readUrl);
            } else {
                updatedList = [...currentSelected, discovered.descriptor];
            }
            await marketService.setSelectedUploadDrops(updatedList);
            const refetched = await marketService.discoverAvailableDrops().catch(() => []);
            setDiscoveredDrops(refetched);
            notifyDropStatusUpdated();
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    // --- Tab 2 Actions (Host / Admin) ---

    const handleDownloadZip = async () => {
        setIsDownloadingZip(true);
        setFeedback(null);
        try {
            const filePath = await save({
                defaultPath: 'humoco-sync-setup.zip',
                filters: [{ name: 'ZIP Archive', extensions: ['zip'] }],
            });
            if (filePath) {
                await protectAction(async (pwd) => {
                    await marketService.prepareDropSetup({
                        policy: hostPolicy,
                        maxMembers: hostMaxMembers,
                        maxTotalStorageMb: hostMaxStorageMb,
                    }, pwd || undefined);
                    await marketService.saveDropSetupZip(filePath, pwd || undefined);
                });
                setFeedback({
                    type: 'success',
                    message: t('settings.syncZipSaved', 'Installationsarchiv erfolgreich gespeichert! Entpacke die Datei auf deinem Webspace.'),
                });
            }
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsDownloadingZip(false);
        }
    };

    const handleTestAndActivateHost = async () => {
        const trimmed = hostWebspaceUrl.trim();
        if (!trimmed) return;
        setIsTestingHost(true);
        setFeedback(null);
        try {
            await protectAction(async (pwd) => {
                const testRes = await marketService.testDropConnection(trimmed, pwd || undefined);
                if (!testRes.success || !testRes.writeOk || !testRes.readOk) {
                    throw new Error(testRes.message || 'Verbindung zum Webspace konnte nicht verifiziert werden.');
                }
                const admin = await marketService.finalizeDropSetup(trimmed, pwd || undefined);
                setAdminStatus(admin);
                notifyDropStatusUpdated();
                setFeedback({
                    type: 'success',
                    message: t('settings.syncHostActivatedSuccess', 'Dein Sync-Dienst wurde erfolgreich aktiviert und verifiziert!'),
                });
            });
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsTestingHost(false);
        }
    };

    const handleUpdateHostPolicy = async () => {
        setIsUpdatingPolicy(true);
        setFeedback(null);
        try {
            await protectAction(async (pwd) => {
                const updated = await marketService.updateDropAdminConfig({
                    cardPolicy: hostPolicy,
                    maxMembers: hostMaxMembers,
                    maxTotalStorageMb: hostMaxStorageMb,
                }, true, pwd || undefined);
                setAdminStatus(updated);
                setFeedback({
                    type: 'success',
                    message: t('settings.syncHostPolicyUpdated', 'Zugriffsregeln wurden erfolgreich auf deinem Server aktualisiert!'),
                });
            });
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsUpdatingPolicy(false);
        }
    };

    const handleRotateKey = async () => {
        setIsRotatingKey(true);
        setFeedback(null);
        try {
            await protectAction(async (pwd) => {
                const updated = await marketService.rotateDropAdminKey(pwd || undefined);
                setAdminStatus(updated);
                setFeedback({
                    type: 'success',
                    message: t('settings.syncHostKeyRotated', 'Server-Schlüssel wurde erfolgreich rotiert.'),
                });
            });
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsRotatingKey(false);
        }
    };

    const handleDeleteHost = async () => {
        setIsDeletingAdmin(true);
        try {
            await protectAction(async (pwd) => {
                await marketService.deleteDropAdmin(pwd || undefined);
                setAdminStatus(null);
                setIsDeleteConfirmOpen(false);
                notifyDropStatusUpdated();
                setFeedback({
                    type: 'success',
                    message: t('settings.syncHostDeletedSuccess', 'Dein Sync-Dienst wurde getrennt.'),
                });
            });
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsDeletingAdmin(false);
        }
    };

    const handleCopyFriendLink = async (url: string) => {
        const ok = await copyToClipboard(url);
        if (ok) {
            setCopiedLink(true);
            setTimeout(() => setCopiedLink(false), 2000);
        }
    };

    return (
        <Card
            header={
                <div className="flex items-center justify-between gap-3 w-full">
                    <div className="flex items-center gap-2">
                        <Cloud size={18} className="text-theme-primary" />
                        <span className="font-black text-xs uppercase tracking-widest">
                            {t('settings.syncServiceCardTitle', 'HuMoCo Sync-Dienst')}
                        </span>
                    </div>
                    {isLoading && <RefreshCw size={14} className="animate-spin text-theme-light" />}
                </div>
            }
        >
            <div className="space-y-6">
                <p className="text-xs text-theme-light font-medium">
                    {t('settings.syncServiceCardSubtitle', 'Automatischer Hintergrund-Austausch für Zahlungen, Angebote und Sicherungen.')}
                </p>

                {feedback && (
                    <div className={`p-3 rounded-xl border text-xs flex items-center gap-2 ${
                        feedback.type === 'success'
                            ? 'bg-emerald-50 text-emerald-800 border-emerald-200 dark:bg-emerald-950/40 dark:text-emerald-300 dark:border-emerald-800'
                            : 'bg-rose-50 text-rose-800 border-rose-200 dark:bg-rose-950/40 dark:text-rose-300 dark:border-rose-800'
                    }`}>
                        {feedback.type === 'success' ? <CheckCircle2 size={16} /> : <AlertTriangle size={16} />}
                        <span>{feedback.message}</span>
                    </div>
                )}

                {/* Sub-Tabs: Connected vs Host */}
                <SegmentedTabs
                    activeTab={activeTab}
                    onChange={(id) => setActiveTab(id as 'connected' | 'host')}
                    tabs={[
                        {
                            id: 'connected',
                            label: t('settings.syncTabConnected', 'Verbundene Dienste & Funktionen'),
                            icon: <Users size={16} />,
                        },
                        {
                            id: 'host',
                            label: t('settings.syncTabHost', 'Eigenen Dienst bereitstellen / Admin'),
                            icon: <Server size={16} />,
                        },
                    ]}
                />

                {/* TAB 1: Connected Services & Features */}
                {activeTab === 'connected' && (
                    <div className="space-y-6 animate-in fade-in duration-200">
                        {/* 1. Feature Comfort Toggles (Channels) */}
                        <div className="p-4 bg-bg-app rounded-2xl border border-theme-subtle space-y-3">
                            <h4 className="text-xs font-bold text-theme-secondary uppercase tracking-wider">
                                {t('settings.syncChannelsTitle', 'Hintergrund-Funktionen (Kanäle)')}
                            </h4>

                            <div className="space-y-2.5 pt-1">
                                <label className="flex items-start gap-3 cursor-pointer group">
                                    <input
                                        type="checkbox"
                                        checked={postAcceptFromContacts}
                                        onChange={(e) => {
                                            setPostAcceptFromContacts(e.target.checked);
                                            updateFeatureToggle('postAcceptFromContacts', e.target.checked);
                                        }}
                                        className="mt-0.5 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary"
                                    />
                                    <div>
                                        <span className="text-xs font-bold text-theme-primary group-hover:text-theme-accent">
                                            {t('settings.syncChannelPostLabel', 'Digitales Postfach aktivieren')}
                                        </span>
                                        <p className="text-[11px] text-theme-light mt-0.5">
                                            {t('settings.syncChannelPostDesc', 'Zahlungen, Rechnungen und Bürgschaftsanfragen automatisch im Hintergrund empfangen.')}
                                        </p>
                                    </div>
                                </label>

                                <label className="flex items-start gap-3 cursor-pointer group pt-1 border-t border-theme-subtle/50">
                                    <input
                                        type="checkbox"
                                        checked={marketSendCardOnPayment}
                                        onChange={(e) => {
                                            setMarketSendCardOnPayment(e.target.checked);
                                            updateFeatureToggle('marketSendCardOnPayment', e.target.checked);
                                        }}
                                        className="mt-0.5 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary"
                                    />
                                    <div>
                                        <span className="text-xs font-bold text-theme-primary group-hover:text-theme-accent">
                                            {t('settings.syncChannelMarketLabel', 'Marktplatz & Angebote synchronisieren')}
                                        </span>
                                        <p className="text-[11px] text-theme-light mt-0.5">
                                            {t('settings.syncChannelMarketDesc', 'Eigene Angebote und Suchanfragen verschlüsselt mit Kontakten abgleichen.')}
                                        </p>
                                    </div>
                                </label>

                                <label className="flex items-start gap-3 cursor-pointer group pt-1 border-t border-theme-subtle/50">
                                    <input
                                        type="checkbox"
                                        checked={wotShareViaDrop}
                                        onChange={(e) => {
                                            setWotShareViaDrop(e.target.checked);
                                            updateFeatureToggle('wotShareViaDrop', e.target.checked);
                                        }}
                                        className="mt-0.5 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary"
                                    />
                                    <div>
                                        <span className="text-xs font-bold text-theme-primary group-hover:text-theme-accent">
                                            {t('settings.syncChannelWotLabel', 'Vertrauenseinstufungen teilen')}
                                        </span>
                                        <p className="text-[11px] text-theme-light mt-0.5">
                                            {t('settings.syncChannelWotDesc', 'Erlaubt befreundeten Kontakten, deine selektiv offengelegten Einstufungen verschlüsselt abzurufen.')}
                                        </p>
                                    </div>
                                </label>

                                <label className="flex items-start gap-3 cursor-pointer group pt-1 border-t border-theme-subtle/50">
                                    <input
                                        type="checkbox"
                                        checked={dropBackupEnabled}
                                        onChange={(e) => {
                                            setDropBackupEnabled(e.target.checked);
                                            updateFeatureToggle('dropBackupEnabled', e.target.checked);
                                        }}
                                        className="mt-0.5 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary"
                                    />
                                    <div>
                                        <span className="text-xs font-bold text-theme-primary group-hover:text-theme-accent">
                                            {t('settings.syncChannelBackupLabel', 'Backups erlauben')}
                                        </span>
                                        <p className="text-[11px] text-theme-light mt-0.5">
                                            {t('settings.syncChannelBackupDesc', 'Erlaubt diesem Wallet, verschlüsselte Sicherungen bei freigegebenen Sync-Diensten abzulegen.')}
                                        </p>
                                    </div>
                                </label>
                            </div>
                        </div>

                        {/* 2. URL-Schnelleingabe */}
                        <div className="p-4 bg-white rounded-2xl border border-theme-subtle shadow-xs space-y-3">
                            <div>
                                <h4 className="text-xs font-bold text-theme-secondary uppercase tracking-wider">
                                    {t('settings.syncAddTitle', 'Sync-Dienst hinzufügen')}
                                </h4>
                                <p className="text-[11px] text-theme-light mt-0.5">
                                    {t('settings.syncAddDesc', 'Gib die Adresse eines Freundes-Webspace oder öffentlichen Sync-Dienstes ein:')}
                                </p>
                            </div>

                            <div className="flex flex-col sm:flex-row gap-2">
                                <Input
                                    value={inputUrl}
                                    onChange={(e) => setInputUrl(e.target.value)}
                                    placeholder={t('settings.syncAddPlaceholder', 'https://beispiel.de/sync/sync.php')}
                                    className="text-xs flex-1"
                                    onKeyDown={(e) => {
                                        if (e.key === 'Enter') {
                                            e.preventDefault();
                                            handleProbeAndAdd();
                                        }
                                    }}
                                />
                                <Button
                                    type="button"
                                    variant="primary"
                                    size="sm"
                                    onClick={handleProbeAndAdd}
                                    disabled={isProbing || !inputUrl.trim()}
                                    className="gap-1.5 shrink-0"
                                >
                                    <RefreshCw size={13} className={isProbing ? 'animate-spin' : ''} />
                                    <span>{isProbing ? t('common.testing', 'Prüfe...') : t('settings.syncAddBtn', 'Verbinden & Prüfen')}</span>
                                </Button>
                            </div>

                            {probeResult && (
                                <div className={`p-2.5 rounded-xl border text-xs flex items-start gap-2 ${
                                    probeResult.status === 'success'
                                        ? 'bg-emerald-50 text-emerald-800 border-emerald-200'
                                        : probeResult.status === 'unauthorized'
                                            ? 'bg-amber-50 text-amber-800 border-amber-200'
                                            : 'bg-rose-50 text-rose-800 border-rose-200'
                                }`}>
                                    {probeResult.status === 'success' ? (
                                        <CheckCircle2 size={16} className="shrink-0 mt-0.5 text-emerald-600" />
                                    ) : (
                                        <AlertTriangle size={16} className="shrink-0 mt-0.5 text-amber-600" />
                                    )}
                                    <span className="leading-relaxed">{probeResult.message}</span>
                                </div>
                            )}
                        </div>

                        {/* 3. Discovered Friend Drops (Auto-Discovery) */}
                        <div className="space-y-3">
                            <h4 className="text-xs font-bold text-theme-secondary uppercase tracking-wider flex items-center gap-1.5">
                                <Users size={14} className="text-theme-primary" />
                                <span>{t('settings.syncDiscoveredTitle', 'Über Kontakte entdeckte Sync-Dienste')}</span>
                            </h4>

                            {(discoveredDrops || []).length === 0 ? (
                                <div className="p-4 bg-bg-app rounded-2xl border border-theme-subtle text-xs text-theme-light text-center leading-relaxed">
                                    {t('settings.syncDiscoveredEmpty', 'Noch keine Sync-Dienste von Kontakten entdeckt. Tausche Visitenkarten mit Freunden, um deren bereitgestellte Dienste automatisch zu nutzen.')}
                                </div>
                            ) : (
                                <div className="space-y-2">
                                    {(discoveredDrops || []).map((drop, idx) => (
                                        <div
                                            key={idx}
                                            className="p-3 bg-white rounded-xl border border-theme-subtle flex items-center justify-between gap-3 text-xs"
                                        >
                                            <div className="flex items-center gap-2.5 min-w-0">
                                                <input
                                                    type="checkbox"
                                                    checked={drop.isSelected}
                                                    onChange={() => handleToggleDiscoveredDrop(drop)}
                                                    className="rounded border-theme-subtle text-theme-primary focus:ring-theme-primary"
                                                    title="Für Synchronisation aktivieren"
                                                />
                                                <div className="min-w-0">
                                                    <span className="font-bold text-theme-primary truncate block">
                                                        {t('settings.syncConnectedVia', {
                                                            name: drop.sourceLabel || drop.descriptor.readUrl,
                                                            defaultValue: `Verbunden über Sync-Dienst von ${drop.sourceLabel || drop.descriptor.readUrl}`,
                                                        })}
                                                    </span>
                                                    <span className="font-mono text-[10px] text-theme-light truncate block">
                                                        {drop.descriptor.readUrl}
                                                    </span>
                                                </div>
                                            </div>

                                            <span className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-emerald-100 text-emerald-800 shrink-0">
                                                Verbunden
                                            </span>
                                        </div>
                                    ))}
                                </div>
                            )}
                        </div>

                        {/* 4. Configured Public / Manual Drops */}
                        <div className="space-y-3 pt-2 border-t border-theme-subtle/50">
                            <h4 className="text-xs font-bold text-theme-secondary uppercase tracking-wider flex items-center gap-1.5">
                                <Globe size={14} className="text-theme-primary" />
                                <span>{t('settings.syncPublicTitle', 'Manuell konfigurierte Dienste')}</span>
                            </h4>

                            {(configuredPublicDrops || []).length === 0 ? (
                                <p className="text-xs text-theme-light">
                                    {t('settings.syncPublicEmpty', 'Keine manuellen Dienste eingerichtet.')}
                                </p>
                            ) : (
                                <div className="space-y-2">
                                    {(configuredPublicDrops || []).map((drop, idx) => (
                                        <div
                                            key={idx}
                                            className="p-3 bg-white rounded-xl border border-theme-subtle flex items-center justify-between gap-3 text-xs"
                                        >
                                            <div className="min-w-0">
                                                <span className="font-mono text-xs text-theme-primary truncate block">
                                                    {drop.writeUrl}
                                                </span>
                                                <span className="text-[10px] text-theme-light block">
                                                    {(drop.authorizedChannels || []).length > 0
                                                        ? `Kanäle: ${(drop.authorizedChannels || []).join(', ')}`
                                                        : 'Schlüsselprüfung ausstehend'}
                                                </span>
                                            </div>

                                            <div className="flex items-center gap-2 shrink-0">
                                                <Button
                                                    type="button"
                                                    variant="outline"
                                                    size="sm"
                                                    onClick={() => handleTogglePublicDrop(drop)}
                                                    className="text-[11px] h-7 px-2"
                                                >
                                                    {drop.enabled ? 'Deaktivieren' : 'Aktivieren'}
                                                </Button>

                                                <button
                                                    type="button"
                                                    onClick={() => handleDeletePublicDrop(drop.writeUrl)}
                                                    className="text-theme-placeholder hover:text-rose-600 p-1 rounded transition-colors"
                                                    title={t('common.delete', 'Löschen')}
                                                >
                                                    <Trash2 size={15} />
                                                </button>
                                            </div>
                                        </div>
                                    ))}
                                </div>
                            )}
                        </div>
                    </div>
                )}

                {/* TAB 2: Host / Own Sync Service */}
                {activeTab === 'host' && (
                    <div className="space-y-6 animate-in fade-in duration-200">
                        {adminStatus == null ? (
                            /* Setup Wizard for new Own Drop */
                            <div className="space-y-6">
                                {/* Intro banner */}
                                <div className="p-4 bg-theme-primary/5 rounded-2xl border border-theme-primary/20 space-y-2 text-xs text-theme-secondary">
                                    <h4 className="font-bold text-theme-primary text-sm flex items-center gap-1.5">
                                        <Server size={16} />
                                        <span>{t('settings.syncHostIntroTitle', 'Eigenen Sync-Dienst für deinen Kreis bereitstellen')}</span>
                                    </h4>
                                    <p className="leading-relaxed">
                                        {t('settings.syncHostIntroDesc', 'Mit deinem eigenen Webspace ermöglichst du deinen Freunden sichere Postfächer, automatischen Marktplatz-Austausch und verschlüsselte Backups.')}
                                    </p>
                                    <div className="flex items-center gap-1.5 pt-1 text-[11px] font-semibold text-emerald-700 dark:text-emerald-400">
                                        <Shield size={13} />
                                        <span>{t('settings.syncHostPrivacyNotice', '100 % Ende-zu-Ende-verschlüsselt: Weder du noch dein Webhosting-Anbieter können Zahlungen oder private Angebote einsehen.')}</span>
                                    </div>
                                </div>

                                {/* Step 1: Policies & ZIP Download */}
                                <div className="p-4 bg-white rounded-2xl border border-theme-subtle space-y-4">
                                    <div className="flex items-center gap-2">
                                        <span className="w-5 h-5 rounded-full bg-theme-primary text-white text-xs font-bold flex items-center justify-center shrink-0">1</span>
                                        <h4 className="text-xs font-bold text-theme-primary uppercase tracking-wider">
                                            {t('settings.syncHostSetupStep1', 'Schritt 1: Zugriffsregeln wählen & Installationspaket herunterladen')}
                                        </h4>
                                    </div>

                                    <PolicyEditor
                                        policy={hostPolicy}
                                        onChange={setHostPolicy}
                                        idPrefix="setup"
                                    />

                                    <div className="pt-2">
                                        <Button
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            onClick={handleDownloadZip}
                                            disabled={isDownloadingZip}
                                            className="gap-2"
                                        >
                                            <FileArchive size={14} className="text-theme-primary" />
                                            <span>{isDownloadingZip ? t('common.saving', 'Erstelle...') : t('settings.syncHostDownloadZip', 'humoco-sync-setup.zip herunterladen')}</span>
                                        </Button>
                                    </div>
                                </div>

                                {/* Step 2: URL & Activation */}
                                <div className="p-4 bg-white rounded-2xl border border-theme-subtle space-y-4">
                                    <div className="flex items-center gap-2">
                                        <span className="w-5 h-5 rounded-full bg-theme-primary text-white text-xs font-bold flex items-center justify-center shrink-0">2</span>
                                        <h4 className="text-xs font-bold text-theme-primary uppercase tracking-wider">
                                            {t('settings.syncHostSetupStep2', 'Schritt 2: Webspace-Adresse eingeben & aktivieren')}
                                        </h4>
                                    </div>

                                    <div className="flex flex-col sm:flex-row gap-2">
                                        <Input
                                            value={hostWebspaceUrl}
                                            onChange={(e) => setHostWebspaceUrl(e.target.value)}
                                            placeholder={t('settings.syncHostUrlPlaceholder', 'https://dein-webspace.de/sync/sync.php')}
                                            className="text-xs flex-1"
                                        />
                                        <Button
                                            type="button"
                                            variant="primary"
                                            size="sm"
                                            onClick={handleTestAndActivateHost}
                                            disabled={isTestingHost || !hostWebspaceUrl.trim()}
                                            className="gap-1.5 shrink-0"
                                        >
                                            <RefreshCw size={13} className={isTestingHost ? 'animate-spin' : ''} />
                                            <span>{isTestingHost ? t('common.testing', 'Prüfe...') : t('settings.syncHostTestActivate', 'Verbindung testen & aktivieren')}</span>
                                        </Button>
                                    </div>
                                </div>
                            </div>
                        ) : (
                            /* Admin View for Active Own Drop */
                            <div className="space-y-6">
                                <div className="p-4 bg-emerald-500/10 border border-emerald-500/20 rounded-2xl flex items-start justify-between gap-3 text-xs">
                                    <div className="flex items-start gap-2.5">
                                        <CheckCircle2 size={18} className="text-emerald-600 mt-0.5 shrink-0" />
                                        <div>
                                            <h4 className="font-bold text-emerald-950 text-sm">
                                                {t('settings.syncHostActiveTitle', 'Dein eigener Sync-Dienst ist aktiv')}
                                            </h4>
                                            <p className="font-mono text-[11px] text-emerald-800 break-all mt-0.5">
                                                {adminStatus.descriptor.readUrl}
                                            </p>
                                        </div>
                                    </div>

                                    <Button
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        onClick={() => handleCopyFriendLink(adminStatus.descriptor.readUrl)}
                                        className="gap-1 text-xs shrink-0 bg-white"
                                    >
                                        {copiedLink ? <Check size={13} className="text-emerald-600" /> : <Copy size={13} />}
                                        <span>{copiedLink ? t('common.copied', 'Kopiert!') : t('settings.syncHostCopyLink', 'Link kopieren')}</span>
                                    </Button>
                                </div>

                                {/* Usage statistics */}
                                <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                                    <div className="p-4 bg-bg-app rounded-2xl border border-theme-subtle space-y-2">
                                        <div className="flex justify-between items-center text-xs">
                                            <span className="font-bold text-theme-secondary flex items-center gap-1.5">
                                                <HardDrive size={14} className="text-theme-primary" />
                                                {t('settings.syncHostStorageUsage', 'Speicherplatzbelegung')}
                                            </span>
                                            <span className="font-mono text-theme-light">
                                                {Math.round((adminStatus.usedBytes || 0) / 1024 / 1024 * 10) / 10} / {adminStatus.maxTotalStorageMb} MB
                                            </span>
                                        </div>
                                        <div className="w-full h-2 rounded-full bg-slate-200 overflow-hidden">
                                            <div
                                                className={`h-full transition-all ${
                                                    (adminStatus.usedBytes || 0) / (adminStatus.maxTotalStorageMb * 1024 * 1024) > 0.8
                                                        ? 'bg-amber-500'
                                                        : 'bg-emerald-500'
                                                }`}
                                                style={{
                                                    width: `${Math.min(100, Math.round(((adminStatus.usedBytes || 0) / (adminStatus.maxTotalStorageMb * 1024 * 1024)) * 100))}%`,
                                                }}
                                            />
                                        </div>
                                    </div>

                                    <div className="p-4 bg-bg-app rounded-2xl border border-theme-subtle space-y-2">
                                        <div className="flex justify-between items-center text-xs">
                                            <span className="font-bold text-theme-secondary flex items-center gap-1.5">
                                                <Users size={14} className="text-theme-primary" />
                                                {t('settings.syncHostMembers', 'Aktive Mitglieder')}
                                            </span>
                                            <span className="font-mono text-theme-light">
                                                {adminStatus.totalMemberCount || 0} / {adminStatus.maxMembers}
                                            </span>
                                        </div>
                                        <div className="w-full h-2 rounded-full bg-slate-200 overflow-hidden">
                                            <div
                                                className="h-full bg-theme-primary transition-all"
                                                style={{
                                                    width: `${Math.min(100, Math.round(((adminStatus.totalMemberCount || 0) / adminStatus.maxMembers) * 100))}%`,
                                                }}
                                            />
                                        </div>
                                    </div>
                                </div>

                                {/* Access Rules Editor for Active Service */}
                                <div className="p-4 bg-white rounded-2xl border border-theme-subtle space-y-3">
                                    <h4 className="text-xs font-bold text-theme-secondary uppercase tracking-wider flex items-center gap-1.5">
                                        <Shield size={14} className="text-theme-primary" />
                                        <span>{t('settings.syncHostAccessRulesTitle', 'Zugriffsregeln (Wer darf diesen Dienst nutzen?)')}</span>
                                    </h4>

                                    <PolicyEditor
                                        policy={hostPolicy}
                                        onChange={setHostPolicy}
                                        idPrefix="admin"
                                    />

                                    <div className="pt-2">
                                        <Button
                                            type="button"
                                            variant="primary"
                                            size="sm"
                                            onClick={handleUpdateHostPolicy}
                                            disabled={isUpdatingPolicy}
                                            className="gap-1.5 text-xs"
                                        >
                                            <RefreshCw size={13} className={isUpdatingPolicy ? 'animate-spin' : ''} />
                                            <span>{isUpdatingPolicy ? t('common.updating', 'Aktualisiere...') : t('settings.syncHostUpdatePolicy', 'Zugriffsregeln auf Server aktualisieren')}</span>
                                        </Button>
                                    </div>
                                </div>

                                {/* Danger / Management Actions */}
                                <div className="flex flex-wrap items-center gap-2 pt-2 border-t border-theme-subtle/50">
                                    <Button
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        onClick={handleRotateKey}
                                        disabled={isRotatingKey}
                                        className="gap-1.5 text-xs"
                                    >
                                        <KeyRound size={13} />
                                        <span>{t('settings.syncHostRotateKey', 'Schlüssel rotieren')}</span>
                                    </Button>

                                    <Button
                                        type="button"
                                        variant="danger"
                                        size="sm"
                                        onClick={() => setIsDeleteConfirmOpen(true)}
                                        className="gap-1.5 text-xs ml-auto"
                                    >
                                        <Trash2 size={13} />
                                        <span>{t('settings.syncHostDelete', 'Dienst löschen')}</span>
                                    </Button>
                                </div>
                            </div>
                        )}
                    </div>
                )}
            </div>

            {/* Confirmation dialog for deleting own drop */}
            <ConfirmationModal
                isOpen={isDeleteConfirmOpen}
                title={t('settings.syncHostDeleteConfirmTitle', 'Eigenen Sync-Dienst wirklich trennen?')}
                description={t('settings.syncHostDeleteConfirmDesc', 'Die Zugangsdaten werden von diesem Gerät entfernt. Auf dem Server verbleibende Daten können manuell gelöscht werden.')}
                confirmVariant="danger"
                confirmText={t('common.delete', 'Löschen')}
                isProcessing={isDeletingAdmin}
                onConfirm={handleDeleteHost}
                onCancel={() => setIsDeleteConfirmOpen(false)}
            />
        </Card>
    );
};

export default SyncServiceSettingsCard;
