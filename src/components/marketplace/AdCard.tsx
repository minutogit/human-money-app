// src/components/marketplace/AdCard.tsx
import React, { useState, useEffect, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { MessageSquare, ArrowRight, Clock, Tag, ChevronDown, ChevronUp, User, Globe, ImageOff, Image as ImageIcon, Edit3 } from 'lucide-react';
import { Ad, FrontendEntrySource, FrontendTrustTier } from '../../types';
import { Button } from '../ui/Button';
import { TrustBadge } from '../ui/TrustBadge';
import { truncateUserId } from '../../utils/userIdHelper';
import { isMeteredOrCellularConnection } from '../../utils/networkStatus';
import { marketService } from '../../services/marketService';

const adImageCache = new Map<string, string | null>();

export function clearAdImageCache(): void {
    adImageCache.clear();
}

export function resolveAuthorHops(sources?: FrontendEntrySource[]): number {
    if (!sources || sources.length === 0) return 99;
    return Math.min(
        ...sources.map((s) => (s.kind === 'card' ? s.hops : 1))
    );
}

export interface AdCardProps {
    ad: Ad;
    ownerDid: string;
    ownerName: string;
    trust?: FrontendTrustTier;
    daysLeft?: number;
    allowInquiries?: boolean;
    isOwn?: boolean;
    mediaAutoLoadMaxHops?: number;
    wifiOnly?: boolean;
    authorHops?: number;
    onEditOwnAd?: (ad: Ad) => void;
    onInquire?: (ad: Ad, ownerDid: string, ownerName: string) => void;
    onPay?: (ownerDid: string) => void;
    onOwnerClick?: (ownerDid: string, isOwn?: boolean) => void;
}

export const AdCard: React.FC<AdCardProps> = ({
    ad,
    ownerDid,
    ownerName,
    trust,
    daysLeft,
    allowInquiries,
    isOwn,
    mediaAutoLoadMaxHops = 2,
    wifiOnly = false,
    authorHops,
    onEditOwnAd,
    onInquire,
    onPay,
    onOwnerClick,
}) => {
    const { t } = useTranslation();
    const [isExpanded, setIsExpanded] = useState(false);

    const imageToken = ad.imageToken;
    const effectiveAuthorHops = isOwn ? 0 : (authorHops ?? 1);
    const isWifiBlocked = Boolean(!isOwn && wifiOnly && isMeteredOrCellularConnection());
    const shouldAutoLoad = Boolean(
        isOwn || (!isWifiBlocked && effectiveAuthorHops <= mediaAutoLoadMaxHops)
    );
    const [manualLoadRequested, setManualLoadRequested] = useState(false);

    useEffect(() => {
        setManualLoadRequested(false);
    }, [ownerDid, imageToken]);

    const [imageUri, setImageUri] = useState<string | null>(() => {
        if (!imageToken) return null;
        return adImageCache.get(`${ownerDid}:${imageToken}`) ?? null;
    });
    const [imageFailed, setImageFailed] = useState<boolean>(() => {
        if (!imageToken) return false;
        const cached = adImageCache.get(`${ownerDid}:${imageToken}`);
        return cached === null && adImageCache.has(`${ownerDid}:${imageToken}`);
    });

    useEffect(() => {
        if (!imageToken) {
            setImageUri(null);
            setImageFailed(false);
            return;
        }

        const cacheKey = `${ownerDid}:${imageToken}`;
        if (adImageCache.has(cacheKey)) {
            const cached = adImageCache.get(cacheKey);
            if (cached) {
                setImageUri(cached);
                setImageFailed(false);
                return;
            }
        }

        if (!shouldAutoLoad && !manualLoadRequested) {
            setImageUri(null);
            setImageFailed(false);
            return;
        }

        let isMounted = true;
        marketService
            .getAdImage(ownerDid, imageToken)
            .then((res) => {
                if (res) {
                    adImageCache.set(cacheKey, res);
                }
                if (!isMounted) return;
                if (res) {
                    setImageUri(res);
                    setImageFailed(false);
                } else {
                    setImageUri(null);
                    setImageFailed(true);
                }
            })
            .catch(() => {
                if (!isMounted) return;
                setImageUri(null);
                setImageFailed(true);
            });

        return () => {
            isMounted = false;
        };
    }, [ownerDid, imageToken, shouldAutoLoad, manualLoadRequested]);

    const nowSecs = Math.floor(Date.now() / 1000);
    const effectiveDaysLeft =
        daysLeft !== undefined
            ? daysLeft
            : ad.expiresAt > nowSecs
            ? Math.ceil((ad.expiresAt - nowSecs) / 86400)
            : 0;

    const isOffer = ad.kind === 'offer';
    const isExpiringSoon = effectiveDaysLeft <= 14;

    const trimmedName = ownerName.trim();
    const displayName = trimmedName.startsWith('did:key:')
        ? truncateUserId(trimmedName)
        : (trimmedName || truncateUserId(ownerDid));

    const trustDto = useMemo(() => {
        if (!trust) return null;
        return {
            score: trust === 'trusted' ? 0.85 : 0,
            trustPercent: trust === 'trusted' ? 85 : 0,
            isOwnVoucher: false,
            blockedByOffender: false,
            additionalPathsCount: 0,
            viaGuarantor: false,
            bestPathNames: [],
        };
    }, [trust]);

    return (
        <div
            className="p-4 sm:p-5 rounded-2xl bg-white border border-theme-subtle hover:border-theme-primary/30 transition-all duration-200 shadow-xs flex flex-col justify-between gap-4"
            data-testid={`ad-card-${ad.id}`}
        >
            <div className="space-y-3">
                {/* Header row: Kind badge + Remote badge + Days left badge */}
                <div className="flex items-center justify-between gap-2 flex-wrap">
                    <div className="flex items-center gap-1.5 flex-wrap">
                        <span
                            className={`inline-flex items-center px-2.5 py-1 rounded-full text-[11px] font-bold uppercase tracking-wider ${
                                isOffer
                                    ? 'bg-emerald-50 text-emerald-700 border border-emerald-200'
                                    : 'bg-sky-50 text-sky-700 border border-sky-200'
                            }`}
                            data-testid="ad-kind-badge"
                        >
                            {isOffer ? t('marketplace.adKindOffer', 'Angebot') : t('marketplace.adKindRequest', 'Gesuch')}
                        </span>
                        {isOwn && (
                            <span
                                className="inline-flex items-center px-2.5 py-1 rounded-full text-[11px] font-bold bg-amber-50 text-amber-800 border border-amber-200"
                                data-testid="ad-own-badge"
                            >
                                {t('marketplace.myAdBadge', 'Mein Inserat')}
                            </span>
                        )}
                        {ad.isRemote && (
                            <span
                                className="inline-flex items-center gap-1 px-2.5 py-1 rounded-full text-[11px] font-semibold bg-indigo-50 text-indigo-700 border border-indigo-200"
                                data-testid="ad-remote-badge"
                            >
                                <Globe size={11} />
                                <span>{t('marketplace.remoteBadge', 'Ortsunabhängig')}</span>
                            </span>
                        )}
                    </div>

                    <span
                        className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-semibold ${
                            isExpiringSoon
                                ? 'bg-amber-50 text-amber-800 border border-amber-200'
                                : 'bg-bg-app text-theme-light border border-theme-subtle'
                        }`}
                        title={t('marketplace.daysLeftTooltip', 'Gültig bis {{date}} (noch {{days}} Tage)', {
                            date: new Date(ad.expiresAt * 1000).toLocaleDateString(),
                            days: effectiveDaysLeft,
                        })}
                    >
                        <Clock size={12} className={isExpiringSoon ? 'text-amber-600' : 'text-theme-light'} />
                        <span>
                            {effectiveDaysLeft === 1
                                ? t('marketplace.oneDayLeft', 'noch 1 Tag')
                                : t('marketplace.daysLeft', 'noch {{count}} Tage', { count: effectiveDaysLeft })}
                        </span>
                    </span>
                </div>

                {/* Ad Image / Deferred Placeholder / Graceful Degradation */}
                {imageToken && (
                    imageUri && !imageFailed ? (
                        <img
                            src={imageUri}
                            alt={ad.title}
                            data-testid="ad-card-image"
                            className="w-full max-h-56 object-cover rounded-xl border border-theme-subtle/50"
                            onError={() => setImageFailed(true)}
                        />
                    ) : imageFailed ? (
                        <div
                            data-testid="ad-card-image-unavailable"
                            className="flex items-center gap-2 px-3 py-2 rounded-xl bg-bg-app border border-theme-subtle/60 text-theme-light text-xs"
                        >
                            <ImageOff size={14} className="shrink-0 text-theme-light/70" />
                            <span>{t('marketplace.imageUnavailable', 'Bild nicht mehr verfügbar')}</span>
                        </div>
                    ) : !shouldAutoLoad && !manualLoadRequested ? (
                        <div
                            data-testid="ad-card-image-placeholder"
                            className="flex flex-col sm:flex-row sm:items-center justify-between gap-2.5 p-3 rounded-xl bg-bg-app border border-theme-subtle text-xs text-theme-light"
                        >
                            <div className="flex items-center gap-2.5 min-w-0">
                                <div className="w-8 h-8 rounded-lg bg-theme-primary/10 text-theme-primary flex items-center justify-center shrink-0">
                                    <ImageIcon size={16} />
                                </div>
                                <span className="leading-snug">
                                    {isWifiBlocked
                                        ? t('marketplace.imageWifiOnlyHint')
                                        : t('marketplace.imageDeferredHint')}
                                </span>
                            </div>
                            <Button
                                type="button"
                                size="xs"
                                variant="outline"
                                onClick={() => setManualLoadRequested(true)}
                                data-testid="ad-card-load-image-btn"
                                className="shrink-0 self-start sm:self-auto"
                            >
                                {t('marketplace.loadImageBtn')}
                            </Button>
                        </div>
                    ) : null
                )}

                {/* Title */}
                <h3 className="text-base font-bold text-theme-secondary leading-snug break-words">
                    {ad.title}
                </h3>

                {/* Text (expandable) */}
                {ad.text && (
                    <div className="text-xs text-theme-light leading-relaxed">
                        <p className={`break-words ${!isExpanded ? 'line-clamp-2' : ''}`}>
                            {ad.text}
                        </p>
                        {ad.text.length > 90 && (
                            <button
                                type="button"
                                onClick={() => setIsExpanded(!isExpanded)}
                                className="text-theme-primary hover:underline text-[11px] font-semibold mt-1 inline-flex items-center gap-0.5"
                            >
                                <span>{isExpanded ? t('common.showLess', 'Weniger anzeigen') : t('common.showMore', 'Mehr anzeigen')}</span>
                                {isExpanded ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
                            </button>
                        )}
                    </div>
                )}

                {/* Pricing badge */}
                {ad.pricing && (
                    <div className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-xl bg-theme-primary/5 text-theme-primary text-xs font-semibold border border-theme-primary/10">
                        <Tag size={12} />
                        <span>{ad.pricing}</span>
                    </div>
                )}
            </div>

            {/* Footer row: Owner name + Trust badge, and Action buttons */}
            <div className="pt-3 border-t border-theme-subtle/40 flex flex-wrap items-center justify-between gap-3">
                <button
                    type="button"
                    onClick={() => (isOwn ? onOwnerClick?.(ownerDid, true) : onOwnerClick?.(ownerDid))}
                    className="flex items-center gap-2 group text-left min-w-0"
                    title={isOwn ? t('marketplace.myOfferBtn', 'Mein Angebot') : t('marketplace.viewPersonCard', 'Visitenkarte ansehen')}
                >
                    <div className="w-7 h-7 rounded-full bg-theme-subtle/50 text-theme-primary flex items-center justify-center shrink-0">
                        <User size={14} />
                    </div>
                    <div className="min-w-0">
                        <span className="text-xs font-bold text-theme-secondary group-hover:text-theme-primary truncate block transition-colors">
                            {displayName}
                        </span>
                    </div>
                    {!isOwn && <TrustBadge trust={trustDto} size="sm" binaryTrust />}
                </button>

                <div className="flex items-center gap-2 shrink-0 ml-auto">
                    {isOwn ? (
                        onEditOwnAd && (
                            <Button
                                size="xs"
                                variant="outline"
                                onClick={() => onEditOwnAd(ad)}
                                className="flex items-center gap-1.5"
                                data-testid="ad-edit-own-btn"
                            >
                                <Edit3 size={13} />
                                <span>{t('common.edit', 'Bearbeiten')}</span>
                            </Button>
                        )
                    ) : (
                        <>
                            {(ad.allowInquiries ?? allowInquiries ?? true) && onInquire && (
                                <Button
                                    size="xs"
                                    variant="outline"
                                    onClick={() => onInquire(ad, ownerDid, displayName)}
                                    className="flex items-center gap-1.5"
                                >
                                    <MessageSquare size={13} />
                                    <span>{t('marketplace.inquireBtn', 'Anfragen')}</span>
                                </Button>
                            )}
                            {onPay && (
                                <Button
                                    size="xs"
                                    variant="primary"
                                    onClick={() => onPay(ownerDid)}
                                    className="flex items-center gap-1"
                                >
                                    <span>{t('marketplace.payBtn', 'Bezahlen')}</span>
                                    <ArrowRight size={13} />
                                </Button>
                            )}
                        </>
                    )}
                </div>
            </div>
        </div>
    );
};
