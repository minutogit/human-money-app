// src/components/contacts/WotProvidersGrid.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import { Building2, RefreshCw, Trash2 } from 'lucide-react';
import { WotProviderDto } from '../../types';
import { Advanced } from '../ui/Advanced';
import { TechId } from '../ui/TechId';

export interface WotProvidersGridProps {
    providers: WotProviderDto[];
    refreshingProviderDid: string | null;
    onRefreshProvider: (provider: WotProviderDto) => void;
    onDeleteProvider: (provider: WotProviderDto) => void;
    onSetProviderTrust: (issuerDid: string, level: number) => void;
}

const PROVIDER_TRUST_OPTIONS = [
    { level: 0, labelKey: 'contacts.providerTrustInactive' },
    { level: 1, labelKey: 'contacts.trustLevel1' },
    { level: 2, labelKey: 'contacts.trustLevel2' },
    { level: 3, labelKey: 'contacts.trustLevel3' },
];

/**
 * Expert-mode grid of imported community seals and WoT trust providers.
 */
export const WotProvidersGrid: React.FC<WotProvidersGridProps> = ({
    providers,
    refreshingProviderDid,
    onRefreshProvider,
    onDeleteProvider,
    onSetProviderTrust,
}) => {
    const { t } = useTranslation();

    return (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
            {providers.map((provider) => (
                <div key={provider.issuerDid} className="group h-full">
                    <div className="h-full bg-white border border-theme-subtle rounded-[32px] p-6 flex flex-col group-hover:border-theme-primary/30 group-hover:shadow-premium transition-all">
                        <div className="flex items-start justify-between mb-4">
                            <div className="w-14 h-14 rounded-[20px] bg-theme-primary/5 border-2 border-theme-subtle flex items-center justify-center text-theme-primary shrink-0 shadow-inner">
                                <Building2 size={28} />
                            </div>
                            <div className="flex gap-1">
                                {provider.sourceUrl && (
                                    <button
                                        onClick={() => onRefreshProvider(provider)}
                                        disabled={refreshingProviderDid === provider.issuerDid}
                                        className="p-2 hover:bg-theme-subtle/20 rounded-xl text-theme-light hover:text-theme-primary transition-all cursor-pointer disabled:opacity-50"
                                        title={t('contacts.providerRefreshButton')}
                                    >
                                        <RefreshCw
                                            size={18}
                                            className={
                                                refreshingProviderDid === provider.issuerDid
                                                    ? 'animate-spin text-theme-primary'
                                                    : ''
                                            }
                                        />
                                    </button>
                                )}
                                <button
                                    onClick={() => onDeleteProvider(provider)}
                                    className="p-2 hover:bg-rose-50 rounded-xl text-theme-light hover:text-rose-500 transition-all cursor-pointer"
                                    title={t('common.remove')}
                                >
                                    <Trash2 size={18} />
                                </button>
                            </div>
                        </div>

                        <div className="flex-1 flex flex-col">
                            <h3 className="text-lg font-black text-theme-primary tracking-tight leading-tight group-hover:text-theme-accent transition-colors">
                                {provider.name}
                            </h3>

                            <Advanced>
                                <div className="flex items-center gap-1.5 mt-2 mb-3">
                                    <TechId value={provider.issuerDid} kind="did" />
                                </div>
                            </Advanced>

                            <div className="flex items-center gap-1.5 flex-wrap mb-4">
                                <span className="px-2 py-0.5 rounded-md text-[9px] font-black uppercase tracking-wider bg-slate-100 text-slate-700 border border-slate-200">
                                    {t('contacts.providerMembers', { count: provider.memberCount })}
                                </span>
                                <span className="text-[10px] font-medium text-theme-light/80">
                                    {new Date(provider.issuedAt * 1000).toLocaleDateString()}
                                </span>
                                {provider.isStale && (
                                    <span className="px-2 py-0.5 rounded-md text-[9px] font-black uppercase tracking-wider bg-amber-50 text-amber-800 border border-amber-200">
                                        {t('contacts.providerStaleBadge')}
                                    </span>
                                )}
                            </div>

                            <div className="space-y-1.5 pt-3 border-t border-theme-subtle/40 mt-auto">
                                <label className="block text-[10px] font-black uppercase tracking-widest text-theme-light">
                                    {t('contacts.trustSectionLabel')}
                                </label>
                                <div className="grid grid-cols-4 gap-1">
                                    {PROVIDER_TRUST_OPTIONS.map((opt) => {
                                        const isSelected = provider.trustLevel === opt.level;
                                        return (
                                            <button
                                                key={opt.level}
                                                type="button"
                                                onClick={() => onSetProviderTrust(provider.issuerDid, opt.level)}
                                                className={`py-1.5 px-1 rounded-lg text-[10px] font-black uppercase tracking-wider border text-center transition-all cursor-pointer ${
                                                    isSelected
                                                        ? 'bg-theme-secondary border-theme-secondary text-white shadow-sm'
                                                        : 'bg-white border-theme-subtle text-theme-light hover:border-theme-primary/30'
                                                }`}
                                            >
                                                {t(opt.labelKey)}
                                            </button>
                                        );
                                    })}
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            ))}
        </div>
    );
};
