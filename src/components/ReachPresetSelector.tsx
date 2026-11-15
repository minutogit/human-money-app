// src/components/ReachPresetSelector.tsx
import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Home, Building2, Mountain, Info, AlertCircle } from 'lucide-react';
import { ReachPresetDto, ReachPresetId } from '../types';
import { settingsService } from '../services/settingsService';
import { wotService } from '../services/wotService';
import { marketService } from '../services/marketService';
import { PresetOrCustom, PresetOption } from './ui/PresetOrCustom';
import { InfoHint } from './ui/InfoHint';
import { translateError, stringifyError } from '../utils/errorHelper';
import { logger } from '../utils/log';

export interface ReachPresetSelectorProps {
    /** Controlled onboarding mode: avoids backend IPC until profile is created. */
    localValue?: ReachPresetId;
    onLocalChange?: (preset: ReachPresetId) => void;
    /** Optional wrapper for password-protected actions in live mode. */
    protectAction?: <T>(fn: (password: string | null) => Promise<T>) => Promise<T | void>;
    /** Called after a preset or granular reach setting has changed in live mode. */
    onChanged?: (dto: ReachPresetDto) => void;
    /** Optional custom handler for updating WoT forwarding hops (1..3). */
    onWotMaxHopsChange?: (hops: number) => Promise<void> | void;
    /** Optional custom handler for updating business card max hops (1..3). */
    onCardMaxHopsChange?: (hops: number) => Promise<void> | void;
    /** Fallback initial values when tests mock only component-specific IPC calls. */
    initialOwnListMaxHops?: number;
    initialCardMaxHops?: number;
    showHeader?: boolean;
    disabled?: boolean;
    className?: string;
}

const PRESET_TUPLES: Record<
    ReachPresetId,
    Pick<ReachPresetDto, 'ownListMaxHops' | 'shareMinLevel' | 'cardMaxHops' | 'mediaAutoLoadMaxHops'>
> = {
    near: { ownListMaxHops: 1, shareMinLevel: 2, cardMaxHops: 1, mediaAutoLoadMaxHops: 1 },
    medium: { ownListMaxHops: 2, shareMinLevel: 1, cardMaxHops: 2, mediaAutoLoadMaxHops: 2 },
    far: { ownListMaxHops: 3, shareMinLevel: 1, cardMaxHops: 3, mediaAutoLoadMaxHops: 2 },
};

function detectLocalFallback(
    ownListMaxHops: number,
    shareMinLevel: number,
    cardMaxHops: number,
    mediaAutoLoadMaxHops: number
): ReachPresetId | 'custom' {
    if (ownListMaxHops === 1 && shareMinLevel === 2 && cardMaxHops === 1 && mediaAutoLoadMaxHops === 1) {
        return 'near';
    }
    if (ownListMaxHops === 2 && shareMinLevel === 1 && cardMaxHops === 2 && mediaAutoLoadMaxHops === 2) {
        return 'medium';
    }
    if (ownListMaxHops === 3 && shareMinLevel === 1 && cardMaxHops === 3 && mediaAutoLoadMaxHops === 2) {
        return 'far';
    }
    return 'custom';
}

export function ReachPresetSelector({
    localValue,
    onLocalChange,
    protectAction,
    onChanged,
    onWotMaxHopsChange,
    onCardMaxHopsChange,
    initialOwnListMaxHops = 2,
    initialCardMaxHops = 2,
    showHeader = true,
    disabled = false,
    className = '',
}: ReachPresetSelectorProps) {
    const { t } = useTranslation();
    const isControlled = localValue !== undefined && onLocalChange !== undefined;

    const [dto, setDto] = useState<ReachPresetDto>(() => ({
        preset: detectLocalFallback(initialOwnListMaxHops, 1, initialCardMaxHops, 2),
        ownListMaxHops: initialOwnListMaxHops,
        cardMaxHops: initialCardMaxHops,
        shareMinLevel: 1,
        mediaAutoLoadMaxHops: 2,
    }));
    const [isUpdating, setIsUpdating] = useState(false);
    const [errorMsg, setErrorMsg] = useState<string | null>(null);

    const runProtected = useCallback(
        async <T,>(fn: (password: string | null) => Promise<T>): Promise<T | void> => {
            if (protectAction) {
                return await protectAction(fn);
            }
            return await fn(null);
        },
        [protectAction]
    );

    const refreshFromBackend = useCallback(async () => {
        if (isControlled || typeof settingsService.getReachPreset !== 'function') return null;
        try {
            const res = await settingsService.getReachPreset();
            if (res && typeof res.preset === 'string') {
                setDto(res);
                return res;
            }
        } catch (err) {
            logger.debug('getReachPreset skipped or unavailable: ' + stringifyError(err));
        }
        return null;
    }, [isControlled]);

    useEffect(() => {
        if (isControlled || typeof settingsService.getReachPreset !== 'function') return;
        let mounted = true;
        settingsService
            .getReachPreset()
            .then((res) => {
                if (mounted && res && typeof res.preset === 'string') {
                    setDto(res);
                }
            })
            .catch((err) => {
                logger.debug('getReachPreset mount fetch failed: ' + stringifyError(err));
            });
        return () => {
            mounted = false;
        };
    }, [isControlled]);

    const applyOptimisticPatch = useCallback(
        (patch: Partial<Omit<ReachPresetDto, 'preset'>>) => {
            setDto((prev) => {
                const ownListMaxHops = patch.ownListMaxHops ?? prev.ownListMaxHops;
                const shareMinLevel = patch.shareMinLevel ?? prev.shareMinLevel;
                const cardMaxHops = patch.cardMaxHops ?? prev.cardMaxHops;
                const mediaAutoLoadMaxHops = patch.mediaAutoLoadMaxHops ?? prev.mediaAutoLoadMaxHops;
                return {
                    preset: detectLocalFallback(
                        ownListMaxHops,
                        shareMinLevel,
                        cardMaxHops,
                        mediaAutoLoadMaxHops
                    ),
                    ownListMaxHops,
                    shareMinLevel,
                    cardMaxHops,
                    mediaAutoLoadMaxHops,
                };
            });
        },
        []
    );

    const handleSelectPreset = async (presetId: ReachPresetId) => {
        if (disabled || isUpdating) return;
        if (isControlled) {
            onLocalChange(presetId);
            return;
        }

        setIsUpdating(true);
        setErrorMsg(null);
        try {
            const updated =
                typeof settingsService.applyReachPreset === 'function'
                    ? await runProtected(async (pwd) => {
                          return await settingsService.applyReachPreset(presetId, pwd || undefined);
                      })
                    : null;
            const resolved: ReachPresetDto =
                updated && typeof updated.preset === 'string'
                    ? updated
                    : { preset: presetId, ...PRESET_TUPLES[presetId] };
            setDto(resolved);
            onChanged?.(resolved);
        } catch (err) {
            logger.error('Failed to apply reach preset: ' + stringifyError(err));
            setErrorMsg(translateError(err, t));
        } finally {
            setIsUpdating(false);
        }
    };

    const handleChangeWotHops = async (hops: number) => {
        if (disabled || isUpdating) return;
        setIsUpdating(true);
        setErrorMsg(null);
        try {
            if (onWotMaxHopsChange) {
                await onWotMaxHopsChange(hops);
            } else {
                await runProtected(async (pwd) => {
                    await wotService.setMaxHops(hops, pwd || null);
                });
            }
            const refreshed = await refreshFromBackend();
            if (refreshed) {
                onChanged?.(refreshed);
            } else {
                applyOptimisticPatch({ ownListMaxHops: hops });
            }
        } catch (err) {
            logger.error('Failed to update WoT hops: ' + stringifyError(err));
            setErrorMsg(translateError(err, t));
        } finally {
            setIsUpdating(false);
        }
    };

    const handleChangeCardMaxHops = async (hops: number) => {
        if (disabled || isUpdating) return;
        setIsUpdating(true);
        setErrorMsg(null);
        try {
            if (onCardMaxHopsChange) {
                await onCardMaxHopsChange(hops);
            } else {
                await runProtected(async (pwd) => {
                    const current = await marketService.getMyMarketProfile(pwd || undefined);
                    await marketService.saveMyMarketProfile(
                        {
                            profile: current.profile,
                            visibility: current.visibility,
                            skills: current.skills,
                            maxHops: hops,
                            validityYears: current.validityYears ?? 2,
                            allowInquiries: current.allowInquiries,
                        },
                        pwd || undefined
                    );
                });
            }
            const refreshed = await refreshFromBackend();
            if (refreshed) {
                onChanged?.(refreshed);
            } else {
                applyOptimisticPatch({ cardMaxHops: hops });
            }
        } catch (err) {
            logger.error('Failed to update business card reach: ' + stringifyError(err));
            setErrorMsg(translateError(err, t));
        } finally {
            setIsUpdating(false);
        }
    };

    const handleChangeShareMinLevel = async (level: number) => {
        if (disabled || isUpdating) return;
        setIsUpdating(true);
        setErrorMsg(null);
        try {
            await runProtected(async (pwd) => {
                const current = await settingsService.getSettings();
                await settingsService.saveSettings(
                    { ...current, shareMinLevel: level },
                    pwd || undefined
                );
            });
            const refreshed = await refreshFromBackend();
            if (refreshed) {
                onChanged?.(refreshed);
            } else {
                applyOptimisticPatch({ shareMinLevel: level });
            }
        } catch (err) {
            logger.error('Failed to update sync share min level: ' + stringifyError(err));
            setErrorMsg(translateError(err, t));
        } finally {
            setIsUpdating(false);
        }
    };

    const handleChangeMediaAutoLoad = async (maxHops: number) => {
        if (disabled || isUpdating) return;
        setIsUpdating(true);
        setErrorMsg(null);
        try {
            await runProtected(async (pwd) => {
                const current = await settingsService.getSettings();
                await settingsService.saveSettings(
                    { ...current, mediaAutoLoadMaxHops: maxHops },
                    pwd || undefined
                );
            });
            const refreshed = await refreshFromBackend();
            if (refreshed) {
                onChanged?.(refreshed);
            } else {
                applyOptimisticPatch({ mediaAutoLoadMaxHops: maxHops });
            }
        } catch (err) {
            logger.error('Failed to update media auto-load setting: ' + stringifyError(err));
            setErrorMsg(translateError(err, t));
        } finally {
            setIsUpdating(false);
        }
    };

    const activePreset: ReachPresetId | 'custom' = isControlled ? localValue : dto.preset;

    const presets: Array<PresetOption<ReachPresetId>> = [
        {
            id: 'near',
            icon: <Home size={16} className="text-theme-primary" />,
            title: t('settings.reachNearTitle'),
            description: (
                <span>
                    <span className="block">{t('settings.reachNearDesc')}</span>
                    <span className="block text-[11px] font-medium text-theme-secondary/80 mt-1">
                        {t('settings.reachNearFit')}
                    </span>
                </span>
            ),
        },
        {
            id: 'medium',
            icon: <Building2 size={16} className="text-theme-primary" />,
            title: t('settings.reachMediumTitle'),
            recommended: true,
            description: (
                <span>
                    <span className="block">{t('settings.reachMediumDesc')}</span>
                    <span className="block text-[11px] font-medium text-theme-secondary/80 mt-1">
                        {t('settings.reachMediumFit')}
                    </span>
                </span>
            ),
        },
        {
            id: 'far',
            icon: <Mountain size={16} className="text-theme-primary" />,
            title: t('settings.reachFarTitle'),
            description: (
                <span>
                    <span className="block">{t('settings.reachFarDesc')}</span>
                    <span className="block text-[11px] font-medium text-theme-secondary/80 mt-1">
                        {t('settings.reachFarFit')}
                    </span>
                </span>
            ),
        },
    ];

    const renderCustomControls = () => (
        <div className="space-y-4 pt-1" data-testid="reach-custom-controls">
            {/* 1. WoT Forwarding (1..3) */}
            <div className="space-y-2">
                <span className="text-xs font-bold text-theme-secondary block">
                    {t('settings.reachCustomWotLabel')}
                </span>
                <div className="grid grid-cols-1 sm:grid-cols-3 gap-2">
                    {[
                        { hops: 1, labelKey: 'contacts.wotHops1' },
                        { hops: 2, labelKey: 'contacts.wotHops2' },
                        { hops: 3, labelKey: 'contacts.wotHops3' },
                    ].map(({ hops, labelKey }) => {
                        const isSelected = dto.ownListMaxHops === hops;
                        return (
                            <button
                                key={hops}
                                type="button"
                                disabled={disabled || isUpdating}
                                onClick={() => handleChangeWotHops(hops)}
                                className={`px-3 py-2 rounded-xl text-xs font-bold transition-all cursor-pointer disabled:opacity-50 text-center border ${
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
            </div>

            {/* 2. Card Max Hops (1..3) */}
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pt-2 border-t border-theme-subtle/30">
                <label htmlFor="reachSelect" className="text-xs font-bold text-theme-secondary">
                    {t('settings.reachCustomCardLabel')}
                </label>
                <select
                    id="reachSelect"
                    data-testid="reach-select"
                    disabled={disabled || isUpdating}
                    value={dto.cardMaxHops}
                    onChange={(e) => handleChangeCardMaxHops(Number(e.target.value))}
                    className="px-3 py-1.5 rounded-xl text-xs font-medium bg-white border border-theme-subtle text-theme-secondary shrink-0 focus:outline-none focus:ring-2 focus:ring-theme-primary/30 cursor-pointer"
                >
                    <option value={1}>{t('marketplace.maxHops1Short')}</option>
                    <option value={2}>{t('marketplace.maxHops2Short')}</option>
                    <option value={3}>{t('marketplace.maxHops3Short')}</option>
                </select>
            </div>

            {/* 3. Sync Service Share Min Level (0..3) */}
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pt-2 border-t border-theme-subtle/30">
                <label htmlFor="shareMinLevel" className="text-xs font-bold text-theme-secondary">
                    {t('settings.reachCustomSyncAccessLabel')}
                </label>
                <select
                    id="shareMinLevel"
                    data-testid="reach-share-min-level-select"
                    disabled={disabled || isUpdating}
                    value={dto.shareMinLevel}
                    onChange={(e) => handleChangeShareMinLevel(Number(e.target.value))}
                    className="px-3 py-1.5 rounded-xl text-xs font-medium bg-white border border-theme-subtle text-theme-secondary shrink-0 focus:outline-none focus:ring-2 focus:ring-theme-primary/30 cursor-pointer"
                >
                    <option value={0}>{t('contacts.trustLevel0')}</option>
                    <option value={1}>{t('contacts.trustLevel1')}</option>
                    <option value={2}>{t('contacts.trustLevel2')}</option>
                    <option value={3}>{t('contacts.trustLevel3')}</option>
                </select>
            </div>

            {/* 4. Reader-side Auto-Load Images (0..3) */}
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pt-2 border-t border-theme-subtle/30">
                <label htmlFor="reachMediaAutoLoadSelect" className="text-xs font-bold text-theme-secondary">
                    {t('settings.reachCustomMediaLabel')}
                </label>
                <select
                    id="reachMediaAutoLoadSelect"
                    data-testid="media-auto-load-select"
                    disabled={disabled || isUpdating}
                    value={dto.mediaAutoLoadMaxHops}
                    onChange={(e) => handleChangeMediaAutoLoad(Number(e.target.value))}
                    className="px-3 py-1.5 rounded-xl text-xs font-medium bg-white border border-theme-subtle text-theme-secondary shrink-0 focus:outline-none focus:ring-2 focus:ring-theme-primary/30 cursor-pointer"
                >
                    <option value={0}>{t('settings.mediaAutoLoad0')}</option>
                    <option value={1}>{t('settings.mediaAutoLoad1')}</option>
                    <option value={2}>{t('settings.mediaAutoLoad2')}</option>
                    <option value={3}>{t('settings.mediaAutoLoad3')}</option>
                </select>
            </div>
        </div>
    );

    return (
        <div className={`space-y-3 ${className}`} data-testid="reach-preset-selector">
            {showHeader && (
                <div className="space-y-1">
                    <div className="flex items-center justify-between gap-2">
                        <h4 className="text-sm font-bold text-theme-secondary">
                            {t('settings.reachTitle')}
                        </h4>
                        <InfoHint topic="visibilityReach" />
                    </div>
                    <p className="text-xs text-theme-light leading-relaxed">
                        {t('settings.reachSubtitle')}
                    </p>
                </div>
            )}

            {errorMsg && (
                <div className="p-3 bg-rose-50 border border-rose-200 text-rose-700 text-xs rounded-xl flex items-center justify-between gap-2">
                    <span>{errorMsg}</span>
                    <button
                        type="button"
                        onClick={() => setErrorMsg(null)}
                        className="text-rose-500 hover:text-rose-700 font-bold cursor-pointer"
                    >
                        ×
                    </button>
                </div>
            )}

            <PresetOrCustom<ReachPresetId>
                presets={presets}
                value={activePreset}
                onChange={handleSelectPreset}
                renderCustom={isControlled ? undefined : renderCustomControls}
            />

            <div className="space-y-2">
                <p
                    className="text-[11px] text-theme-light flex items-start gap-1.5 leading-relaxed"
                    data-testid="reach-reciprocal-hint"
                >
                    <Info size={13} className="shrink-0 mt-0.5 text-theme-primary" />
                    <span>{t('settings.reachReciprocalHint')}</span>
                </p>

                {activePreset === 'far' && (
                    <div
                        className="p-3 rounded-xl bg-amber-50 border border-amber-200 text-amber-900 text-xs flex items-start gap-2 leading-relaxed"
                        data-testid="reach-far-privacy-hint"
                    >
                        <AlertCircle size={15} className="shrink-0 mt-0.5 text-amber-600" />
                        <span>{t('settings.reachFarPrivacyHint')}</span>
                    </div>
                )}
            </div>
        </div>
    );
}
