// src/components/settings/sync/SyncChannelPanel.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle } from 'lucide-react';
import { Button } from '../../ui/Button';
import { ChannelSettingsState, RevokedDropNotice } from './syncHelpers';

export interface SyncChannelPanelProps {
    revokedList: RevokedDropNotice[];
    channelSettings: ChannelSettingsState;
    onChannelSettingChange: <K extends keyof ChannelSettingsState>(
        key: K,
        value: ChannelSettingsState[K]
    ) => void;
    onUseReplacementUrl: (url: string) => void;
}

/**
 * Renders sync server revocation alerts and per-channel sync preference toggles.
 */
export const SyncChannelPanel: React.FC<SyncChannelPanelProps> = ({
    revokedList,
    channelSettings,
    onChannelSettingChange,
    onUseReplacementUrl,
}) => {
    const { t } = useTranslation();

    const primaryChannels: Array<{
        key: 'postAcceptFromContacts' | 'marketSyncEnabled' | 'wotShareViaSync';
        label: string;
        desc: string;
    }> = [
        {
            key: 'postAcceptFromContacts',
            label: t('settings.syncChannelPostLabel'),
            desc: t('settings.syncChannelPostDesc'),
        },
        {
            key: 'marketSyncEnabled',
            label: t('settings.syncChannelMarketLabel'),
            desc: t('settings.syncChannelMarketDesc'),
        },
        {
            key: 'wotShareViaSync',
            label: t('settings.syncChannelWotLabel'),
            desc: t('settings.syncChannelWotDesc'),
        },
    ];

    return (
        <>
            {revokedList.length > 0 && (
                <div className="p-3.5 bg-amber-50 border border-amber-300 rounded-2xl space-y-2 text-xs text-amber-900">
                    <div className="font-bold flex items-center gap-2">
                        <AlertTriangle size={15} className="text-amber-600 shrink-0" />
                        <span>{t('settings.syncRevocationBannerTitle', 'Abschaltung eines Sync-Dienstes angekündigt')}</span>
                    </div>
                    {revokedList.map((item, idx) => (
                        <div key={item.url + idx} className="pt-1 border-t border-amber-200/70 space-y-1">
                            <div className="font-mono text-[11px] break-all font-semibold">{item.url}</div>
                            <div>
                                {item.status === 'revoked'
                                    ? t('settings.syncStatusRevoked', 'Geschlossen')
                                    : item.sunsetAt
                                    ? t('settings.syncStatusDeprecatingDate', {
                                          date: new Date(item.sunsetAt * 1000).toLocaleDateString(),
                                          defaultValue: `Schließt am ${new Date(item.sunsetAt * 1000).toLocaleDateString()}`,
                                      })
                                    : t('settings.syncStatusDeprecating', 'Wird bald geschlossen')}
                                {item.reason ? ` — ${item.reason}` : ''}
                            </div>
                            {item.replacement?.writeUrl && (
                                <div className="flex flex-wrap items-center gap-2 pt-1">
                                    <span className="font-mono text-[11px] break-all">→ {item.replacement.writeUrl}</span>
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="secondary"
                                        onClick={() => onUseReplacementUrl(item.replacement!.writeUrl)}
                                        className="text-xs"
                                    >
                                        {t('settings.syncUseReplacement', 'Neuen Sync-Dienst übernehmen')}
                                    </Button>
                                </div>
                            )}
                        </div>
                    ))}
                </div>
            )}

            <div className="p-4 bg-bg-app rounded-2xl border border-theme-subtle space-y-3">
                <h4 className="text-xs font-bold text-theme-secondary uppercase tracking-wider">
                    {t('settings.syncChannelsTitle')}
                </h4>

                <div className="space-y-2.5 pt-1">
                    {primaryChannels.map((ch, idx) => (
                        <label
                            key={ch.key}
                            className={`flex items-start gap-3 cursor-pointer group ${
                                idx > 0 ? 'pt-1 border-t border-theme-subtle/50' : ''
                            }`}
                        >
                            <input
                                type="checkbox"
                                checked={channelSettings[ch.key]}
                                onChange={(e) => onChannelSettingChange(ch.key, e.target.checked)}
                                className="mt-0.5 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary"
                            />
                            <div>
                                <span className="text-xs font-bold text-theme-primary group-hover:text-theme-accent">
                                    {ch.label}
                                </span>
                                <p className="text-[11px] text-theme-light mt-0.5">{ch.desc}</p>
                            </div>
                        </label>
                    ))}

                    <div className="pt-3 border-t border-theme-subtle/50 flex flex-col gap-1.5">
                        <div className="flex items-center justify-between gap-3">
                            <label htmlFor="shareMinLevel" className="text-xs font-bold text-theme-primary cursor-pointer">
                                {t('settings.shareMinLevel')}
                            </label>
                            <select
                                id="shareMinLevel"
                                value={channelSettings.shareMinLevel}
                                onChange={(e) => onChannelSettingChange('shareMinLevel', Number(e.target.value))}
                                className="h-8 px-2.5 bg-bg-app border border-theme-subtle rounded-lg text-xs font-semibold text-theme-secondary outline-none focus:ring-2 focus:ring-theme-primary/20 cursor-pointer"
                            >
                                <option value={0}>{t('contacts.trustLevel0')}</option>
                                <option value={1}>{t('contacts.trustLevel1')}</option>
                                <option value={2}>{t('contacts.trustLevel2')}</option>
                                <option value={3}>{t('contacts.trustLevel3')}</option>
                            </select>
                        </div>
                        <p className="text-[11px] text-theme-light">{t('settings.shareMinLevelDesc')}</p>
                    </div>

                    <label className="flex items-start gap-3 cursor-pointer group pt-3 border-t border-theme-subtle/50">
                        <input
                            type="checkbox"
                            checked={channelSettings.marketPollWifiOnly}
                            onChange={(e) => onChannelSettingChange('marketPollWifiOnly', e.target.checked)}
                            className="mt-0.5 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary"
                        />
                        <div>
                            <span className="text-xs font-bold text-theme-primary group-hover:text-theme-accent">
                                {t('settings.marketPollWifiOnly')}
                            </span>
                            <p className="text-[11px] text-theme-light mt-0.5">{t('settings.marketPollWifiOnlyDesc')}</p>
                        </div>
                    </label>
                </div>
            </div>
        </>
    );
};
