// src/components/marketplace/MineTab.tsx
import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
    User,
    Edit3,
    Plus,
    Clock,
    Calendar,
    Tag,
    Trash2,
    RefreshCw,
    AlertCircle,
    ChevronDown,
    ChevronUp,
    Inbox,
} from 'lucide-react';
import {
    Contact,
    FrontendMyAd,
    FrontendMyMarketProfile,
    FrontendPostInboxSummary,
} from '../../types';
import { Button } from '../ui/Button';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { Advanced } from '../ui/Advanced';
import { MyProfileCard } from '../profile/MyProfileCard';
import { InquiryThreadCard } from './InquiryThreadCard';
import { formatEventDateTime, getAdKindBadgeClass } from '../../utils/adEventFormat';

export interface MineTabProps {
    myProfile: FrontendMyMarketProfile;
    inbox: FrontendPostInboxSummary;
    contacts?: Contact[];
    onOpenProfileEditor: () => void;
    onOpenShareCard: () => void;
    onTogglePublished: (published: boolean) => Promise<void>;
    onCreateAd: () => void;
    onEditAd: (ad: FrontendMyAd) => void;
    onRenewAd: (ad: FrontendMyAd) => void;
    onQuickRenewAd?: (ad: FrontendMyAd) => Promise<void> | void;
    onDeleteAd: (id: string) => Promise<void>;
    onSendInquiryMessage?: (
        inquiryId: string,
        options: { text?: string; decision?: 'confirm' | 'decline' }
    ) => Promise<void>;
    onMarkInquiryRead?: (inquiryId: string) => Promise<void> | void;
    onPayInquiry?: (counterpartyDid: string, paymentUri?: string | null) => void;
    onDismissInquiry?: (inquiryId: string) => Promise<void>;
    onAddContact?: (did: string, suggestedName?: string | null) => void;
    onViewCard?: (did: string) => void;
}

export const MineTab: React.FC<MineTabProps> = ({
    myProfile,
    inbox,
    contacts = [],
    onOpenProfileEditor,
    onOpenShareCard,
    onTogglePublished,
    onCreateAd,
    onEditAd,
    onRenewAd,
    onQuickRenewAd,
    onDeleteAd,
    onSendInquiryMessage,
    onMarkInquiryRead,
    onPayInquiry,
    onDismissInquiry,
    onAddContact,
    onViewCard,
}) => {
    const { t, i18n } = useTranslation();
    const [isExpiredOpen, setIsExpiredOpen] = useState(false);
    const [confirmDeleteAdId, setConfirmDeleteAdId] = useState<string | null>(null);
    const [isDeletingAd, setIsDeletingAd] = useState(false);

    // Separate active and expired ads
    const activeAds = myProfile.ads.filter((a) => a.status === 'active');
    const expiredAds = myProfile.ads.filter((a) => a.status === 'expired');

    const inquiries = inbox.inquiries || [];
    const totalInquiriesCount = inquiries.length;
    const unreadInquiriesCount = inquiries.filter((i) => i.unread).length;

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

                <MyProfileCard
                    myProfile={myProfile}
                    onEdit={onOpenProfileEditor}
                    onShare={onOpenShareCard}
                    onTogglePublished={onTogglePublished}
                />
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
                            const isEvent = ad.kind === 'event';
                            const isExpiringSoon = !isEvent && ad.daysLeft <= 14;
                            const kindLabel =
                                ad.kind === 'offer'
                                    ? t('marketplace.adKindOffer', 'Angebot')
                                    : ad.kind === 'request'
                                        ? t('marketplace.adKindRequest', 'Gesuch')
                                        : t('marketplace.adKindEvent', 'Termin');

                            return (
                                <div
                                    key={ad.id}
                                    className="p-4 sm:p-5 rounded-2xl bg-white border border-theme-subtle flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4 shadow-xs"
                                    data-testid={`my-active-ad-${ad.id}`}
                                >
                                    <div className="space-y-1.5 min-w-0">
                                        <div className="flex items-center gap-2 flex-wrap">
                                            <span
                                                className={`inline-flex items-center px-2 py-0.5 rounded-full text-[10px] font-bold uppercase tracking-wider ${getAdKindBadgeClass(ad.kind)}`}
                                            >
                                                {kindLabel}
                                            </span>

                                            {isEvent && ad.eventAt ? (
                                                <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-semibold bg-violet-50 text-violet-800 border border-violet-200">
                                                    <Calendar size={11} className="text-violet-600" />
                                                    <span>{formatEventDateTime(ad.eventAt, ad.eventAllDay, i18n.language)}</span>
                                                </span>
                                            ) : (
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
                                            )}

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
                                        {!isEvent && (
                                            <>
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
                                            </>
                                        )}
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
                                                        : ad.kind === 'request'
                                                            ? t('marketplace.adKindRequest', 'Gesuch')
                                                            : t('marketplace.adKindEvent', 'Termin')}
                                                </span>
                                                <h5 className="font-bold text-theme-secondary truncate max-w-sm">
                                                    {ad.title}
                                                </h5>
                                            </div>
                                        </div>
                                        <div className="flex items-center gap-2 self-end sm:self-auto shrink-0">
                                            {ad.kind === 'event' ? (
                                                <Button
                                                    size="xs"
                                                    variant="outline"
                                                    onClick={() => onEditAd(ad)}
                                                    className="flex items-center gap-1"
                                                    data-testid={`edit-expired-ad-${ad.id}`}
                                                >
                                                    <Edit3 size={12} />
                                                    <span>{t('marketplace.rescheduleBtn', 'Neu ansetzen')}</span>
                                                </Button>
                                            ) : (
                                                <>
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
                                                </>
                                            )}
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
            {/* (c) Anfragen & Nachrichten                              */}
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
                        {unreadInquiriesCount > 0 && (
                            <span className="px-2 py-0.5 rounded-full text-[10px] font-black bg-rose-500 text-white">
                                {unreadInquiriesCount} {t('common.newBadge', 'Neu')}
                            </span>
                        )}
                    </h3>
                </div>

                {inquiries.length > 0 ? (
                    <div className="space-y-3">
                        {inquiries.map((inq) => (
                            <InquiryThreadCard
                                key={inq.inquiryId}
                                item={inq}
                                contacts={contacts}
                                onSendMessage={onSendInquiryMessage}
                                onMarkRead={onMarkInquiryRead}
                                onPay={onPayInquiry}
                                onDismiss={onDismissInquiry ? () => onDismissInquiry(inq.inquiryId) : undefined}
                                onAddContact={onAddContact}
                                onViewCard={onViewCard}
                            />
                        ))}
                    </div>
                ) : (
                    <div className="p-6 rounded-2xl bg-bg-app border border-theme-subtle text-center text-xs text-theme-light">
                        {t('marketplace.noInquiries', 'Noch keine Anfragen vorhanden.')}
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
