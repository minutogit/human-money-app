// src/components/marketplace/AdEditorModal.tsx
import React, { useState, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { Tag, HandHeart, Search, Calendar, Upload, Trash2 } from 'lucide-react';
import { AdKind, FrontendAdInput, FrontendMyAd } from '../../types';
import { ModalShell } from '../ui/ModalShell';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { SegmentedTabs } from '../ui/SegmentedTabs';
import { marketService } from '../../services/marketService';
import { compressAdImageToWebpDataUri } from '../../utils/imageCompression';

export interface AdEditorModalProps {
    isOpen: boolean;
    onClose: () => void;
    initialAd?: FrontendMyAd | null;
    editingAd?: FrontendMyAd | null;
    activeAdsCount?: number;
    maxAds?: number;
    onSave: (input: FrontendAdInput) => Promise<void>;
    ownerDid?: string;
    mediaMaxBytes?: number;
    mediaMaxCount?: number;
    mediaUsedCount?: number;
}

export const AdEditorModal: React.FC<AdEditorModalProps> = ({
    isOpen,
    onClose,
    initialAd,
    editingAd,
    activeAdsCount,
    maxAds = 5,
    onSave,
    ownerDid,
    mediaMaxBytes,
    mediaMaxCount,
    mediaUsedCount,
}) => {
    const { t } = useTranslation();

    const targetAd = editingAd !== undefined ? editingAd : initialAd;
    const isEditMode = !!targetAd;
    const isLimitReached = !isEditMode && activeAdsCount !== undefined && activeAdsCount >= maxAds;

    const [kind, setKind] = useState<AdKind>('offer');
    const [title, setTitle] = useState('');
    const [text, setText] = useState('');
    const [pricing, setPricing] = useState('');
    const [durationDays, setDurationDays] = useState<number>(90);
    const [allowInquiries, setAllowInquiries] = useState<boolean>(true);
    const [isRemote, setIsRemote] = useState<boolean>(false);
    const [isSaving, setIsSaving] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const [imagePreviewUri, setImagePreviewUri] = useState<string | null>(null);
    const [imageWebpDataUri, setImageWebpDataUri] = useState<string | null | undefined>(undefined);
    const fileInputRef = useRef<HTMLInputElement | null>(null);

    useEffect(() => {
        if (targetAd) {
            setKind(targetAd.kind);
            setTitle(targetAd.title);
            setText(targetAd.text || '');
            setPricing(targetAd.pricing || '');
            setAllowInquiries(targetAd.allowInquiries);
            setIsRemote(targetAd.isRemote ?? (targetAd as any).is_remote ?? false);
            setError(null);
        } else {
            setKind('offer');
            setTitle('');
            setText('');
            setPricing('');
            setDurationDays(90);
            setAllowInquiries(true);
            setIsRemote(false);
            setError(null);
        }
    }, [targetAd, isOpen]);

    useEffect(() => {
        setImageWebpDataUri(undefined);
        const token = targetAd?.imageToken || (targetAd as any)?.image_token;
        if (isOpen && token) {
            let isMounted = true;
            marketService
                .getAdImage(ownerDid || '', token)
                .then((uri) => {
                    if (isMounted) setImagePreviewUri(uri);
                })
                .catch(() => {
                    if (isMounted) setImagePreviewUri(null);
                });
            return () => {
                isMounted = false;
            };
        } else {
            setImagePreviewUri(null);
        }
    }, [isOpen, targetAd, ownerDid]);

    const handleImageFileChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
        const file = e.target.files?.[0];
        if (!file) return;
        try {
            const maxBytes = mediaMaxBytes || 102400;
            const webpDataUri = await compressAdImageToWebpDataUri(file, maxBytes);
            setImagePreviewUri(webpDataUri);
            setImageWebpDataUri(webpDataUri);
        } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
        } finally {
            e.target.value = '';
        }
    };

    const handleRemoveImage = () => {
        setImagePreviewUri(null);
        setImageWebpDataUri(null);
    };

    const titleCharCount = title.trim().length;
    const textCharCount = text.trim().length;
    const pricingCharCount = pricing.trim().length;

    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault();
        setError(null);

        const trimmedTitle = title.trim();
        if (!trimmedTitle) {
            setError(t('marketplace.errorTitleRequired', 'Titel ist erforderlich'));
            return;
        }
        if (trimmedTitle.length > 80) {
            setError(t('marketplace.errorTitleTooLong', 'Titel darf maximal 80 Zeichen lang sein'));
            return;
        }
        if (text.trim().length > 280) {
            setError(t('marketplace.errorTextTooLong', 'Text darf maximal 280 Zeichen lang sein'));
            return;
        }
        if (pricing.trim().length > 40) {
            setError(t('marketplace.errorPricingTooLong', 'Preisangabe darf maximal 40 Zeichen lang sein'));
            return;
        }

        setIsSaving(true);
        try {
            await onSave({
                id: targetAd?.id,
                kind,
                title: trimmedTitle,
                text: text.trim() || undefined,
                pricing: pricing.trim() || undefined,
                durationDays,
                allowInquiries,
                isRemote,
                ...(imageWebpDataUri !== undefined ? { imageWebpDataUri } : {}),
            });
            onClose();
        } catch (err) {
            const msg = err instanceof Error ? err.message : String(err);
            if (msg.includes('market.adLimitReached')) {
                setError(t('marketplace.adLimitReached', 'Das Limit von 5 aktiven Inseraten ist erreicht.'));
            } else {
                setError(msg);
            }
        } finally {
            setIsSaving(false);
        }
    };

    const formatExpiryDate = (timestampSecs: number) => {
        return new Date(timestampSecs * 1000).toLocaleDateString();
    };

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={onClose}
            maxWidth="md"
            title={
                <div className="flex items-center gap-2.5">
                    <div className="w-9 h-9 rounded-xl bg-theme-primary/10 text-theme-primary flex items-center justify-center">
                        <Tag size={18} />
                    </div>
                    <div>
                        <h2 className="text-base font-bold text-theme-secondary">
                            {isEditMode
                                ? t('marketplace.editAdTitle', 'Inserat bearbeiten')
                                : t('marketplace.createAdTitle', 'Inserat aufgeben')}
                        </h2>
                        <p className="text-xs text-theme-light">
                            {t('marketplace.adModalSubtitle', 'Befristeter Eintrag auf der Pinnwand')}
                        </p>
                    </div>
                </div>
            }
        >
            <form onSubmit={handleSubmit} className="space-y-5 pt-2">
                {isLimitReached && (
                    <div className="text-xs p-3 rounded-xl bg-amber-50 text-amber-800 border border-amber-200">
                        {t('marketplace.adLimitReached', 'Du hast das Limit von {{limit}} aktiven Inseraten erreicht. Bitte lösche oder lasse ein altes Inserat auslaufen.', { limit: maxAds })}
                    </div>
                )}

                {error && (
                    <div className="text-xs p-3 rounded-xl bg-rose-50 text-rose-800 border border-rose-200">
                        {error}
                    </div>
                )}

                {/* 1. Kind selection: two large tiles */}
                <div className="space-y-1.5">
                    <label className="text-[11px] font-black text-theme-light uppercase tracking-wider block">
                        {t('marketplace.adKindLabel', 'Art des Inserats')}
                    </label>
                    <div className="grid grid-cols-2 gap-3">
                        <button
                            type="button"
                            onClick={() => setKind('offer')}
                            className={`p-3.5 rounded-2xl border text-left transition-all flex items-start gap-3 ${
                                kind === 'offer'
                                    ? 'bg-emerald-50/70 border-emerald-400 text-emerald-950 ring-2 ring-emerald-500/20'
                                    : 'bg-white border-theme-subtle text-theme-secondary hover:border-theme-primary/30'
                            }`}
                            data-testid="ad-kind-offer-btn"
                        >
                            <div className={`p-2 rounded-xl shrink-0 ${kind === 'offer' ? 'bg-emerald-200/60 text-emerald-800' : 'bg-bg-app text-theme-light'}`}>
                                <HandHeart size={18} />
                            </div>
                            <div className="min-w-0">
                                <div className="text-xs font-bold">{t('marketplace.adKindOffer', 'Angebot')}</div>
                                <div className="text-[10px] text-theme-light mt-0.5 leading-snug">
                                    {t('marketplace.adKindOfferDesc', 'Etwas anbieten, helfen oder teilen')}
                                </div>
                            </div>
                        </button>

                        <button
                            type="button"
                            onClick={() => setKind('request')}
                            className={`p-3.5 rounded-2xl border text-left transition-all flex items-start gap-3 ${
                                kind === 'request'
                                    ? 'bg-sky-50/70 border-sky-400 text-sky-950 ring-2 ring-sky-500/20'
                                    : 'bg-white border-theme-subtle text-theme-secondary hover:border-theme-primary/30'
                            }`}
                            data-testid="ad-kind-request-btn"
                        >
                            <div className={`p-2 rounded-xl shrink-0 ${kind === 'request' ? 'bg-sky-200/60 text-sky-800' : 'bg-bg-app text-theme-light'}`}>
                                <Search size={18} />
                            </div>
                            <div className="min-w-0">
                                <div className="text-xs font-bold">{t('marketplace.adKindRequest', 'Gesuch')}</div>
                                <div className="text-[10px] text-theme-light mt-0.5 leading-snug">
                                    {t('marketplace.adKindRequestDesc', 'Hilfe, Werkzeug oder Rat suchen')}
                                </div>
                            </div>
                        </button>
                    </div>
                </div>

                {/* 2. Title with counter */}
                <div className="space-y-1.5">
                    <div className="flex items-center justify-between">
                        <label className="text-[11px] font-black text-theme-light uppercase tracking-wider block">
                            {t('marketplace.adTitleLabel', 'Titel')}
                        </label>
                        <span className={`text-[10px] ${titleCharCount > 80 ? 'text-rose-600 font-bold' : 'text-theme-light'}`}>
                            {titleCharCount} / 80
                        </span>
                    </div>
                    <Input
                        value={title}
                        onChange={(e) => setTitle(e.target.value)}
                        placeholder={kind === 'offer'
                            ? t('marketplace.adTitlePlaceholderOffer', 'z. B. Frischer Honig aus eigenem Garten')
                            : t('marketplace.adTitlePlaceholderRequest', 'z. B. Suche Leiter für Samstag')}
                        maxLength={80}
                        required
                        data-testid="ad-title-input"
                    />
                </div>

                {/* 3. Text with counter */}
                <div className="space-y-1.5">
                    <div className="flex items-center justify-between">
                        <label className="text-[11px] font-black text-theme-light uppercase tracking-wider block">
                            {t('marketplace.adTextLabel', 'Beschreibung (optional)')}
                        </label>
                        <span className={`text-[10px] ${textCharCount > 280 ? 'text-rose-600 font-bold' : 'text-theme-light'}`}>
                            {textCharCount} / 280
                        </span>
                    </div>
                    <textarea
                        value={text}
                        onChange={(e) => setText(e.target.value)}
                        placeholder={t('marketplace.adTextPlaceholder', 'Beschreibe kurz und verständlich, worum es geht...')}
                        className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-3 text-xs font-medium text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft transition-all min-h-[90px] resize-y"
                        maxLength={280}
                        data-testid="ad-text-input"
                    />
                </div>

                {/* 4. Pricing with counter */}
                <div className="space-y-1.5">
                    <div className="flex items-center justify-between">
                        <label className="text-[11px] font-black text-theme-light uppercase tracking-wider block">
                            {t('marketplace.adPricingLabel', 'Preis / Richtwert (optional)')}
                        </label>
                        <span className={`text-[10px] ${pricingCharCount > 40 ? 'text-rose-600 font-bold' : 'text-theme-light'}`}>
                            {pricingCharCount} / 40
                        </span>
                    </div>
                    <Input
                        value={pricing}
                        onChange={(e) => setPricing(e.target.value)}
                        placeholder={t('marketplace.adPricingPlaceholder', 'z. B. 10 Minuto / 1 Std. oder VB')}
                        maxLength={40}
                        data-testid="ad-pricing-input"
                    />
                </div>

                {/* Image upload section (optional) */}
                <div className="space-y-1.5 pt-1">
                    <div className="flex items-center justify-between">
                        <label className="text-[11px] font-black text-theme-light uppercase tracking-wider block">
                            {t('marketplace.adImageLabel', 'Inseratsbild (optional)')}
                        </label>
                        <span className="text-[10px] font-semibold text-theme-light" data-testid="media-quota-badge">
                            {t('marketplace.mediaQuotaStatus', '{{used}} von {{max}} Bildern belegt', {
                                used: mediaUsedCount ?? 0,
                                max: mediaMaxCount ?? 5,
                            })}
                        </span>
                    </div>

                    <div className="p-3 rounded-2xl bg-bg-app/50 border border-theme-subtle space-y-3">
                        <input
                            ref={fileInputRef}
                            type="file"
                            accept="image/*"
                            data-testid="ad-image-file-input"
                            onChange={handleImageFileChange}
                            className="hidden"
                        />

                        {imagePreviewUri ? (
                            <div className="space-y-2">
                                <div className="rounded-xl overflow-hidden border border-theme-subtle max-h-48 bg-white flex items-center justify-center">
                                    <img
                                        src={imagePreviewUri}
                                        alt="Ad preview"
                                        className="w-full max-h-48 object-cover rounded-xl"
                                    />
                                </div>
                                <div className="flex items-center gap-2">
                                    <Button
                                        type="button"
                                        size="xs"
                                        variant="outline"
                                        onClick={() => fileInputRef.current?.click()}
                                        className="flex items-center gap-1.5"
                                    >
                                        <Upload size={13} />
                                        <span>{t('marketplace.adImageUpload', 'Bild auswählen')}</span>
                                    </Button>
                                    <Button
                                        type="button"
                                        size="xs"
                                        variant="secondary"
                                        data-testid="ad-image-remove-btn"
                                        onClick={handleRemoveImage}
                                        className="flex items-center gap-1.5"
                                    >
                                        <Trash2 size={13} />
                                        <span>{t('marketplace.adImageRemove', 'Bild entfernen')}</span>
                                    </Button>
                                </div>
                            </div>
                        ) : (
                            <div className="flex items-center gap-3">
                                <Button
                                    type="button"
                                    size="xs"
                                    variant="outline"
                                    onClick={() => fileInputRef.current?.click()}
                                    className="flex items-center gap-1.5"
                                >
                                    <Upload size={13} />
                                    <span>{t('marketplace.adImageUpload', 'Bild auswählen')}</span>
                                </Button>
                                <span className="text-[10px] text-theme-light leading-snug">
                                    {t('marketplace.adImageHint', 'Max. 800×800 Pixel, WebP komprimiert (max. 100 KB).')}
                                </span>
                            </div>
                        )}
                    </div>
                </div>

                {/* 5. Duration: Segmented control if creating, info text if editing */}
                <div className="space-y-1.5">
                    <label className="text-[11px] font-black text-theme-light uppercase tracking-wider block">
                        {t('marketplace.adDurationLabel', 'Laufzeit')}
                    </label>
                    {!isEditMode ? (
                        <div className="space-y-2">
                            <SegmentedTabs<string>
                                fullWidth
                                activeTab={String(durationDays)}
                                onChange={(tab) => setDurationDays(Number(tab))}
                                tabs={[
                                    { id: '30', label: t('marketplace.duration30Days', '30 Tage') },
                                    { id: '90', label: t('marketplace.duration90Days', '90 Tage (Standard)') },
                                    { id: '180', label: t('marketplace.duration180Days', '180 Tage') },
                                ]}
                            />
                            <p className="text-[11px] text-theme-light leading-snug">
                                {t('marketplace.adDurationHint', 'Du kannst dein Inserat jederzeit vor oder nach Ablauf beliebig oft verlängern.')}
                            </p>
                        </div>
                    ) : (
                        <div className="p-3 rounded-xl bg-bg-app border border-theme-subtle flex items-center gap-2 text-xs text-theme-secondary">
                            <Calendar size={15} className="text-theme-primary shrink-0" />
                            <span>
                                {t('marketplace.adEditingDurationInfo', 'Läuft bis {{date}} (Verlängern über „Verlängern“)', {
                                    date: formatExpiryDate(targetAd?.expiresAt || 0),
                                })}
                            </span>
                        </div>
                    )}
                </div>

                {/* 6. Allow inquiries toggle */}
                <div className="pt-1">
                    <label className="flex items-start gap-3 p-3 rounded-xl bg-bg-app/50 border border-theme-subtle cursor-pointer hover:bg-bg-app transition-colors">
                        <input
                            type="checkbox"
                            checked={allowInquiries}
                            onChange={(e) => setAllowInquiries(e.target.checked)}
                            className="mt-0.5 rounded text-theme-primary focus:ring-theme-primary"
                        />
                        <div className="min-w-0">
                            <span className="text-xs font-bold text-theme-secondary block">
                                {t('marketplace.allowInquiriesLabel', 'Anfragen erlauben')}
                            </span>
                            <span className="text-[11px] text-theme-light block mt-0.5 leading-snug">
                                {t('marketplace.allowInquiriesDesc', 'Andere können dir über ihren Sync-Dienst eine Nachricht oder Reservierungsanfrage zu diesem Inserat senden.')}
                            </span>
                        </div>
                    </label>
                </div>

                {/* 7. Is remote toggle */}
                <div className="pt-1">
                    <label className="flex items-start gap-3 p-3 rounded-xl bg-bg-app/50 border border-theme-subtle cursor-pointer hover:bg-bg-app transition-colors">
                        <input
                            type="checkbox"
                            checked={isRemote}
                            onChange={(e) => setIsRemote(e.target.checked)}
                            className="mt-0.5 rounded text-theme-primary focus:ring-theme-primary"
                            data-testid="ad-is-remote-checkbox"
                        />
                        <div className="min-w-0">
                            <span className="text-xs font-bold text-theme-secondary block">
                                {t('marketplace.adIsRemoteLabel', 'Ortsunabhängig / Online / Versand (überregional sichtbar)')}
                            </span>
                            <span className="text-[11px] text-theme-light block mt-0.5 leading-snug">
                                {t('marketplace.adIsRemoteDesc', 'Dieses Inserat wird auch bei regionaler Filterung als ortsunabhängig angezeigt.')}
                            </span>
                        </div>
                    </label>
                </div>

                {/* Footer buttons */}
                <div className="flex items-center justify-end gap-2 pt-3 border-t border-theme-subtle/50">
                    <Button variant="secondary" type="button" onClick={onClose} disabled={isSaving}>
                        {t('common.cancel', 'Abbrechen')}
                    </Button>
                    <Button variant="primary" type="submit" data-testid="ad-submit-button" disabled={isSaving || !title.trim() || isLimitReached}>
                        {isSaving ? t('common.saving', 'Speichern...') : t('common.save', 'Speichern')}
                    </Button>
                </div>
            </form>
        </ModalShell>
    );
};
