// src/components/settings/AdvancedTab.tsx
import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ShieldCheck, Copy, CheckCircle2 } from 'lucide-react';
import { AppConfigDto } from '../../types';
import { NetworkProtectionCard } from './NetworkProtectionCard';
import { StandardsManager } from '../standards/StandardsManager';
import { HelpIcon } from '../ui/HelpIcon';
import { Disclosure } from '../ui/Disclosure';
import { copyToClipboard } from '../../utils/clipboard';

export interface AdvancedTabProps {
    config: AppConfigDto | null;
    onConfigUpdated?: (newConfig: AppConfigDto) => void;
    localInstanceId: string;
    appVersion: string;
    isDevUnlocked: boolean;
    onDevClick: () => void;
    devClickCount: number;
    initialOpenSection?: 'network' | 'standards';
}

export const AdvancedTab: React.FC<AdvancedTabProps> = ({
    config,
    onConfigUpdated,
    localInstanceId,
    appVersion,
    isDevUnlocked,
    onDevClick,
    devClickCount,
    initialOpenSection,
}) => {
    const { t } = useTranslation();
    const [copiedDeviceId, setCopiedDeviceId] = useState(false);

    return (
        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500">
            {/* Network & L2 Protection */}
            <NetworkProtectionCard
                config={config}
                onConfigUpdated={onConfigUpdated}
                isDevUnlocked={isDevUnlocked}
                initialOpen={initialOpenSection === 'network'}
            />

            {/* Voucher Standards Manager */}
            <div id="settings-standards-section">
                <Disclosure
                    defaultOpen={initialOpenSection === 'standards'}
                    icon={<ShieldCheck size={18} className="text-theme-primary" />}
                    title={
                        <span className="font-black text-xs uppercase tracking-widest text-theme-secondary block">
                            {t('standards.title')}
                        </span>
                    }
                    subtitle={
                        <p className="text-[10px] text-theme-light/70 font-medium">
                            {t('standards.manageStandardsDescription')}
                        </p>
                    }
                    className="border border-theme-subtle rounded-2xl overflow-hidden bg-white/50 dark:bg-slate-900/50"
                    buttonClassName="w-full flex items-center justify-between p-5 bg-slate-50/70 dark:bg-slate-800/40 hover:bg-slate-100/70 transition-colors text-left cursor-pointer"
                    contentClassName="p-6 border-t border-theme-subtle bg-white dark:bg-slate-900"
                >
                    <StandardsManager />
                </Disclosure>
            </div>

            {/* Diagnostics & Dev Options Footer */}
            <div className="pt-4 border-t border-theme-subtle/40 flex flex-wrap items-center justify-center gap-x-3 gap-y-1.5 text-[11px] text-theme-light/70">
                <span
                    onClick={onDevClick}
                    className="font-mono text-[10px] text-theme-light/60 select-none cursor-pointer hover:text-theme-light/90 transition-colors"
                    title={
                        isDevUnlocked
                            ? t('settings.devModeActive', 'Entwicklermodus aktiv')
                            : t('settings.devModeClicksRemaining', {
                                  count: Math.max(0, 7 - devClickCount),
                                  defaultValue: `Klick ${Math.max(0, 7 - devClickCount)} bis Entwicklermodus`,
                              })
                    }
                    role="button"
                    tabIndex={0}
                    onKeyDown={(e) => {
                        if (e.key === 'Enter' || e.key === ' ') {
                            e.preventDefault();
                            onDevClick();
                        }
                    }}
                >
                    v{appVersion} •{' '}
                    {isDevUnlocked
                        ? t('settings.devModeUnlockedBadge', 'DEV freigeschaltet')
                        : `${devClickCount}/7`}
                </span>
                <span>•</span>
                <div className="inline-flex items-center gap-1.5">
                    <span>{t('settings.deviceIdFooterLabel')}:</span>
                    <code className="font-mono text-[10px] text-theme-light/80 select-all" title={localInstanceId}>
                        {localInstanceId
                            ? localInstanceId.length > 16
                                ? `${localInstanceId.slice(0, 8)}…${localInstanceId.slice(-4)}`
                                : localInstanceId
                            : t('settings.retrievingId')}
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
                        {copiedDeviceId ? (
                            <CheckCircle2 size={12} className="text-emerald-500" />
                        ) : (
                            <Copy size={12} />
                        )}
                    </button>
                    <HelpIcon topic="deviceBinding" size={12} />
                </div>
            </div>
        </div>
    );
};

export default AdvancedTab;
