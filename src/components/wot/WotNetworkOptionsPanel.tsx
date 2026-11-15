// src/components/wot/WotNetworkOptionsPanel.tsx
import { useTranslation } from 'react-i18next';
import { Shield, MoreHorizontal, CloudOff, Settings } from 'lucide-react';
import { PageOptionsPanel } from '../ui/PageOptionsPanel';
import { Button } from '../ui/Button';
import { HelpIcon } from '../ui/HelpIcon';
import { InfoHint } from '../ui/InfoHint';
import { WotSummaryDto } from '../../types';

export interface WotNetworkOptionsPanelProps {
    wotSummary: WotSummaryDto | null;
    currentMaxHops: number;
    isUpdatingHops: boolean;
    hasSyncService: boolean;
    onSelectMaxHops: (hops: number) => void;
    onOpenTrustListModal: () => void;
    onNavigateToSyncSettings: () => void;
    onClose?: () => void;
    className?: string;
}

export function WotNetworkOptionsPanel({
    wotSummary,
    currentMaxHops,
    isUpdatingHops,
    hasSyncService,
    onSelectMaxHops,
    onOpenTrustListModal,
    onNavigateToSyncSettings,
    onClose,
    className = '',
}: WotNetworkOptionsPanelProps) {
    const { t } = useTranslation();

    const reachAudienceExplanation = (() => {
        switch (currentMaxHops) {
            case 1:
                return t('contacts.wotReachAudience1', 'Nur Personen, die du direkt in deinen Kontakten hast.');
            case 2:
                return t('contacts.wotReachAudience2', 'Deine Kontakte und deren direkte Kontakte (Freunde von Freunden).');
            case 3:
            default:
                return t('contacts.wotReachAudience3', 'Dein erweitertes Umfeld über bis zu drei Stufen.');
        }
    })();

    return (
        <PageOptionsPanel
            title={t('contacts.networkOptionsModalTitle', 'Netzwerk-Optionen & Reichweite')}
            icon={<Shield size={18} />}
            helpTopic="wotNetwork"
            onClose={onClose}
            className={className}
            data-testid="wot-network-options-panel"
        >
            <div className="space-y-4">
                {/* Block 1 – Reach & Forwarding Limit */}
                <div className="space-y-3" data-testid="expanded-network-options">
                    <div className="space-y-1">
                        <div className="flex items-center justify-between gap-2">
                            <span className="text-xs font-bold text-theme-secondary">
                                {t('contacts.wotReachLabel', 'Netzwerk-Reichweite deiner Vertrauenseinstufungen')}
                            </span>
                            <HelpIcon topic="wotReach" size={14} />
                        </div>
                        <p className="text-[11px] text-theme-light leading-relaxed">
                            {t('contacts.wotReachSubtitle', 'Bestimme, wie weit deine Vertrauenseinstufungen im Netzwerk weitergegeben werden.')}
                        </p>
                    </div>

                    <div className="grid grid-cols-1 sm:grid-cols-3 gap-2">
                        {[
                            { hops: 1, labelKey: 'contacts.wotHops1' },
                            { hops: 2, labelKey: 'contacts.wotHops2' },
                            { hops: 3, labelKey: 'contacts.wotHops3' },
                        ].map(({ hops, labelKey }) => {
                            const isSelected = currentMaxHops === hops;
                            return (
                                <button
                                    key={hops}
                                    type="button"
                                    disabled={isUpdatingHops}
                                    onClick={() => onSelectMaxHops(hops)}
                                    className={`px-3 py-2.5 rounded-xl text-xs font-bold transition-all cursor-pointer disabled:opacity-50 text-center border ${
                                        isSelected
                                            ? 'bg-theme-primary text-white border-theme-primary shadow-sm'
                                            : 'bg-white border-theme-subtle text-theme-secondary hover:border-theme-primary/40'
                                    }`}
                                >
                                    {t(labelKey)}
                                </button>
                            );
                        })}
                    </div>

                    {/* Live sentence & InfoHint */}
                    <div className="p-3.5 bg-theme-primary/5 border border-theme-primary/10 rounded-2xl flex flex-col sm:flex-row sm:items-center justify-between gap-2 text-xs">
                        <div className="text-theme-secondary leading-relaxed">
                            <span className="font-bold text-theme-primary mr-1">
                                {t('contacts.wotReachAudienceLabel', 'Deine Einstufungen sehen:')}
                            </span>
                            <span data-testid="reach-live-sentence">{reachAudienceExplanation}</span>
                        </div>
                        <InfoHint topic="wotReach" />
                    </div>
                </div>

                {/* Block 2 – Offline Trust Lists card */}
                <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 p-3 bg-white/70 border border-theme-subtle rounded-xl">
                    <div className="flex-1 min-w-0">
                        <h5 className="text-xs font-bold text-theme-secondary">
                            {t('contacts.trustListMenuBtn', 'Vertrauensliste')}
                        </h5>
                        <p className="text-[11px] text-theme-light mt-0.5 leading-relaxed">
                            {wotSummary && (wotSummary.ratedContactsCount > 0 || wotSummary.importedPeerListsCount > 0)
                                ? `${t('contacts.wotBannerStats', {
                                      rated: wotSummary.ratedContactsCount,
                                      lists: wotSummary.importedPeerListsCount,
                                  })} · ${t('contacts.trustListMenuDesc', 'Vertrauenslisten offline teilen oder einlesen')}`
                                : t('contacts.trustListMenuDesc', 'Vertrauenslisten offline teilen oder einlesen')}
                        </p>
                    </div>
                    <Button
                        type="button"
                        variant="secondary"
                        size="sm"
                        onClick={onOpenTrustListModal}
                        className="rounded-xl text-xs py-1.5 px-3 gap-1.5 whitespace-nowrap self-start sm:self-auto shrink-0"
                        title={t('contacts.trustListMenuDesc', 'Vertrauenslisten offline teilen oder einlesen')}
                        data-testid="trust-list-menu-button"
                    >
                        <MoreHorizontal size={13} />
                        <span>{t('contacts.trustListMenuBtn', 'Vertrauensliste')}</span>
                    </Button>
                </div>

                {/* Block 3 – Sync Service recommendation banner */}
                {!hasSyncService && (
                    <div
                        data-testid="sync-service-recommendation"
                        className="p-3 rounded-xl bg-amber-500/10 border border-amber-500/20 flex flex-col sm:flex-row sm:items-center justify-between gap-3 text-amber-950"
                    >
                        <div className="flex items-start sm:items-center gap-2.5">
                            <CloudOff size={16} className="text-amber-600 shrink-0 mt-0.5 sm:mt-0" />
                            <p className="text-xs font-medium text-amber-900 leading-relaxed">
                                <span className="font-bold text-amber-950">
                                    {t('contacts.syncRecommendationTitle', 'Empfohlen')}:
                                </span>{' '}
                                {t('contacts.syncRecommendationDesc', 'Verbinde einen Sync-Dienst, damit Zahlungen, Bürgschaften und Vertrauenseinstufungen automatisch im Hintergrund fließen und du Dateien nicht manuell austauschen musst.')}
                            </p>
                        </div>
                        <Button
                            type="button"
                            size="xs"
                            variant="secondary"
                            onClick={onNavigateToSyncSettings}
                            className="self-start sm:self-auto shrink-0 bg-white hover:bg-amber-50 border-amber-200 text-amber-900 font-bold text-xs gap-1.5 shadow-xs"
                        >
                            <Settings size={13} />
                            <span>{t('contacts.connectSyncServiceBtn', 'Sync-Dienst verbinden')}</span>
                        </Button>
                    </div>
                )}
            </div>
        </PageOptionsPanel>
    );
}

export default WotNetworkOptionsPanel;
