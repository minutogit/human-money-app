// src/components/settings/AdvancedTab.tsx
import React, { useState, FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import {
    ChevronDown,
    ChevronUp,
    ShieldCheck,
    Clock,
    Save,
    Copy,
    CheckCircle2
} from 'lucide-react';
import { AppSettings } from '../../types';
import { NetworkProtectionCard } from './NetworkProtectionCard';
import { SyncServiceSettingsCard } from './SyncServiceSettingsCard';
import { StandardsManager } from '../StandardsManager';
import { Button } from '../ui/Button';
import { HelpIcon } from '../ui/HelpIcon';
import { AlertBanner } from '../ui/AlertBanner';
import { copyToClipboard } from '../../utils/clipboard';

export interface AdvancedTabProps {
    settings: AppSettings | null;
    onSettingsUpdated: (updater: AppSettings | ((prev: AppSettings | null) => AppSettings | null)) => void;
    onSave: (event: FormEvent) => void;
    isSaving: boolean;
    saveError: string;
    saveSuccess: string;
    localInstanceId: string;
    appVersion: string;
    isDevUnlocked: boolean;
    onDevClick: () => void;
    devClickCount: number;
    initialOpenSection?: 'network' | 'standards';
}

export const AdvancedTab: React.FC<AdvancedTabProps> = ({
    settings,
    onSettingsUpdated,
    onSave,
    isSaving,
    saveError,
    saveSuccess,
    localInstanceId,
    appVersion,
    isDevUnlocked,
    onDevClick,
    devClickCount,
    initialOpenSection,
}) => {
    const { t } = useTranslation();
    const [isStandardsOpen, setIsStandardsOpen] = useState(initialOpenSection === 'standards');
    const [copiedDeviceId, setCopiedDeviceId] = useState(false);

    return (
        <form onSubmit={onSave} className="space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500">
            {/* Network & L2 Protection */}
            <NetworkProtectionCard
                settings={settings}
                onSettingsUpdated={onSettingsUpdated}
                isDevUnlocked={isDevUnlocked}
                initialOpen={initialOpenSection === 'network'}
            />

            {/* Voucher Standards Manager */}
            <div id="settings-standards-section" className="border border-theme-subtle rounded-2xl overflow-hidden bg-white/50 dark:bg-slate-900/50">
                <button
                    type="button"
                    onClick={() => setIsStandardsOpen(v => !v)}
                    className="w-full flex items-center justify-between p-5 bg-slate-50/70 dark:bg-slate-800/40 hover:bg-slate-100/70 transition-colors text-left"
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
                    <div className="p-6 border-t border-theme-subtle bg-white dark:bg-slate-900">
                        <StandardsManager />
                    </div>
                )}
            </div>

            {/* Eigenen Dienst betreiben (Host / Admin Mode) */}
            <div id="settings-host-section">
                <SyncServiceSettingsCard
                    settings={settings}
                    onSettingsUpdated={(updated) => onSettingsUpdated(updated)}
                    mode="host"
                />
            </div>

            {/* Save Button */}
            <div className="pt-2 flex flex-col items-center gap-4">
                <Button type="submit" disabled={isSaving} className="min-w-[240px] gap-2 rounded-2xl py-4 shadow-lg shadow-theme-primary/20">
                    {isSaving ? <Clock className="animate-spin" size={18} /> : <Save size={18} />}
                    {isSaving ? t('settings.saving') : t('settings.saveButton')}
                </Button>
                {saveError && <AlertBanner variant="error" className="w-full max-w-md">{saveError}</AlertBanner>}
                {saveSuccess && <AlertBanner variant="success" className="w-full max-w-md">{saveSuccess}</AlertBanner>}

                {/* Diagnostics & Dev Options Footer */}
                <div className="pt-4 border-t border-theme-subtle/40 flex flex-wrap items-center justify-center gap-x-3 gap-y-1.5 text-[11px] text-theme-light/70">
                    <span
                        onClick={onDevClick}
                        className="font-mono text-[10px] text-theme-light/60 select-none cursor-pointer hover:text-theme-light/90 transition-colors"
                        title={isDevUnlocked ? "Entwicklermodus aktiv" : `Klick ${Math.max(0, 7 - devClickCount)} bis Entwicklermodus`}
                        role="button"
                        tabIndex={0}
                        onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onDevClick(); }}}
                    >
                        v{appVersion} • {isDevUnlocked ? 'DEV freigeschaltet' : `${devClickCount}/7`}
                    </span>
                    <span>•</span>
                    <div className="inline-flex items-center gap-1.5">
                        <span>{t('settings.deviceIdFooterLabel')}:</span>
                        <code className="font-mono text-[10px] text-theme-light/80 select-all" title={localInstanceId}>
                            {localInstanceId ? (localInstanceId.length > 16 ? `${localInstanceId.slice(0, 8)}…${localInstanceId.slice(-4)}` : localInstanceId) : t('settings.retrievingId')}
                        </code>
                        <button
                            type="button"
                            onClick={async () => {
                                if (localInstanceId) {
                                    const ok = await copyToClipboard(localInstanceId);
                                    try {
                                        if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
                                            await navigator.clipboard.writeText(localInstanceId);
                                        }
                                    } catch {
                                        // Ignore clipboard write failure
                                    }
                                    if (ok) {
                                        setCopiedDeviceId(true);
                                        setTimeout(() => setCopiedDeviceId(false), 2000);
                                    }
                                }
                            }}
                            className="p-1 text-theme-light/70 hover:text-theme-primary transition-colors rounded inline-flex items-center justify-center"
                            aria-label={copiedDeviceId ? t('common.copied') : t('settings.copyButton')}
                            title={copiedDeviceId ? t('common.copied') : t('settings.copyButton')}
                        >
                            {copiedDeviceId ? <CheckCircle2 size={12} className="text-emerald-500" /> : <Copy size={12} />}
                        </button>
                        <HelpIcon topic="deviceBinding" size={12} />
                    </div>
                </div>
            </div>
        </form>
    );
};

export default AdvancedTab;
