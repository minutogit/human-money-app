// src/components/marketplace/MineTab.tsx
import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import {
    User,
    Edit3,
    Share2,
    Plus,
    Clock,
    Tag,
    Trash2,
    RefreshCw,
    AlertCircle,
    ChevronDown,
    ChevronUp,
    Inbox,
    CheckCircle2,
    XCircle,
    MessageSquare,
    ArrowRight,
} from 'lucide-react';
import {
    FrontendMyAd,
    FrontendMyMarketProfile,
    FrontendPostInboxSummary,
    FrontendMarketInquiryItem,
} from '../../types';
import { Button } from '../ui/Button';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { Advanced } from '../ui/Advanced';
import { truncateUserId } from '../../utils/userIdHelper';

export interface MineTabProps {
    myProfile: FrontendMyMarketProfile;
    inbox: FrontendPostInboxSummary;
    onOpenProfileEditor: () => void;
    onOpenShareCard: () => void;
    onTogglePublished: (published: boolean) => Promise<void>;
    onCreateAd: () => void;
    onEditAd: (ad: FrontendMyAd) => void;
    onRenewAd: (ad: FrontendMyAd) => void;
    onQuickRenewAd?: (ad: FrontendMyAd) => Promise<void> | void;
    onDeleteAd: (id: string) => Promise<void>;
    onReplyInquiry?: (
        inquiryId: string,
        decision: 'confirm' | 'decline',
        note?: string,
        paymentUri?: string
    ) => Promise<void>;
    onPayInquiry?: (counterpartyDid: string, paymentUri?: string | null) => void;
    onDismissInquiry?: (itemKind: 'seller_inquiry' | 'buyer_inquiry', inquiryId: string) => Promise<void>;
}

export const MineTab: React.FC<MineTabProps> = ({
    myProfile,
    inbox,
    onOpenProfileEditor,
    onOpenShareCard,
    onTogglePublished,
    onCreateAd,
    onEditAd,
    onRenewAd,
    onQuickRenewAd,
    onDeleteAd,
    onReplyInquiry,
    onPayInquiry,
    onDismissInquiry,
}) => {
    const { t } = useTranslation();
    const [isExpiredOpen, setIsExpiredOpen] = useState(false);
    const [confirmDeleteAdId, setConfirmDeleteAdId] = useState<string | null>(null);
    const [isDeletingAd, setIsDeletingAd] = useState(false);
    const [isTogglingPublish, setIsTogglingPublish] = useState(false);
    const [inquiryTab, setInquiryTab] = useState<'received' | 'sent'>('received');
    const [avatarError, setAvatarError] = useState(false);

    useEffect(() => {
        setAvatarError(false);
    }, [myProfile.profile.pictureUrl]);

    // Separate active and expired ads
    const activeAds = myProfile.ads.filter((a) => a.status === 'active');
    const expiredAds = myProfile.ads.filter((a) => a.status === 'expired');

    const displayName =
        [myProfile.profile.firstName, myProfile.profile.lastName].filter(Boolean).join(' ') ||
        myProfile.profile.organization ||
        t('marketplace.anonymousUser', 'Mein Profil');

    const handlePublishToggle = async () => {
        setIsTogglingPublish(true);
        try {
            await onTogglePublished(!myProfile.published);
        } finally {
            setIsTogglingPublish(false);
        }
    };

    const totalInquiriesCount = inbox.sellerInquiries.length + inbox.buyerInquiries.length;

    return (
        <div className="space-y-8">
            {/* ======================================================== */}
            {/* (a) Meine Visitenkarte                                  */}
            {/* ======================================================== */}
            <div className="space-y-3">
                <div className="flex items-center justify-between">
                    <div>
                        <h3 className="text-xs font-black uppercase tracking-wider text-theme-secondary flex items-center gap-2">
                            <User size={14} className="text-theme-primary" />
                            <span>{t('marketplace.myBusinessCardTitle', 'Meine Visitenkarte')}</span>
                        </h3>
                        <p className="text-[11px] text-theme-light mt-0.5">
                            {t('marketplace.myBusinessCardSubtitle', 'Was ich dauerhaft anbiete')}
                        </p>
                    </div>
                </div>

                {!myProfile.published ? (
                    /* Invitation Card: Not published yet */
                    <div
                        className="p-6 sm:p-8 rounded-3xl bg-gradient-to-br from-theme-primary/5 via-white to-bg-app border border-theme-primary/20 space-y-4"
                        data-testid="card-invitation"
                    >
                        <div className="flex items-start gap-4">
                            <div className="w-12 h-12 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center shrink-0">
                                <User size={24} />
                            </div>
                            <div className="space-y-1 min-w-0">
                                <h4 className="text-sm font-bold text-theme-secondary">
                                    {t('marketplace.createCardInvitationTitle', 'Erstelle deine Visitenkarte')}
                                </h4>
                                <p className="text-xs text-theme-light leading-relaxed">
                                    {t(
                                        'marketplace.createCardInvitationDesc',
                                        'Erstelle deine Visitenkarte – so finden dich andere über deine Fähigkeiten.'
                                    )}
                                </p>
                            </div>
                        </div>
                        <div className="pt-1 flex items-center gap-3">
                            <Button variant="primary" onClick={onOpenProfileEditor} className="flex items-center gap-2">
                                <Edit3 size={15} />
                                <span>{t('marketplace.createCardNowBtn', 'Jetzt erstellen')}</span>
                            </Button>
                        </div>
                    </div>
                ) : (
                    /* Preview Summary & Controls: Card is active / published */
                    <div
                        className="p-5 sm:p-6 rounded-3xl bg-white border border-theme-subtle space-y-5 shadow-xs"
                        data-testid="my-published-card-summary"
                    >
                        <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4">
                            <div className="flex items-center gap-3.5 min-w-0">
                                <div className="w-12 h-12 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center font-bold text-base shrink-0 overflow-hidden">
                                    {myProfile.profile.pictureUrl && !avatarError ? (
                                        <img
                                            src={myProfile.profile.pictureUrl}
                                            alt=""
                                            className="w-full h-full object-cover rounded-full"
                                            onError={() => setAvatarError(true)}
                                        />
                                    ) : (
                                        displayName.charAt(0) || <User size={20} />
                                    )}
                                </div>
                                <div className="min-w-0">
                                    <div className="flex items-center gap-2">
                                        <h4 className="text-sm font-bold text-theme-secondary truncate">
                                            {displayName}
                                        </h4>
                                        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-bold bg-emerald-50 text-emerald-700 border border-emerald-200">
                                            <span className="w-1.5 h-1.5 rounded-full bg-emerald-500" />
                                            <span>{t('marketplace.publishedBadge', 'Veröffentlicht')}</span>
                                        </span>
                                    </div>
                                    <p className="text-xs text-theme-light truncate mt-0.5">
                                        {[
                                            myProfile.profile.organization,
                                            myProfile.profile.address?.city,
                                            myProfile.skills.length > 0
                                                ? t('marketplace.skillsCount', '{{count}} Fähigkeit(en)', {
                                                      count: myProfile.skills.length,
                                                  })
                                                : null,
                                        ]
                                            .filter(Boolean)
                                            .join(' · ')}
                                    </p>
                                </div>
                            </div>

                            {/* Publish switch & action buttons */}
                            <div className="flex flex-wrap items-center gap-2.5 self-end sm:self-auto">
                                <label className="inline-flex items-center gap-2 px-3 py-1.5 rounded-xl bg-bg-app border border-theme-subtle cursor-pointer text-xs font-semibold text-theme-secondary select-none">
                                    <input
                                        type="checkbox"
                                        checked={myProfile.published}
                                        disabled={isTogglingPublish}
                                        onChange={handlePublishToggle}
                                        className="rounded text-theme-primary focus:ring-theme-primary w-4 h-4 cursor-pointer"
                                    />
                                    <span>{t('marketplace.publishedToggle', 'Veröffentlicht')}</span>
                                </label>

                                <Button
                                    size="sm"
                                    variant="outline"
                                    onClick={onOpenProfileEditor}
                                    className="flex items-center gap-1.5"
                                >
                                    <Edit3 size={14} />
                                    <span>{t('marketplace.editCardBtn', 'Bearbeiten')}</span>
                                </Button>

                                <Button
                                    size="sm"
                                    variant="primary"
                                    onClick={onOpenShareCard}
                                    className="flex items-center gap-1.5"
                                >
                                    <Share2 size={14} />
                                    <span>{t('marketplace.shareCardBtn', 'Teilen')}</span>
                                </Button>
                            </div>
                        </div>

                        {/* Skills summary chips */}
                        {myProfile.skills.length > 0 && (
                            <div className="pt-3 border-t border-theme-subtle/50 flex flex-wrap gap-2">
                                {myProfile.skills.map((s, idx) => (
                                    <span
                                        key={idx}
                                        className="inline-flex items-center gap-1 px-2.5 py-1 rounded-xl bg-bg-app border border-theme-subtle text-xs text-theme-secondary font-medium"
                                    >
                                        <Tag size={11} className="text-theme-primary" />
                                        <span>{s.title}</span>
                                        {s.pricing && (
                                            <span className="text-[10px] text-theme-light">({s.pricing})</span>
                                        )}
                                    </span>
                                ))}
                            </div>
                        )}
                    </div>
                )}
            </div>

            {/* ======================================================== */}
            {/* (b) Meine Inserate                                      */}
            {/* ======================================================== */}
            <div className="space-y-4">
                <div className="flex items-center justify-between">
                    <div>
                        <h3 className="text-xs font-black uppercase tracking-wider text-theme-secondary flex items-center gap-2">
                            <Tag size={14} className="text-theme-primary" />
                            <span>{t('marketplace.myAdsTitle', 'Meine Inserate')}</span>
                            <span className="text-theme-light font-normal">({activeAds.length})</span>
                        </h3>
                    </div>
                    <Button
                        size="xs"
                        variant="primary"
                        onClick={onCreateAd}
                        className="flex items-center gap-1"
                        data-testid="create-ad-btn"
                    >
                        <Plus size={14} />
                        <span>{t('marketplace.createAdBtn', 'Inserat aufgeben')}</span>
                    </Button>
                </div>

                {activeAds.length > 0 ? (
                    <div className="space-y-3">
                        {activeAds.map((ad) => {
                            const isOffer = ad.kind === 'offer';
                            const isExpiringSoon = ad.daysLeft <= 14;

                            return (
                                <div
                                    key={ad.id}
                                    className="p-4 sm:p-5 rounded-2xl bg-white border border-theme-subtle flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4 shadow-xs"
                                    data-testid={`my-active-ad-${ad.id}`}
                                >
                                    <div className="space-y-1.5 min-w-0">
                                        <div className="flex items-center gap-2">
                                            <span
                                                className={`inline-flex items-center px-2 py-0.5 rounded-full text-[10px] font-bold uppercase tracking-wider ${
                                                    isOffer
                                                        ? 'bg-emerald-50 text-emerald-700 border border-emerald-200'
                                                        : 'bg-sky-50 text-sky-700 border border-sky-200'
                                                }`}
                                            >
                                                {isOffer
                                                    ? t('marketplace.adKindOffer', 'Angebot')
                                                    : t('marketplace.adKindRequest', 'Gesuch')}
                                            </span>

                                            <span
                                                className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-semibold ${
                                                    isExpiringSoon
                                                        ? 'bg-amber-50 text-amber-800 border border-amber-200'
                                                        : 'bg-bg-app text-theme-light border border-theme-subtle'
                                                }`}
                                            >
                                                <Clock size={11} className={isExpiringSoon ? 'text-amber-600' : 'text-theme-light'} />
                                                <span>
                                                    {ad.daysLeft === 1
                                                        ? t('marketplace.oneDayLeft', 'noch 1 Tag')
                                                        : t('marketplace.daysLeft', 'noch {{count}} Tage', {
                                                              count: ad.daysLeft,
                                                          })}
                                                </span>
                                            </span>

                                            {ad.pricing && (
                                                <span className="text-xs text-theme-primary font-semibold">
                                                    · {ad.pricing}
                                                </span>
                                            )}
                                        </div>

                                        <h4 className="text-sm font-bold text-theme-secondary truncate max-w-lg">
                                            {ad.title}
                                        </h4>
                                        {ad.text && (
                                            <p className="text-xs text-theme-light line-clamp-1 max-w-lg">
                                                {ad.text}
                                            </p>
                                        )}
                                    </div>

                                    {/* Action buttons */}
                                    <div className="flex items-center gap-2 self-end sm:self-auto shrink-0">
                                        <Button
                                            size="xs"
                                            variant="outline"
                                            onClick={() => onEditAd(ad)}
                                            className="flex items-center gap-1"
                                        >
                                            <Edit3 size={13} />
                                            <span>{t('marketplace.editBtn', 'Bearbeiten')}</span>
                                        </Button>
                                        <Button
                                            size="xs"
                                            variant="outline"
                                            onClick={() => (onQuickRenewAd ? onQuickRenewAd(ad) : onRenewAd(ad))}
                                            className="flex items-center gap-1"
                                            data-testid={`quick-renew-ad-${ad.id}`}
                                        >
                                            <RefreshCw size={13} />
                                            <span>{t('marketplace.quickRenew90Btn', '+90 Tage')}</span>
                                        </Button>
                                        <Advanced>
                                            <Button
                                                size="xs"
                                                variant="outline"
                                                onClick={() => onRenewAd(ad)}
                                                className="flex items-center gap-1"
                                                data-testid={`custom-renew-ad-${ad.id}`}
                                            >
                                                <Clock size={13} />
                                                <span>{t('marketplace.renewBtn', 'Verlängern')}</span>
                                            </Button>
                                        </Advanced>
                                        <Button
                                            size="xs"
                                            variant="outline"
                                            onClick={() => setConfirmDeleteAdId(ad.id)}
                                            className="text-rose-600 hover:bg-rose-50 border-rose-200"
                                            title={t('marketplace.deleteBtn', 'Löschen')}
                                        >
                                            <Trash2 size={13} />
                                        </Button>
                                    </div>
                                </div>
                            );
                        })}
                    </div>
                ) : (
                    <div className="p-6 rounded-2xl bg-bg-app border border-theme-subtle text-center space-y-2">
                        <p className="text-xs text-theme-light">
                            {t('marketplace.noOwnActiveAds', 'Aktuell hast du keine aktiven Inserate auf der Pinnwand.')}
                        </p>
                    </div>
                )}

                {/* Collapsible Expired Ads */}
                {expiredAds.length > 0 && (
                    <div className="border border-theme-subtle rounded-2xl overflow-hidden bg-bg-app/40">
                        <button
                            type="button"
                            onClick={() => setIsExpiredOpen(!isExpiredOpen)}
                            className="w-full p-4 flex items-center justify-between text-left text-xs font-bold text-theme-secondary hover:bg-bg-app transition-colors"
                        >
                            <span className="flex items-center gap-2">
                                <AlertCircle size={15} className="text-theme-light" />
                                <span>{t('marketplace.expiredAdsSection', 'Abgelaufen ({{count}})', { count: expiredAds.length })}</span>
                            </span>
                            {isExpiredOpen ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
                        </button>

                        {isExpiredOpen && (
                            <div className="p-4 pt-0 space-y-3">
                                {expiredAds.map((ad) => (
                                    <div
                                        key={ad.id}
                                        className="p-3.5 rounded-xl bg-white border border-theme-subtle flex flex-col sm:flex-row items-start sm:items-center justify-between gap-3 text-xs opacity-75"
                                    >
                                        <div className="min-w-0">
                                            <div className="flex items-center gap-2">
                                                <span className="text-[10px] font-bold uppercase text-slate-500">
                                                    {ad.kind === 'offer'
                                                        ? t('marketplace.adKindOffer', 'Angebot')
                                                        : t('marketplace.adKindRequest', 'Gesuch')}
                                                </span>
                                                <h5 className="font-bold text-theme-secondary truncate max-w-sm">
                                                    {ad.title}
                                                </h5>
                                            </div>
                                        </div>
                                        <div className="flex items-center gap-2 self-end sm:self-auto shrink-0">
                                            <Button
                                                size="xs"
                                                variant="outline"
                                                onClick={() => (onQuickRenewAd ? onQuickRenewAd(ad) : onRenewAd(ad))}
                                                className="flex items-center gap-1"
                                                data-testid={`quick-renew-ad-${ad.id}`}
                                            >
                                                <RefreshCw size={12} />
                                                <span>{t('marketplace.quickRenew90Btn', '+90 Tage')}</span>
                                            </Button>
                                            <Advanced>
                                                <Button
                                                    size="xs"
                                                    variant="outline"
                                                    onClick={() => onRenewAd(ad)}
                                                    className="flex items-center gap-1"
                                                    data-testid={`custom-renew-ad-${ad.id}`}
                                                >
                                                    <Clock size={12} />
                                                    <span>{t('marketplace.republishBtn', 'Erneut veröffentlichen')}</span>
                                                </Button>
                                            </Advanced>
                                            <Button
                                                size="xs"
                                                variant="outline"
                                                onClick={() => setConfirmDeleteAdId(ad.id)}
                                                className="text-rose-600 hover:bg-rose-50"
                                            >
                                                <Trash2 size={12} />
                                            </Button>
                                        </div>
                                    </div>
                                ))}
                            </div>
                        )}
                    </div>
                )}
            </div>

            {/* ======================================================== */}
            {/* (c) Anfragen                                            */}
            {/* ======================================================== */}
            <div className="space-y-4 pt-4 border-t border-theme-subtle/60">
                <div className="flex items-center justify-between">
                    <h3 className="text-xs font-black uppercase tracking-wider text-theme-secondary flex items-center gap-2">
                        <Inbox size={14} className="text-theme-primary" />
                        <span>{t('marketplace.inquiriesTitle', 'Anfragen')}</span>
                        {totalInquiriesCount > 0 && (
                            <span className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-theme-primary/10 text-theme-primary">
                                {totalInquiriesCount}
                            </span>
                        )}
                    </h3>

                    {/* Sub-tabs: Eingegangen / Gesendet */}
                    <div className="inline-flex rounded-xl p-1 bg-bg-app border border-theme-subtle">
                        <button
                            type="button"
                            onClick={() => setInquiryTab('received')}
                            className={`px-3 py-1 rounded-lg text-xs font-bold transition-all ${
                                inquiryTab === 'received'
                                    ? 'bg-white text-theme-primary shadow-xs'
                                    : 'text-theme-light hover:text-theme-secondary'
                            }`}
                        >
                            {t('marketplace.inquiriesReceived', 'Eingegangen')} ({inbox.sellerInquiries.length})
                        </button>
                        <button
                            type="button"
                            onClick={() => setInquiryTab('sent')}
                            className={`px-3 py-1 rounded-lg text-xs font-bold transition-all ${
                                inquiryTab === 'sent'
                                    ? 'bg-white text-theme-primary shadow-xs'
                                    : 'text-theme-light hover:text-theme-secondary'
                            }`}
                        >
                            {t('marketplace.inquiriesSent', 'Gesendet')} ({inbox.buyerInquiries.length})
                        </button>
                    </div>
                </div>

                {/* Inquiries list */}
                {inquiryTab === 'received' ? (
                    inbox.sellerInquiries.length > 0 ? (
                        <div className="space-y-3">
                            {inbox.sellerInquiries.map((inq) => (
                                <InquiryItemCard
                                    key={inq.inquiryId}
                                    item={inq}
                                    isSeller={true}
                                    onReply={onReplyInquiry}
                                    onPay={onPayInquiry}
                                    onDismiss={onDismissInquiry ? () => onDismissInquiry('seller_inquiry', inq.inquiryId) : undefined}
                                />
                            ))}
                        </div>
                    ) : (
                        <div className="p-6 rounded-2xl bg-bg-app border border-theme-subtle text-center text-xs text-theme-light">
                            {t('marketplace.noReceivedInquiries', 'Keine eingegangenen Anfragen.')}
                        </div>
                    )
                ) : inbox.buyerInquiries.length > 0 ? (
                    <div className="space-y-3">
                        {inbox.buyerInquiries.map((inq) => (
                            <InquiryItemCard
                                key={inq.inquiryId}
                                item={inq}
                                isSeller={false}
                                onReply={onReplyInquiry}
                                onPay={onPayInquiry}
                                onDismiss={onDismissInquiry ? () => onDismissInquiry('buyer_inquiry', inq.inquiryId) : undefined}
                            />
                        ))}
                    </div>
                ) : (
                    <div className="p-6 rounded-2xl bg-bg-app border border-theme-subtle text-center text-xs text-theme-light">
                        {t('marketplace.noSentInquiries', 'Keine gesendeten Anfragen.')}
                    </div>
                )}
            </div>

            <ConfirmationModal
                isOpen={confirmDeleteAdId !== null}
                title={t('marketplace.deleteBtn', 'Löschen')}
                description={t('common.deleteConfirm', 'Diesen Eintrag wirklich dauerhaft löschen?')}
                confirmText={t('marketplace.deleteBtn', 'Löschen')}
                cancelText={t('common.cancel', 'Abbrechen')}
                confirmVariant="danger"
                isProcessing={isDeletingAd}
                onCancel={() => setConfirmDeleteAdId(null)}
                onConfirm={async () => {
                    if (!confirmDeleteAdId) return;
                    setIsDeletingAd(true);
                    try {
                        await onDeleteAd(confirmDeleteAdId);
                        setConfirmDeleteAdId(null);
                    } finally {
                        setIsDeletingAd(false);
                    }
                }}
            />
        </div>
    );
};

interface InquiryItemCardProps {
    item: FrontendMarketInquiryItem;
    isSeller: boolean;
    onReply?: (
        inquiryId: string,
        decision: 'confirm' | 'decline',
        note?: string,
        paymentUri?: string
    ) => Promise<void>;
    onPay?: (counterpartyDid: string, paymentUri?: string | null) => void;
    onDismiss?: () => Promise<void>;
}

const InquiryItemCard: React.FC<InquiryItemCardProps> = ({
    item,
    isSeller,
    onReply,
    onPay,
    onDismiss,
}) => {
    const { t } = useTranslation();
    const [isReplying, setIsReplying] = useState(false);
    const [replyNote, setReplyNote] = useState('');
    const [isSubmitting, setIsSubmitting] = useState(false);
    const [isDismissing, setIsDismissing] = useState(false);

    const targetLabel =
        item.target.kind === 'ad'
            ? `Inserat: ${item.target.ad_title}`
            : item.target.skill_title
            ? `Fähigkeit: ${item.target.skill_title}`
            : t('marketplace.generalInquiry', 'Allgemeine Anfrage');

    const counterpartyDisplay = item.counterpartyName || truncateUserId(item.counterpartyDid);
    const dateFormatted = new Date(item.createdAt * 1000).toLocaleString();

    const handleAction = async (decision: 'confirm' | 'decline') => {
        if (!onReply) return;
        setIsSubmitting(true);
        try {
            await onReply(item.inquiryId, decision, replyNote.trim() || undefined);
            setIsReplying(false);
        } finally {
            setIsSubmitting(false);
        }
    };

    return (
        <div className="p-4 rounded-2xl bg-white border border-theme-subtle space-y-3 shadow-xs">
            <div className="flex items-start justify-between gap-3">
                <div>
                    <span className="text-[10px] font-bold uppercase tracking-wider text-theme-primary px-2 py-0.5 rounded-full bg-theme-primary/10">
                        {targetLabel}
                    </span>
                    <h4 className="text-xs font-bold text-theme-secondary mt-1.5">
                        {isSeller ? `Von: ${counterpartyDisplay}` : `An: ${counterpartyDisplay}`}
                    </h4>
                </div>
                <div className="flex items-center gap-2">
                    <span className="text-[10px] text-theme-light">{dateFormatted}</span>
                    {onDismiss && (
                        <button
                            type="button"
                            disabled={isDismissing}
                            onClick={async () => {
                                setIsDismissing(true);
                                try {
                                    await onDismiss();
                                } finally {
                                    setIsDismissing(false);
                                }
                            }}
                            title={t('common.remove', 'Entfernen')}
                            aria-label={t('common.remove', 'Entfernen')}
                            className="p-1 rounded-lg text-theme-light hover:text-rose-600 hover:bg-rose-50 transition-colors cursor-pointer disabled:opacity-50"
                        >
                            <Trash2 size={13} />
                        </button>
                    )}
                </div>
            </div>

            <p className="text-xs text-theme-secondary bg-bg-app p-3 rounded-xl leading-relaxed whitespace-pre-wrap">
                {item.message}
            </p>

            {/* Reply status */}
            {item.reply ? (
                <div
                    className={`p-3 rounded-xl text-xs flex items-start gap-2 border ${
                        item.reply.decision === 'confirm'
                            ? 'bg-emerald-50 text-emerald-800 border-emerald-200'
                            : 'bg-rose-50 text-rose-800 border-rose-200'
                    }`}
                >
                    {item.reply.decision === 'confirm' ? (
                        <CheckCircle2 size={15} className="text-emerald-600 shrink-0 mt-0.5" />
                    ) : (
                        <XCircle size={15} className="text-rose-600 shrink-0 mt-0.5" />
                    )}
                    <div className="min-w-0">
                        <div className="font-bold">
                            {item.reply.decision === 'confirm'
                                ? t('marketplace.inquiryConfirmed', 'Bestätigt')
                                : t('marketplace.inquiryDeclined', 'Abgelehnt')}
                        </div>
                        {item.reply.note && <p className="mt-0.5 text-[11px]">{item.reply.note}</p>}
                        {!isSeller && item.reply.decision === 'confirm' && onPay && (
                            <div className="pt-2">
                                <Button
                                    size="xs"
                                    variant="primary"
                                    onClick={() => onPay(item.counterpartyDid, item.reply!.paymentUri)}
                                    data-testid={`inquiry-pay-btn-${item.inquiryId}`}
                                    className="flex items-center gap-1"
                                >
                                    <span>{t('marketplace.payNowBtn', 'Jetzt bezahlen')}</span>
                                    <ArrowRight size={13} />
                                </Button>
                            </div>
                        )}
                    </div>
                </div>
            ) : isSeller && onReply ? (
                <div className="pt-2">
                    {!isReplying ? (
                        <div className="flex items-center gap-2">
                            <Button
                                size="xs"
                                variant="primary"
                                onClick={() => setIsReplying(true)}
                                className="flex items-center gap-1"
                            >
                                <MessageSquare size={12} />
                                <span>{t('marketplace.replyInquiryBtn', 'Antworten')}</span>
                            </Button>
                        </div>
                    ) : (
                        <div className="space-y-2 pt-2 border-t border-theme-subtle/50">
                            <input
                                type="text"
                                placeholder={t('marketplace.replyNotePlaceholder', 'Optionale Notiz (z. B. Abholzeitpunkt)...')}
                                value={replyNote}
                                onChange={(e) => setReplyNote(e.target.value)}
                                className="w-full text-xs p-2 rounded-xl border border-theme-subtle bg-white"
                                maxLength={280}
                            />
                            <div className="flex items-center justify-end gap-2">
                                <Button
                                    size="xs"
                                    variant="secondary"
                                    onClick={() => setIsReplying(false)}
                                    disabled={isSubmitting}
                                >
                                    {t('common.cancel', 'Abbrechen')}
                                </Button>
                                <Button
                                    size="xs"
                                    variant="outline"
                                    onClick={() => handleAction('decline')}
                                    disabled={isSubmitting}
                                    className="text-rose-600 hover:bg-rose-50"
                                >
                                    {t('marketplace.declineInquiryBtn', 'Ablehnen')}
                                </Button>
                                <Button
                                    size="xs"
                                    variant="primary"
                                    onClick={() => handleAction('confirm')}
                                    disabled={isSubmitting}
                                >
                                    {t('marketplace.confirmInquiryBtn', 'Bestätigen')}
                                </Button>
                            </div>
                        </div>
                    )}
                </div>
            ) : (
                <div className="text-[11px] text-theme-light italic">
                    {t('marketplace.inquiryWaitingForSeller', 'Wartet auf Antwort des Verkäufers...')}
                </div>
            )}
        </div>
    );
};
