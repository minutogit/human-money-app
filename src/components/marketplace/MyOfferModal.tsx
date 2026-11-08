// src/components/marketplace/MyOfferModal.tsx
import { useState, useEffect, useMemo, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { 
    Tag, 
    Plus, 
    Trash2, 
    Share2, 
    Save, 
    AlertCircle, 
    Info, 
    Eye, 
    Edit3, 
    Clock, 
    Network,
    ArrowUp,
    ArrowDown,
    Building2,
    Users,
    Mail,
    Phone,
    Globe,
    Smartphone
} from 'lucide-react';
import { 
    FrontendOwnContactCard, 
    FrontendCardListingInput, 
    FrontendListingType,
    FrontendUserProfile,
    FrontendSaveOwnCardRequest
} from '../../types';
import { marketService } from '../../services/marketService';
import { profileService } from '../../services/profileService';
import { useSession } from '../../context/SessionContext';
import { ModalShell } from '../ui/ModalShell';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { Textarea } from '../ui/Textarea';
import { Card } from '../ui/Card';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { ShareCardModal } from './ShareCardModal';
import { translateError } from '../../utils/errorHelper';
import { logger } from '../../utils/log';

export interface MyOfferModalProps {
    isOpen: boolean;
    onClose: () => void;
    onCardUpdated?: () => void;
}

const MAX_LISTINGS_PER_CARD = 10;
const MAX_TITLE_CHARS = 80;
const MAX_TEXT_CHARS = 280;
const MAX_PRICING_CHARS = 40;
const DEFAULT_VALIDITY_DAYS = 60;

export function MyOfferModal({ isOpen, onClose, onCardUpdated }: MyOfferModalProps) {
    const { t } = useTranslation();
    const { protectAction } = useSession();

    const [isLoading, setIsLoading] = useState(true);
    const [isSaving, setIsSaving] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [successMsg, setSuccessMsg] = useState<string | null>(null);

    // Card data
    const [ownCard, setOwnCard] = useState<FrontendOwnContactCard | null>(null);
    const [profile, setProfile] = useState<FrontendUserProfile>({ id: '' });
    const [listings, setListings] = useState<FrontendCardListingInput[]>([]);
    const [maxHops, setMaxHops] = useState<number>(2);
    const [validityDays, setValidityDays] = useState<number>(DEFAULT_VALIDITY_DAYS);

    // Active tab in editor: 'edit' or 'preview'
    const [viewMode, setViewMode] = useState<'edit' | 'preview'>('edit');

    // Modals
    const [isShareModalOpen, setIsShareModalOpen] = useState(false);
    const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
    const [isDeleting, setIsDeleting] = useState(false);

    // Load initial data
    const loadCardData = useCallback(async () => {
        setIsLoading(true);
        setError(null);
        try {
            const card = await marketService.getOwnContactCard();
            if (card) {
                setOwnCard(card);
                setProfile(card.card.profile || { id: card.card.did });
                setListings(card.card.listings || []);
                setMaxHops(card.card.maxHops || 2);
                // Calculate remaining days from validUntil
                const nowSecs = Math.floor(Date.now() / 1000);
                if (card.card.validUntil > nowSecs) {
                    const diffDays = Math.ceil((card.card.validUntil - nowSecs) / 86400);
                    setValidityDays(Math.max(1, diffDays));
                }
            } else {
                // Prefill from wallet profile
                setOwnCard(null);
                try {
                    const walletProfile = await profileService.getProfile();
                    if (walletProfile) {
                        setProfile({
                            ...walletProfile,
                            id: walletProfile.id || '',
                        });
                    }
                } catch (e) {
                    logger.warn('Failed to prefill wallet profile: ' + String(e));
                }
                setListings([]);
                setMaxHops(2);
                setValidityDays(DEFAULT_VALIDITY_DAYS);
            }
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsLoading(false);
        }
    }, [t]);

    useEffect(() => {
        if (isOpen) {
            loadCardData();
            setViewMode('edit');
            setSuccessMsg(null);
        }
    }, [isOpen, loadCardData]);

    // Validation for listings
    const validationErrors = useMemo(() => {
        const errors: Record<number, { title?: string; text?: string; pricing?: string }> = {};
        listings.forEach((item, index) => {
            const itemErrors: { title?: string; text?: string; pricing?: string } = {};
            if (!item.title.trim()) {
                itemErrors.title = t('marketplace.errorTitleRequired');
            } else if (item.title.length > MAX_TITLE_CHARS) {
                itemErrors.title = t('marketplace.errorTitleTooLong', { max: MAX_TITLE_CHARS });
            }
            if (item.text && item.text.length > MAX_TEXT_CHARS) {
                itemErrors.text = t('marketplace.errorTextTooLong', { max: MAX_TEXT_CHARS });
            }
            if (item.pricing && item.pricing.length > MAX_PRICING_CHARS) {
                itemErrors.pricing = t('marketplace.errorPricingTooLong', { max: MAX_PRICING_CHARS });
            }
            if (Object.keys(itemErrors).length > 0) {
                errors[index] = itemErrors;
            }
        });
        return errors;
    }, [listings, t]);

    const hasValidationErrors = Object.keys(validationErrors).length > 0;

    // Listing mutations
    const handleAddListing = (type: FrontendListingType) => {
        if (listings.length >= MAX_LISTINGS_PER_CARD) return;
        setListings([
            ...listings,
            {
                listingType: type,
                title: '',
                text: '',
                pricing: '',
                allowInquiries: true,
            },
        ]);
    };

    const handleUpdateListing = (index: number, updates: Partial<FrontendCardListingInput>) => {
        setListings(
            listings.map((item, idx) => (idx === index ? { ...item, ...updates } : item))
        );
    };

    const handleRemoveListing = (index: number) => {
        setListings(listings.filter((_, idx) => idx !== index));
    };

    const handleMoveListing = (index: number, direction: 'up' | 'down') => {
        const targetIndex = direction === 'up' ? index - 1 : index + 1;
        if (targetIndex < 0 || targetIndex >= listings.length) return;
        const updated = [...listings];
        const [moved] = updated.splice(index, 1);
        updated.splice(targetIndex, 0, moved);
        setListings(updated);
    };

    // Calculate expiration date
    const expirationDateStr = useMemo(() => {
        const exp = new Date(Date.now() + validityDays * 86400 * 1000);
        return exp.toLocaleDateString();
    }, [validityDays]);

    // Save action
    const handleSave = async () => {
        if (hasValidationErrors) return;
        setIsSaving(true);
        setError(null);
        setSuccessMsg(null);
        try {
            const request: FrontendSaveOwnCardRequest = {
                profile,
                listings,
                maxHops,
                validityDays,
            };

            const saved = await protectAction(async (pwd) => {
                return await marketService.saveOwnContactCard(request, pwd || undefined);
            });

            if (saved) {
                setOwnCard(saved);
                setSuccessMsg(t('marketplace.cardSavedSuccess'));
                if (onCardUpdated) {
                    onCardUpdated();
                }
                marketService.syncMarketDrops({ forceUpload: true }).catch((e) => {
                    logger.warn('Non-blocking drop upload on card save error: ' + String(e));
                });
            }
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsSaving(false);
        }
    };

    // Delete action
    const handleDelete = async () => {
        setIsDeleting(true);
        try {
            await protectAction(async (pwd) => {
                await marketService.deleteOwnContactCard(pwd || undefined);
            });
            setOwnCard(null);
            setListings([]);
            setIsDeleteConfirmOpen(false);
            setSuccessMsg(t('marketplace.cardDeletedSuccess'));
            if (onCardUpdated) {
                onCardUpdated();
            }
            marketService.deleteOwnDropSlots().catch((e) => {
                logger.warn('Non-blocking drop slot delete error: ' + String(e));
            });
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsDeleting(false);
        }
    };

    const displayName = [profile.firstName, profile.lastName].filter(Boolean).join(' ') ||
        profile.organization || profile.community || t('marketplace.yourCardPreview');

    return (
        <>
            <ModalShell
                isOpen={isOpen}
                onClose={onClose}
                maxWidth="2xl"
                title={
                    <div className="flex items-center justify-between w-full pr-6">
                        <div className="flex items-center gap-2.5">
                            <div className="w-9 h-9 rounded-xl bg-theme-primary/10 text-theme-primary flex items-center justify-center">
                                <Tag size={18} />
                            </div>
                            <div>
                                <h2 className="text-base font-bold text-theme-secondary">
                                    {t('marketplace.myOfferTitle')}
                                </h2>
                                <p className="text-xs text-theme-light">
                                    {t('marketplace.myOfferSubtitle')}
                                </p>
                            </div>
                        </div>

                        {/* Switch edit / preview mode */}
                        <div className="flex items-center bg-bg-app border border-theme-subtle rounded-xl p-1 text-xs">
                            <button
                                type="button"
                                onClick={() => setViewMode('edit')}
                                className={`px-2.5 py-1 rounded-lg font-semibold flex items-center gap-1.5 cursor-pointer ${
                                    viewMode === 'edit'
                                        ? 'bg-white text-theme-primary shadow-xs'
                                        : 'text-theme-light hover:text-theme-secondary'
                                }`}
                            >
                                <Edit3 size={13} />
                                <span>{t('marketplace.modeEdit')}</span>
                            </button>
                            <button
                                type="button"
                                onClick={() => setViewMode('preview')}
                                className={`px-2.5 py-1 rounded-lg font-semibold flex items-center gap-1.5 cursor-pointer ${
                                    viewMode === 'preview'
                                        ? 'bg-white text-theme-primary shadow-xs'
                                        : 'text-theme-light hover:text-theme-secondary'
                                }`}
                            >
                                <Eye size={13} />
                                <span>{t('marketplace.modePreview')}</span>
                            </button>
                        </div>
                    </div>
                }
                footer={
                    <div className="flex items-center justify-between w-full gap-3">
                        <div>
                            {ownCard && (
                                <Button
                                    variant="danger"
                                    size="sm"
                                    onClick={() => setIsDeleteConfirmOpen(true)}
                                    disabled={isSaving}
                                    className="gap-1.5"
                                >
                                    <Trash2 size={14} />
                                    <span>{t('marketplace.deleteCardBtn')}</span>
                                </Button>
                            )}
                        </div>

                        <div className="flex items-center gap-2">
                            {ownCard && (
                                <Button
                                    variant="outline"
                                    size="sm"
                                    onClick={() => setIsShareModalOpen(true)}
                                    className="gap-1.5"
                                >
                                    <Share2 size={14} />
                                    <span>{t('marketplace.shareCardBtn')}</span>
                                </Button>
                            )}

                            <Button
                                variant="primary"
                                size="sm"
                                onClick={handleSave}
                                disabled={isSaving || hasValidationErrors}
                                className="gap-1.5 shadow-sm"
                            >
                                <Save size={14} />
                                <span>{isSaving ? t('common.processing') : t('marketplace.saveCardBtn')}</span>
                            </Button>
                        </div>
                    </div>
                }
            >
                <div className="space-y-6 pt-2">
                    {/* Multi-Device Hint */}
                    <div className="p-3 bg-amber-50/70 border border-amber-200/80 rounded-2xl flex items-start gap-3 text-xs text-amber-900">
                        <Smartphone size={16} className="text-amber-700 shrink-0 mt-0.5" />
                        <div className="leading-relaxed">
                            <span className="font-bold">{t('marketplace.multiDeviceHintTitle')}: </span>
                            {t('marketplace.multiDeviceHintDesc')}
                        </div>
                    </div>

                    {/* Feedback Messages */}
                    {error && (
                        <div className="p-3 bg-rose-50 border border-rose-200 rounded-xl text-xs text-rose-800 flex items-center gap-2">
                            <AlertCircle size={15} className="shrink-0" />
                            <span>{error}</span>
                        </div>
                    )}
                    {successMsg && (
                        <div className="p-3 bg-emerald-50 border border-emerald-200 rounded-xl text-xs text-emerald-800 flex items-center gap-2">
                            <Info size={15} className="shrink-0" />
                            <span>{successMsg}</span>
                        </div>
                    )}

                    {isLoading ? (
                        <div className="py-12 text-center text-xs text-theme-light">
                            {t('common.loading')}
                        </div>
                    ) : viewMode === 'edit' ? (
                        /* EDIT MODE */
                        <div className="space-y-6">
                            {/* Profile Information Section */}
                            <div className="space-y-3">
                                <div className="flex items-center justify-between">
                                    <h3 className="text-xs font-black text-theme-light uppercase tracking-wider flex items-center gap-1.5">
                                        <Users size={13} />
                                        <span>{t('marketplace.cardProfileHeader')}</span>
                                    </h3>
                                    <span className="text-[10px] text-theme-light/70">
                                        {t('marketplace.cardProfileSubtitle')}
                                    </span>
                                </div>

                                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 p-4 bg-bg-app rounded-2xl border border-theme-subtle">
                                    <div>
                                        <label className="block text-[11px] font-bold text-theme-secondary mb-1">
                                            {t('profile.firstName')}
                                        </label>
                                        <Input
                                            value={profile.firstName || ''}
                                            onChange={(e) => setProfile({ ...profile, firstName: e.target.value })}
                                            placeholder={t('profile.firstNamePlaceholder')}
                                            className="h-9 text-xs bg-white"
                                        />
                                    </div>

                                    <div>
                                        <label className="block text-[11px] font-bold text-theme-secondary mb-1">
                                            {t('profile.lastName')}
                                        </label>
                                        <Input
                                            value={profile.lastName || ''}
                                            onChange={(e) => setProfile({ ...profile, lastName: e.target.value })}
                                            placeholder={t('profile.lastNamePlaceholder')}
                                            className="h-9 text-xs bg-white"
                                        />
                                    </div>

                                    <div>
                                        <label className="block text-[11px] font-bold text-theme-secondary mb-1">
                                            {t('profile.field.organization')}
                                        </label>
                                        <Input
                                            value={profile.organization || ''}
                                            onChange={(e) => setProfile({ ...profile, organization: e.target.value })}
                                            placeholder={t('profile.organizationPlaceholder')}
                                            className="h-9 text-xs bg-white"
                                        />
                                    </div>

                                    <div>
                                        <label className="block text-[11px] font-bold text-theme-secondary mb-1">
                                            {t('profile.community')}
                                        </label>
                                        <Input
                                            value={profile.community || ''}
                                            onChange={(e) => setProfile({ ...profile, community: e.target.value })}
                                            placeholder={t('profile.communityPlaceholder')}
                                            className="h-9 text-xs bg-white"
                                        />
                                    </div>

                                    <div>
                                        <label className="block text-[11px] font-bold text-theme-secondary mb-1">
                                            {t('profile.field.email')}
                                        </label>
                                        <Input
                                            type="email"
                                            value={profile.email || ''}
                                            onChange={(e) => setProfile({ ...profile, email: e.target.value })}
                                            placeholder="kontakt@beispiel.de"
                                            className="h-9 text-xs bg-white"
                                        />
                                    </div>

                                    <div>
                                        <label className="block text-[11px] font-bold text-theme-secondary mb-1">
                                            {t('profile.field.phone')}
                                        </label>
                                        <Input
                                            type="tel"
                                            value={profile.phone || ''}
                                            onChange={(e) => setProfile({ ...profile, phone: e.target.value })}
                                            placeholder="+49 123 456789"
                                            className="h-9 text-xs bg-white"
                                        />
                                    </div>

                                    <div className="sm:col-span-2">
                                        <label className="block text-[11px] font-bold text-theme-secondary mb-1">
                                            {t('profile.website')}
                                        </label>
                                        <Input
                                            type="url"
                                            value={profile.url || ''}
                                            onChange={(e) => setProfile({ ...profile, url: e.target.value })}
                                            placeholder="https://meine-initiative.org"
                                            className="h-9 text-xs bg-white"
                                        />
                                    </div>
                                </div>
                            </div>

                            {/* Inserate / Listings Section */}
                            <div className="space-y-3">
                                <div className="flex items-center justify-between">
                                    <h3 className="text-xs font-black text-theme-light uppercase tracking-wider flex items-center gap-1.5">
                                        <Tag size={13} />
                                        <span>{t('marketplace.listingsHeader', { current: listings.length, max: MAX_LISTINGS_PER_CARD })}</span>
                                    </h3>
                                    <div className="flex items-center gap-2">
                                        <Button
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            onClick={() => handleAddListing('offer')}
                                            disabled={listings.length >= MAX_LISTINGS_PER_CARD}
                                            className="h-7 text-[11px] gap-1 bg-emerald-50 text-emerald-800 border-emerald-200 hover:bg-emerald-100"
                                        >
                                            <Plus size={12} />
                                            <span>{t('marketplace.addOfferBtn')}</span>
                                        </Button>
                                        <Button
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            onClick={() => handleAddListing('request')}
                                            disabled={listings.length >= MAX_LISTINGS_PER_CARD}
                                            className="h-7 text-[11px] gap-1 bg-amber-50 text-amber-800 border-amber-200 hover:bg-amber-100"
                                        >
                                            <Plus size={12} />
                                            <span>{t('marketplace.addRequestBtn')}</span>
                                        </Button>
                                    </div>
                                </div>

                                {listings.length === 0 ? (
                                    <div className="p-6 text-center bg-bg-app border border-theme-subtle rounded-2xl text-xs text-theme-light">
                                        {t('marketplace.noOwnListingsYet')}
                                    </div>
                                ) : (
                                    <div className="space-y-3">
                                        {listings.map((item, idx) => {
                                            const itemError = validationErrors[idx];
                                            const isOffer = item.listingType === 'offer';

                                            return (
                                                <Card
                                                    key={idx}
                                                    className="p-4 bg-white border border-theme-subtle rounded-2xl shadow-xs space-y-3"
                                                >
                                                    <div className="flex items-center justify-between gap-3">
                                                        {/* Type Toggle */}
                                                        <div className="flex items-center gap-1 bg-bg-app p-1 rounded-xl border border-theme-subtle text-xs">
                                                            <button
                                                                type="button"
                                                                onClick={() => handleUpdateListing(idx, { listingType: 'offer' })}
                                                                className={`px-2 py-0.5 rounded-lg font-bold text-[11px] cursor-pointer ${
                                                                    isOffer
                                                                        ? 'bg-emerald-600 text-white shadow-xs'
                                                                        : 'text-theme-light hover:text-theme-secondary'
                                                                }`}
                                                            >
                                                                {t('marketplace.tabOffers')}
                                                            </button>
                                                            <button
                                                                type="button"
                                                                onClick={() => handleUpdateListing(idx, { listingType: 'request' })}
                                                                className={`px-2 py-0.5 rounded-lg font-bold text-[11px] cursor-pointer ${
                                                                    !isOffer
                                                                        ? 'bg-amber-600 text-white shadow-xs'
                                                                        : 'text-theme-light hover:text-theme-secondary'
                                                                }`}
                                                            >
                                                                {t('marketplace.tabRequests')}
                                                            </button>
                                                        </div>

                                                        {/* Actions: Reorder & Remove */}
                                                        <div className="flex items-center gap-1">
                                                            <button
                                                                type="button"
                                                                onClick={() => handleMoveListing(idx, 'up')}
                                                                disabled={idx === 0}
                                                                className="p-1 text-theme-light hover:text-theme-secondary disabled:opacity-30 cursor-pointer"
                                                                title={t('marketplace.moveUpBtn')}
                                                            >
                                                                <ArrowUp size={14} />
                                                            </button>
                                                            <button
                                                                type="button"
                                                                onClick={() => handleMoveListing(idx, 'down')}
                                                                disabled={idx === listings.length - 1}
                                                                className="p-1 text-theme-light hover:text-theme-secondary disabled:opacity-30 cursor-pointer"
                                                                title={t('marketplace.moveDownBtn')}
                                                            >
                                                                <ArrowDown size={14} />
                                                            </button>
                                                            <button
                                                                type="button"
                                                                onClick={() => handleRemoveListing(idx)}
                                                                className="p-1 text-rose-500 hover:text-rose-700 cursor-pointer ml-1"
                                                                title={t('marketplace.deleteListingBtn')}
                                                            >
                                                                <Trash2 size={14} />
                                                            </button>
                                                        </div>
                                                    </div>

                                                    {/* Title & Pricing */}
                                                    <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
                                                        <div className="sm:col-span-2">
                                                            <div className="flex items-center justify-between mb-1">
                                                                <label className="text-[11px] font-bold text-theme-secondary">
                                                                    {t('marketplace.listingTitleLabel')} *
                                                                </label>
                                                                <span className={`text-[10px] ${item.title.length > MAX_TITLE_CHARS ? 'text-rose-600 font-bold' : 'text-theme-light'}`}>
                                                                    {item.title.length} / {MAX_TITLE_CHARS}
                                                                </span>
                                                            </div>
                                                            <Input
                                                                value={item.title}
                                                                onChange={(e) => handleUpdateListing(idx, { title: e.target.value })}
                                                                placeholder={t('marketplace.listingTitlePlaceholder')}
                                                                className={`h-9 text-xs ${itemError?.title ? 'border-rose-300 ring-rose-200' : ''}`}
                                                            />
                                                            {itemError?.title && (
                                                                <p className="text-[10px] text-rose-600 mt-1 font-medium">
                                                                    {itemError.title}
                                                                </p>
                                                            )}
                                                        </div>

                                                        <div>
                                                            <div className="flex items-center justify-between mb-1">
                                                                <label className="text-[11px] font-bold text-theme-secondary">
                                                                    {t('marketplace.listingPricingLabel')}
                                                                </label>
                                                                <span className={`text-[10px] ${(item.pricing?.length || 0) > MAX_PRICING_CHARS ? 'text-rose-600 font-bold' : 'text-theme-light'}`}>
                                                                    {item.pricing?.length || 0} / {MAX_PRICING_CHARS}
                                                                </span>
                                                            </div>
                                                            <Input
                                                                value={item.pricing || ''}
                                                                onChange={(e) => handleUpdateListing(idx, { pricing: e.target.value })}
                                                                placeholder={t('marketplace.listingPricingPlaceholder')}
                                                                className={`h-9 text-xs ${itemError?.pricing ? 'border-rose-300 ring-rose-200' : ''}`}
                                                            />
                                                            {itemError?.pricing && (
                                                                <p className="text-[10px] text-rose-600 mt-1 font-medium">
                                                                    {itemError.pricing}
                                                                </p>
                                                            )}
                                                        </div>
                                                    </div>

                                                    {/* Description */}
                                                    <div>
                                                        <div className="flex items-center justify-between mb-1">
                                                            <label className="text-[11px] font-bold text-theme-secondary">
                                                                {t('marketplace.listingTextLabel')}
                                                            </label>
                                                            <span className={`text-[10px] ${(item.text?.length || 0) > MAX_TEXT_CHARS ? 'text-rose-600 font-bold' : 'text-theme-light'}`}>
                                                                {item.text?.length || 0} / {MAX_TEXT_CHARS}
                                                            </span>
                                                        </div>
                                                        <Textarea
                                                            value={item.text || ''}
                                                            onChange={(e) => handleUpdateListing(idx, { text: e.target.value })}
                                                            placeholder={t('marketplace.listingTextPlaceholder')}
                                                            rows={2}
                                                            className={`text-xs ${itemError?.text ? 'border-rose-300 ring-rose-200' : ''}`}
                                                        />
                                                        {itemError?.text && (
                                                            <p className="text-[10px] text-rose-600 mt-1 font-medium">
                                                                {itemError.text}
                                                            </p>
                                                        )}
                                                    </div>

                                                    {/* Allow Inquiries Checkbox (§6.4.3) */}
                                                    <div className="flex items-center gap-2 pt-2 border-t border-theme-subtle/50">
                                                        <input
                                                            type="checkbox"
                                                            id={`listing-allow-inquiries-${idx}`}
                                                            checked={item.allowInquiries ?? true}
                                                            onChange={(e) => handleUpdateListing(idx, { allowInquiries: e.target.checked })}
                                                            className="rounded accent-theme-primary h-4 w-4"
                                                            data-testid={`allow-inquiries-checkbox-${idx}`}
                                                        />
                                                        <label htmlFor={`listing-allow-inquiries-${idx}`} className="text-xs font-semibold text-theme-secondary cursor-pointer">
                                                            {t('marketplace.allowInquiriesLabel', 'Anfragen über Ablage erlauben')}
                                                        </label>
                                                    </div>
                                                </Card>
                                            );
                                        })}
                                    </div>
                                )}
                            </div>

                            {/* Settings: Max Hops & Validity */}
                            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 p-4 bg-bg-app rounded-2xl border border-theme-subtle">
                                {/* Max Hops Selector */}
                                <div className="space-y-1.5">
                                    <label className="block text-[11px] font-bold text-theme-secondary flex items-center gap-1.5">
                                        <Network size={13} className="text-theme-primary" />
                                        <span>{t('marketplace.maxHopsLabel')}</span>
                                    </label>
                                    <select
                                        aria-label={t('marketplace.maxHopsLabel')}
                                        value={maxHops}
                                        onChange={(e) => setMaxHops(Number(e.target.value))}
                                        className="w-full h-10 px-3 bg-white border border-theme-subtle rounded-xl text-xs font-medium text-theme-secondary outline-none focus:ring-2 focus:ring-theme-primary/20 cursor-pointer"
                                    >
                                        <option value={1}>{t('marketplace.maxHops1')}</option>
                                        <option value={2}>{t('marketplace.maxHops2')}</option>
                                        <option value={3}>{t('marketplace.maxHops3')}</option>
                                    </select>
                                    <p className="text-[10px] text-theme-light">
                                        {t('marketplace.maxHopsDesc')}
                                    </p>
                                </div>

                                {/* Validity Selector */}
                                <div className="space-y-1.5">
                                    <label className="block text-[11px] font-bold text-theme-secondary flex items-center gap-1.5">
                                        <Clock size={13} className="text-theme-primary" />
                                        <span>{t('marketplace.validityLabel')}</span>
                                    </label>
                                    <select
                                        aria-label={t('marketplace.validityLabel')}
                                        value={validityDays}
                                        onChange={(e) => setValidityDays(Number(e.target.value))}
                                        className="w-full h-10 px-3 bg-white border border-theme-subtle rounded-xl text-xs font-medium text-theme-secondary outline-none focus:ring-2 focus:ring-theme-primary/20 cursor-pointer"
                                    >
                                        <option value={14}>{t('marketplace.validityDays14')}</option>
                                        <option value={30}>{t('marketplace.validityDays30')}</option>
                                        <option value={60}>{t('marketplace.validityDays60')}</option>
                                        <option value={90}>{t('marketplace.validityDays90')}</option>
                                        <option value={180}>{t('marketplace.validityDays180')}</option>
                                    </select>
                                    <p className="text-[10px] text-theme-light">
                                        {t('marketplace.validUntilInfo', { date: expirationDateStr })}
                                    </p>
                                </div>
                            </div>
                        </div>
                    ) : (
                        /* PREVIEW MODE */
                        <div className="space-y-4">
                            <div className="p-4 bg-emerald-50/50 border border-emerald-200/60 rounded-2xl text-xs text-emerald-900 flex items-center gap-2">
                                <Eye size={15} className="shrink-0 text-emerald-700" />
                                <span>{t('marketplace.previewBannerDesc')}</span>
                            </div>

                            {/* Preview Card */}
                            <Card className="p-6 bg-white border border-theme-subtle rounded-3xl shadow-sm space-y-4">
                                <div className="flex items-start justify-between gap-4">
                                    <div>
                                        <h3 className="text-lg font-black text-theme-secondary">
                                            {displayName}
                                        </h3>
                                        {(profile.organization || profile.community) && (
                                            <div className="flex flex-wrap items-center gap-3 text-xs text-theme-light pt-0.5">
                                                {profile.organization && (
                                                    <span className="flex items-center gap-1">
                                                        <Building2 size={12} />
                                                        {profile.organization}
                                                    </span>
                                                )}
                                                {profile.community && (
                                                    <span className="flex items-center gap-1">
                                                        <Users size={12} />
                                                        {profile.community}
                                                    </span>
                                                )}
                                            </div>
                                        )}
                                    </div>

                                    <span className="px-2.5 py-1 rounded-full text-[11px] font-bold bg-indigo-50 text-indigo-800 border border-indigo-200">
                                        {t('marketplace.sourceCard', { hops: 1 })}
                                    </span>
                                </div>

                                {/* Preview Listings */}
                                {listings.length === 0 ? (
                                    <div className="p-6 text-center text-xs text-theme-light italic border border-dashed border-theme-subtle rounded-2xl">
                                        {t('marketplace.noListings')}
                                    </div>
                                ) : (
                                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-2">
                                        {listings.map((l, i) => (
                                            <div
                                                key={i}
                                                className="p-3.5 bg-bg-app rounded-2xl border border-theme-subtle/70 space-y-1.5"
                                            >
                                                <div className="flex items-start justify-between gap-2">
                                                    <span className={`text-[10px] font-extrabold px-1.5 py-0.5 rounded-md ${
                                                        l.listingType === 'offer'
                                                            ? 'bg-emerald-100 text-emerald-800'
                                                            : 'bg-amber-100 text-amber-800'
                                                    }`}>
                                                        {l.listingType === 'offer' ? t('marketplace.tabOffers') : t('marketplace.tabRequests')}
                                                    </span>
                                                    {l.pricing && (
                                                        <span className="text-xs font-bold text-theme-secondary font-mono">
                                                            {l.pricing}
                                                        </span>
                                                    )}
                                                </div>
                                                <h4 className="text-xs font-bold text-theme-secondary">
                                                    {l.title || t('marketplace.untitled')}
                                                </h4>
                                                {l.text && (
                                                    <p className="text-[11px] text-theme-light whitespace-pre-wrap leading-relaxed">
                                                        {l.text}
                                                    </p>
                                                )}
                                            </div>
                                        ))}
                                    </div>
                                )}

                                {/* Contact previews */}
                                {(profile.email || profile.phone || profile.url) && (
                                    <div className="pt-3 border-t border-theme-subtle/50 flex flex-wrap gap-3 text-xs text-theme-light">
                                        {profile.email && (
                                            <span className="flex items-center gap-1.5">
                                                <Mail size={13} />
                                                {profile.email}
                                            </span>
                                        )}
                                        {profile.phone && (
                                            <span className="flex items-center gap-1.5">
                                                <Phone size={13} />
                                                {profile.phone}
                                            </span>
                                        )}
                                        {profile.url && (
                                            <span className="flex items-center gap-1.5">
                                                <Globe size={13} />
                                                {profile.url}
                                            </span>
                                        )}
                                    </div>
                                )}
                            </Card>
                        </div>
                    )}
                </div>
            </ModalShell>

            {/* Share Modal */}
            {ownCard && (
                <ShareCardModal
                    isOpen={isShareModalOpen}
                    onClose={() => setIsShareModalOpen(false)}
                    title={t('marketplace.shareOwnCardTitle')}
                    subtitle={t('marketplace.shareOwnCardSubtitle')}
                    envelopeJson={ownCard.envelopeJson}
                    shareUri={ownCard.shareUri}
                    qrSvg={ownCard.qrSvg}
                    did={ownCard.card.did}
                />
            )}

            {/* Delete Confirmation Modal */}
            <ConfirmationModal
                isOpen={isDeleteConfirmOpen}
                title={t('marketplace.deleteCardConfirmTitle')}
                description={t('marketplace.deleteCardConfirmDesc')}
                confirmVariant="danger"
                confirmText={t('common.remove')}
                onConfirm={handleDelete}
                onCancel={() => setIsDeleteConfirmOpen(false)}
                isProcessing={isDeleting}
            />
        </>
    );
}
