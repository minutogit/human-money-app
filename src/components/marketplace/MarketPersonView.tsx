// src/components/marketplace/MarketPersonView.tsx
import React, { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { UserAvatar } from '../ui/UserAvatar';
import {
    Send,
    UserPlus,
    UserCheck,
    CheckCircle2,
    HelpCircle,
    Ticket,
    CreditCard,
    Box,
    MapPin,
    Mail,
    Phone,
    Globe,
    Building2,
    Users,
    Copy,
    Check,
    Tag,
    Briefcase,
    Calendar,
    ArrowLeft,
    Share2,
    RefreshCw,
    Heart,
    MessageSquare,
    Clock,
} from 'lucide-react';
import {
    FrontendMarketPerson,
    Contact,
    FrontendForwardCardExport,
    Ad,
    InquiryTarget,
} from '../../types';
import { useNavigation } from '../../context/NavigationContext';
import { useSession } from '../../context/SessionContext';
import { contactService } from '../../services/contactService';
import { marketService } from '../../services/marketService';
import { postService } from '../../services/postService';
import { useRateLimitCooldown } from '../../hooks/useRateLimitCooldown';
import ContactDialog from '../contacts/ContactDialog';
import { ShareCardModal } from './ShareCardModal';
import { AdCard, resolveAuthorHops } from './AdCard';
import { copyToClipboard } from '../../utils/clipboard';
import { truncateUserId, isExactSameUserId } from '../../utils/userIdHelper';
import { translateError, stringifyError } from '../../utils/errorHelper';
import { logger } from '../../utils/log';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { ModalShell } from '../ui/ModalShell';
import { openUrl } from '@tauri-apps/plugin-opener';

export interface MarketPersonViewProps {
    person: FrontendMarketPerson;
    onBack: () => void;
    initialInquiryAd?: Ad | null;
    mediaAutoLoadMaxHops?: number;
    wifiOnly?: boolean;
}

export const MarketPersonView: React.FC<MarketPersonViewProps> = ({
    person,
    onBack,
    initialInquiryAd,
    mediaAutoLoadMaxHops = 2,
    wifiOnly = false,
}) => {
    const { t } = useTranslation();
    const { navigate } = useNavigation();
    const { protectAction } = useSession();

    const [currentPerson, setCurrentPerson] = useState<FrontendMarketPerson>(person);
    const [isManualSyncing, setIsManualSyncing] = useState(false);
    const { isCoolingDown, checkAndTriggerCooldown } = useRateLimitCooldown();
    const [contacts, setContacts] = useState<Contact[]>([]);
    const [isContactDialogOpen, setIsContactDialogOpen] = useState(false);
    const [copiedDid, setCopiedDid] = useState(false);

    // Forwarding
    const [isForwarding, setIsForwarding] = useState(false);
    const [forwardCardData, setForwardCardData] = useState<FrontendForwardCardExport | null>(null);
    const [isShareModalOpen, setIsShareModalOpen] = useState(false);
    const [forwardError, setForwardError] = useState<string | null>(null);

    // Inquiries
    const [inquiryModalOpen, setInquiryModalOpen] = useState(false);
    const [inquiryTarget, setInquiryTarget] = useState<InquiryTarget>({ kind: 'person' });
    const [inquiryMessage, setInquiryMessage] = useState('');
    const [isSendingInquiry, setIsSendingInquiry] = useState(false);
    const [inquiryFeedback, setInquiryFeedback] = useState<{ type: 'success' | 'error'; message: string } | null>(null);

    const copiedTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const inquiryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const mountedRef = useRef(true);
    const syncSeqRef = useRef(0);
    const contactsSeqRef = useRef(0);
    const forwardSeqRef = useRef(0);
    const inquirySeqRef = useRef(0);

    useEffect(() => {
        mountedRef.current = true;
        return () => {
            mountedRef.current = false;
            if (copiedTimerRef.current) clearTimeout(copiedTimerRef.current);
            if (inquiryTimerRef.current) clearTimeout(inquiryTimerRef.current);
        };
    }, []);

    useEffect(() => {
        setCurrentPerson(person);
    }, [person]);

    const handleManualSync = async () => {
        if (isCoolingDown) return;
        const seq = ++syncSeqRef.current;
        setIsManualSyncing(true);
        try {
            const res = await marketService.syncMarketChannels();
            if (!mountedRef.current || seq !== syncSeqRef.current) return;
            if (res) {
                checkAndTriggerCooldown(res);
            }
            const snapshot = await marketService.getMarketView();
            if (!mountedRef.current || seq !== syncSeqRef.current) return;
            const targetId = currentPerson.recipientDid || currentPerson.did;
            const updated = (snapshot.people || []).find((p) =>
                isExactSameUserId(p.recipientDid || p.did, targetId),
            );
            if (updated) {
                setCurrentPerson(updated);
            }
        } catch (err) {
            if (!mountedRef.current || seq !== syncSeqRef.current) return;
            logger.error(`Manual sync failed in MarketPersonView: ${stringifyError(err)}`);
            checkAndTriggerCooldown(err);
        } finally {
            if (mountedRef.current && seq === syncSeqRef.current) {
                setIsManualSyncing(false);
            }
        }
    };

    const loadContacts = useCallback(async () => {
        const seq = ++contactsSeqRef.current;
        try {
            const list = await contactService.getContacts();
            if (!mountedRef.current || seq !== contactsSeqRef.current) return;
            setContacts(list || []);
        } catch {
            // Contacts might not be available
        }
    }, []);

    useEffect(() => {
        loadContacts();
    }, [loadContacts]);

    const paymentRecipientId = currentPerson.recipientDid || currentPerson.did;
    const existingContact = Array.isArray(contacts)
        ? contacts.find((c) => isExactSameUserId(c.did, paymentRecipientId))
        : undefined;
    const hasReachableInquirySync =
        (currentPerson.cardSyncs?.length ?? 0) > 0 || (existingContact?.syncs?.length ?? 0) > 0;

    const handleCopyDid = async () => {
        const success = await copyToClipboard(paymentRecipientId);
        if (success) {
            setCopiedDid(true);
            if (copiedTimerRef.current) clearTimeout(copiedTimerRef.current);
            copiedTimerRef.current = setTimeout(() => setCopiedDid(false), 2000);
        }
    };

    const handlePay = () => {
        navigate({ view: 'send_vouchers', initialRecipientId: paymentRecipientId });
    };

    const hasCardSource = currentPerson.sources.some((s) => s.kind === 'card');

    const handleForwardCard = async () => {
        const seq = ++forwardSeqRef.current;
        setIsForwarding(true);
        setForwardError(null);
        try {
            const exportData = await protectAction(async (pwd) => {
                return await marketService.exportForwardCard(paymentRecipientId, pwd || undefined);
            });
            if (!mountedRef.current || seq !== forwardSeqRef.current) return;
            if (exportData) {
                setForwardCardData(exportData);
                setIsShareModalOpen(true);
            }
        } catch (err) {
            if (!mountedRef.current || seq !== forwardSeqRef.current) return;
            setForwardError(translateError(err, t));
        } finally {
            if (mountedRef.current && seq === forwardSeqRef.current) {
                setIsForwarding(false);
            }
        }
    };

    const handleOpenWebsite = async (url: string) => {
        try {
            const fullUrl = url.startsWith('http://') || url.startsWith('https://') ? url : `https://${url}`;
            await openUrl(fullUrl);
        } catch (err) {
            logger.warn(`Failed to open website ${url}: ${stringifyError(err)}`);
        }
    };

    // Inquiry flow
    const openInquiryForPerson = (skillTitle?: string) => {
        setInquiryTarget({ kind: 'person', skill_title: skillTitle });
        setInquiryMessage('');
        setInquiryFeedback(null);
        setInquiryModalOpen(true);
    };

    const openInquiryForAd = (ad: Ad) => {
        setInquiryTarget({ kind: 'ad', ad_id: ad.id, ad_title: ad.title });
        setInquiryMessage('');
        setInquiryFeedback(null);
        setInquiryModalOpen(true);
    };

    useEffect(() => {
        if (initialInquiryAd) {
            openInquiryForAd(initialInquiryAd);
        }
    }, [initialInquiryAd]);

    const handleSendInquiry = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!inquiryMessage.trim()) return;

        const seq = ++inquirySeqRef.current;
        setIsSendingInquiry(true);
        setInquiryFeedback(null);
        try {
            await protectAction(async (pwd) => {
                return await postService.sendMarketInquiryViaPost(
                    paymentRecipientId,
                    inquiryTarget,
                    inquiryMessage.trim(),
                    currentPerson.cardSyncs,
                    pwd || undefined
                );
            });
            if (!mountedRef.current || seq !== inquirySeqRef.current) return;
            setInquiryFeedback({
                type: 'success',
                message: t('marketplace.inquirySentSuccess', 'Anfrage erfolgreich gesendet!'),
            });
            if (inquiryTimerRef.current) clearTimeout(inquiryTimerRef.current);
            inquiryTimerRef.current = setTimeout(() => {
                if (mountedRef.current) {
                    setInquiryModalOpen(false);
                }
            }, 1500);
        } catch (err) {
            if (!mountedRef.current || seq !== inquirySeqRef.current) return;
            setInquiryFeedback({
                type: 'error',
                message: translateError(err, t),
            });
        } finally {
            if (mountedRef.current && seq === inquirySeqRef.current) {
                setIsSendingInquiry(false);
            }
        }
    };

    const initialSyncUrls = useMemo(
        () => currentPerson.cardSyncs?.map((s) => s.writeUrl || s.readUrl).filter(Boolean),
        [currentPerson.cardSyncs]
    );

    const fullName = [currentPerson.profile.firstName, currentPerson.profile.lastName].filter(Boolean).join(' ');
    const displayName =
        fullName ||
        currentPerson.profile.organization ||
        currentPerson.profile.community ||
        truncateUserId(currentPerson.did);

    const formattedDate = currentPerson.asOf ? new Date(currentPerson.asOf * 1000).toLocaleDateString() : '';

    const address = currentPerson.profile.address;
    const addressLine1 = [address?.street, address?.houseNumber].filter(Boolean).join(' ');
    const addressLine2 = [address?.zipCode, address?.city].filter(Boolean).join(' ');
    const hasAddress = !!(address?.fullAddress || addressLine1 || addressLine2 || address?.country);

    return (
        <div className="max-w-4xl mx-auto space-y-6">
            {/* Header row with navigation & primary actions */}
            <div className="flex items-center justify-between gap-4">
                <Button
                    variant="outline"
                    size="sm"
                    onClick={onBack}
                    className="gap-2 text-theme-light hover:text-theme-secondary"
                >
                    <ArrowLeft size={16} />
                    <span>{t('common.back', 'Zurück')}</span>
                </Button>

                <div className="flex items-center gap-2">
                    {/* Send Payment */}
                    <Button onClick={handlePay} size="sm" className="gap-2 shadow-sm font-semibold">
                        <Send size={15} />
                        <span>{t('marketplace.pay', 'Bezahlen')}</span>
                    </Button>

                    {/* Contact Action */}
                    <Button
                        variant={existingContact ? 'secondary' : 'outline'}
                        size="sm"
                        onClick={() => setIsContactDialogOpen(true)}
                        className="gap-2 shadow-sm"
                    >
                        {existingContact ? (
                            <>
                                <UserCheck size={15} className="text-theme-success" />
                                <span>
                                    {existingContact.trustLevel == null
                                        ? t('marketplace.rateTrust', 'Vertrauen einstufen')
                                        : t('marketplace.editContact', 'Kontakt bearbeiten')}
                                </span>
                            </>
                        ) : (
                            <>
                                <UserPlus size={15} />
                                <span>{t('marketplace.addToContacts', 'Ins Adressbuch')}</span>
                            </>
                        )}
                    </Button>

                    {/* General Inquiry Action */}
                    {currentPerson.allowInquiries && (
                        <Button
                            variant="outline"
                            size="sm"
                            onClick={() => openInquiryForPerson()}
                            className="gap-2 shadow-sm"
                        >
                            <MessageSquare size={15} />
                            <span>{t('marketplace.sendInquiryBtn', 'Anfrage senden')}</span>
                        </Button>
                    )}

                    {/* Recommend / Forward Action */}
                    {(currentPerson.canForward || hasCardSource) && (
                        <Button
                            variant="outline"
                            size="sm"
                            disabled={!currentPerson.canForward || isForwarding}
                            onClick={handleForwardCard}
                            title={
                                !currentPerson.canForward
                                    ? t('marketplace.forwardMaxHopsReached', 'Maximale Weiterempfehlungsstufen erreicht')
                                    : t('marketplace.forwardCardBtn', 'Weiterempfehlen')
                            }
                            className="gap-2 shadow-sm"
                        >
                            <Share2 size={15} />
                            <span>{t('marketplace.forwardCardBtn', 'Weiterempfehlen')}</span>
                        </Button>
                    )}
                </div>
            </div>

            {/* Forward Error Banner */}
            {forwardError && (
                <div className="p-3 bg-rose-50 border border-rose-200 rounded-2xl text-xs text-rose-800">
                    {forwardError}
                </div>
            )}

            {/* Profile Overview Card */}
            <Card className="p-6 sm:p-8 bg-white border border-theme-subtle rounded-3xl shadow-soft space-y-6">
                <div className="flex flex-col sm:flex-row items-start sm:items-center gap-6">
                    <div className="ring-4 ring-theme-primary/10 rounded-full p-1 shrink-0 bg-white shadow-sm overflow-hidden flex items-center justify-center">
                        <UserAvatar
                            size={72}
                            pictureUrl={currentPerson.profile.pictureUrl}
                            name={displayName}
                        />
                    </div>

                    <div className="space-y-2 flex-1 min-w-0">
                        <div className="flex flex-wrap items-center gap-3">
                            <h2 className="text-2xl font-black text-theme-secondary tracking-tight truncate">
                                {displayName}
                            </h2>

                            {/* Trust Badge */}
                            {currentPerson.trust === 'trusted' ? (
                                <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-bold bg-emerald-50 text-emerald-800 border border-emerald-200 shadow-xs">
                                    <CheckCircle2 size={13} className="text-emerald-600" />
                                    {t('marketplace.trustTrusted', 'Aus deinem Vertrauensnetz')}
                                </span>
                            ) : (
                                <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-bold bg-slate-100 text-slate-700 border border-slate-200 shadow-xs">
                                    <HelpCircle size={13} className="text-slate-500" />
                                    {t('marketplace.trustUnknown', 'Unbekannter Kontakt')}
                                </span>
                            )}
                        </div>

                        {/* Org / Community */}
                        {(currentPerson.profile.organization || currentPerson.profile.community) && (
                            <div className="flex flex-wrap items-center gap-4 text-xs font-medium text-theme-light">
                                {currentPerson.profile.organization && (
                                    <span className="flex items-center gap-1.5">
                                        <Building2 size={13} />
                                        {currentPerson.profile.organization}
                                    </span>
                                )}
                                {currentPerson.profile.community && (
                                    <span className="flex items-center gap-1.5">
                                        <Users size={13} />
                                        {currentPerson.profile.community}
                                    </span>
                                )}
                            </div>
                        )}

                        {/* Metadata row */}
                        <div className="flex flex-wrap items-center gap-3 pt-1 text-[11px] text-theme-light">
                            <button
                                type="button"
                                onClick={handleCopyDid}
                                title={paymentRecipientId}
                                className="inline-flex items-center gap-1.5 font-mono bg-bg-app hover:bg-theme-subtle/30 px-2.5 py-1 rounded-lg border border-theme-subtle/50 transition-colors cursor-pointer"
                            >
                                <span>{truncateUserId(paymentRecipientId)}</span>
                                {copiedDid ? <Check size={12} className="text-theme-success" /> : <Copy size={12} />}
                            </button>

                            {formattedDate && (
                                <span className="flex items-center gap-1 opacity-75">
                                    <Calendar size={12} />
                                    {t('marketplace.asOf', { date: formattedDate, defaultValue: `Stand: ${formattedDate}` })}
                                </span>
                            )}

                            {/* Source indicators */}
                            <div className="flex items-center gap-1.5">
                                {currentPerson.sources.map((src, i) => {
                                    if (src.kind === 'voucher') {
                                        return (
                                            <span
                                                key={i}
                                                className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-amber-50 text-amber-800 border border-amber-200 text-[10px] font-semibold"
                                            >
                                                <Ticket size={11} />
                                                {t('marketplace.sourceVoucher', 'Aus Gutschein-Profil')}
                                            </span>
                                        );
                                    } else if (src.kind === 'card') {
                                        return (
                                            <span
                                                key={i}
                                                className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-indigo-50 text-indigo-800 border border-indigo-200 text-[10px] font-semibold"
                                            >
                                                <CreditCard size={11} />
                                                {t('marketplace.sourceCard', {
                                                    hops: src.hops,
                                                    defaultValue: `Visitenkarte (Stufe ${src.hops})`,
                                                })}
                                            </span>
                                        );
                                    } else if (src.kind === 'sync') {
                                        return (
                                            <span
                                                key={i}
                                                className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-sky-50 text-sky-800 border border-sky-200 text-[10px] font-semibold"
                                            >
                                                <Box size={11} />
                                                {t('marketplace.sourceSync', 'Sync-Dienst')}
                                            </span>
                                        );
                                    }
                                    return null;
                                })}
                            </div>
                        </div>
                    </div>
                </div>

                {/* Maintainer Decision 7: Subtle note if person.profile.needs is present and non-empty */}
                {currentPerson.profile.needs && currentPerson.profile.needs.trim() !== '' && (
                    <div className="p-3.5 rounded-2xl bg-amber-50/70 border border-amber-200/70 flex items-start gap-2.5 text-xs text-amber-900">
                        <Heart size={16} className="text-amber-600 shrink-0 mt-0.5" />
                        <div>
                            <span className="font-bold">
                                {t('marketplace.personNeedsNote', 'Laut Gutschein gesucht:')}{' '}
                            </span>
                            <span>{currentPerson.profile.needs}</span>
                        </div>
                    </div>
                )}

                {/* Pending ads hint */}
                {currentPerson.adsPending > 0 && (
                    <div className="p-3.5 rounded-2xl bg-sky-50 border border-sky-200 text-xs text-sky-850 flex items-center justify-between gap-3">
                        <div className="flex items-center gap-2">
                            <Clock size={16} className="text-sky-600 shrink-0" />
                            <span>
                                {t('marketplace.adsPendingHint', '{{count}} Inserate werden über den Sync-Dienst nachgeladen.', {
                                    count: currentPerson.adsPending,
                                })}
                            </span>
                        </div>
                        <Button
                            size="xs"
                            variant="outline"
                            onClick={handleManualSync}
                            disabled={isManualSyncing || isCoolingDown}
                            className="shrink-0 flex items-center gap-1"
                        >
                            <RefreshCw size={12} className={isManualSyncing ? 'animate-spin' : ''} />
                            <span>{t('marketplace.fetchNowBtn', 'Jetzt abrufen')}</span>
                        </Button>
                    </div>
                )}

                {/* Disclosed Address & Contact Details */}
                {(hasAddress || currentPerson.profile.email || currentPerson.profile.phone || currentPerson.profile.url) && (
                    <div className="pt-4 border-t border-theme-subtle/50 grid grid-cols-1 sm:grid-cols-2 gap-3 text-xs text-theme-light">
                        {hasAddress && (
                            <div className="flex items-start gap-2">
                                <MapPin size={15} className="shrink-0 text-theme-primary mt-0.5" />
                                <div>
                                    {addressLine1 && <div>{addressLine1}</div>}
                                    {addressLine2 && <div>{addressLine2}</div>}
                                    {address?.country && <div>{address.country}</div>}
                                </div>
                            </div>
                        )}
                        {currentPerson.profile.email && (
                            <div className="flex items-center gap-2">
                                <Mail size={15} className="shrink-0 text-theme-primary" />
                                <a href={`mailto:${currentPerson.profile.email}`} className="hover:underline truncate">
                                    {currentPerson.profile.email}
                                </a>
                            </div>
                        )}
                        {currentPerson.profile.phone && (
                            <div className="flex items-center gap-2">
                                <Phone size={15} className="shrink-0 text-theme-primary" />
                                <a href={`tel:${currentPerson.profile.phone}`} className="hover:underline truncate">
                                    {currentPerson.profile.phone}
                                </a>
                            </div>
                        )}
                        {currentPerson.profile.url && (
                            <div className="flex items-center gap-2">
                                <Globe size={15} className="shrink-0 text-theme-primary" />
                                <button
                                    type="button"
                                    onClick={() => handleOpenWebsite(currentPerson.profile.url!)}
                                    className="hover:underline truncate text-theme-primary text-left"
                                >
                                    {currentPerson.profile.url}
                                </button>
                            </div>
                        )}
                    </div>
                )}

                {/* Accepted Standards */}
                {currentPerson.standards.length > 0 && (
                    <div className="pt-4 border-t border-theme-subtle/50">
                        <h4 className="text-[11px] font-black text-theme-light uppercase tracking-wider mb-2.5 flex items-center gap-1.5">
                            <Ticket size={12} />
                            {t('marketplace.standardsIssued', 'Angenommene Gutschein-Standards')}
                        </h4>
                        <div className="flex flex-wrap gap-2">
                            {currentPerson.standards.map((std) => (
                                <span
                                    key={std.uuid}
                                    className="inline-flex items-center gap-1.5 px-3 py-1 bg-theme-primary/5 text-theme-primary border border-theme-primary/15 rounded-xl text-xs font-semibold"
                                >
                                    <Tag size={11} />
                                    {std.name}
                                </span>
                            ))}
                        </div>
                    </div>
                )}
            </Card>

            {/* ======================================================== */}
            {/* Fähigkeiten (Skills)                                    */}
            {/* ======================================================== */}
            <div className="space-y-4">
                <div className="flex items-center justify-between">
                    <h3 className="text-xs font-black uppercase tracking-wider text-theme-secondary flex items-center gap-2">
                        <Briefcase size={14} className="text-theme-primary" />
                        <span>{t('marketplace.sectionSkills', 'Fähigkeiten & Angebote')}</span>
                        <span className="text-theme-light font-normal">({(currentPerson.skills || []).length})</span>
                    </h3>
                </div>

                {(currentPerson.skills || []).length > 0 ? (
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                        {(currentPerson.skills || []).map((skill, idx) => (
                            <div
                                key={idx}
                                className="p-4 rounded-2xl bg-white border border-theme-subtle space-y-2 shadow-xs flex flex-col justify-between"
                            >
                                <div className="space-y-1.5">
                                    <div className="flex items-start justify-between gap-2">
                                        <h4 className="text-sm font-bold text-theme-secondary leading-snug">
                                            {skill.title}
                                        </h4>
                                        <div className="flex items-center gap-1.5 flex-wrap shrink-0">
                                            {skill.isRemote && (
                                                <span
                                                    className="inline-flex items-center gap-1 text-[10px] font-semibold text-indigo-700 bg-indigo-50 border border-indigo-200 px-2 py-0.5 rounded-md"
                                                    data-testid="skill-remote-badge"
                                                >
                                                    <Globe size={10} />
                                                    <span>{t('marketplace.remoteBadge', 'Ortsunabhängig')}</span>
                                                </span>
                                            )}
                                            {skill.origin === 'voucher' ? (
                                                <span className="text-[10px] font-semibold text-amber-700 bg-amber-50 border border-amber-200 px-2 py-0.5 rounded-md">
                                                    {t('marketplace.originFromVoucher', 'laut Gutschein')}
                                                </span>
                                            ) : (
                                                <span className="text-[10px] font-semibold text-indigo-700 bg-indigo-50 border border-indigo-200 px-2 py-0.5 rounded-md">
                                                    {t('marketplace.originCard', 'Aus Visitenkarte')}
                                                </span>
                                            )}
                                        </div>
                                    </div>

                                    {skill.text && (
                                        <p className="text-xs text-theme-light leading-relaxed">{skill.text}</p>
                                    )}

                                    {skill.pricing && (
                                        <div className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-lg bg-theme-primary/5 text-theme-primary text-xs font-semibold">
                                            <Tag size={11} />
                                            <span>{skill.pricing}</span>
                                        </div>
                                    )}
                                </div>

                                {currentPerson.allowInquiries && (
                                    <div className="pt-2 flex justify-end">
                                        <Button
                                            size="xs"
                                            variant="outline"
                                            onClick={() => openInquiryForPerson(skill.title)}
                                            className="flex items-center gap-1.5"
                                        >
                                            <MessageSquare size={12} />
                                            <span>{t('marketplace.inquireBtn', 'Anfragen')}</span>
                                        </Button>
                                    </div>
                                )}
                            </div>
                        ))}
                    </div>
                ) : (
                    <div className="p-6 rounded-2xl bg-bg-app border border-theme-subtle text-center text-xs text-theme-light">
                        {t('marketplace.noSkillsListed', 'Keine Fähigkeiten angegeben.')}
                    </div>
                )}
            </div>

            {/* ======================================================== */}
            {/* Aktive Inserate (Active Ads)                            */}
            {/* ======================================================== */}
            <div className="space-y-4">
                <div className="flex items-center justify-between">
                    <h3 className="text-xs font-black uppercase tracking-wider text-theme-secondary flex items-center gap-2">
                        <Tag size={14} className="text-theme-primary" />
                        <span>{t('marketplace.activeAdsSectionTitle', 'Aktuelle Inserate')}</span>
                        <span className="text-theme-light font-normal">({(currentPerson.activeAds || []).length})</span>
                    </h3>
                </div>

                {(currentPerson.activeAds || []).length > 0 ? (
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                        {(currentPerson.activeAds || []).map((ad) => {
                            const nowSecs = Math.floor(Date.now() / 1000);
                            const daysLeft =
                                ad.expiresAt > nowSecs ? Math.ceil((ad.expiresAt - nowSecs) / 86400) : 0;
                            const authorHops = resolveAuthorHops(currentPerson.sources);

                            return (
                                <AdCard
                                    key={ad.id}
                                    ad={ad}
                                    ownerDid={paymentRecipientId}
                                    ownerName={displayName}
                                    trust={currentPerson.trust}
                                    daysLeft={daysLeft}
                                    authorHops={authorHops}
                                    mediaAutoLoadMaxHops={mediaAutoLoadMaxHops}
                                    wifiOnly={wifiOnly}
                                    onInquire={() => openInquiryForAd(ad)}
                                    onPay={() => handlePay()}
                                />
                            );
                        })}
                    </div>
                ) : (
                    <div className="p-6 rounded-2xl bg-bg-app border border-theme-subtle text-center text-xs text-theme-light">
                        {currentPerson.adsPending > 0
                            ? t('marketplace.adsLoadingFromSync', 'Inserate werden noch synchronisiert...')
                            : t('marketplace.noActiveAdsForPerson', 'Aktuell keine Inserate auf der Pinnwand.')}
                    </div>
                )}
            </div>

            {/* Dialogs */}
            <ContactDialog
                isOpen={isContactDialogOpen}
                onClose={() => setIsContactDialogOpen(false)}
                existingContact={existingContact}
                initialDid={paymentRecipientId}
                initialProfile={currentPerson.profile}
                initialSyncUrls={initialSyncUrls}
                onSave={async (saved) => {
                    await protectAction(async (pwd) => {
                        await contactService.saveContact(saved, pwd || undefined);
                    });
                    await loadContacts();
                }}
            />

            {/* Forward Share Modal */}
            {forwardCardData && (
                <ShareCardModal
                    isOpen={isShareModalOpen}
                    onClose={() => setIsShareModalOpen(false)}
                    title={t('marketplace.forwardCardTitle', 'Visitenkarte weiterempfehlen')}
                    subtitle={t('marketplace.forwardCardSubtitle', {
                        name: displayName,
                        defaultValue: `Empfiehl die Visitenkarte von ${displayName} an deine Kontakte weiter`,
                    })}
                    fileWire={forwardCardData.envelopeJson}
                    shareUri={forwardCardData.shareUri}
                    qrSvg={forwardCardData.qrSvg}
                    did={forwardCardData.did}
                />
            )}

            {/* Inquiry Modal */}
            <ModalShell
                isOpen={inquiryModalOpen}
                onClose={() => setInquiryModalOpen(false)}
                maxWidth="md"
                title={
                    <div className="flex items-center gap-2.5">
                        <div className="w-9 h-9 rounded-xl bg-theme-primary/10 text-theme-primary flex items-center justify-center">
                            <MessageSquare size={18} />
                        </div>
                        <div>
                            <h2 className="text-base font-bold text-theme-secondary">
                                {t('marketplace.inquiryModalTitle', 'Anfrage senden')}
                            </h2>
                            <p className="text-xs text-theme-light truncate">
                                {t('marketplace.inquiryTo', { name: displayName, defaultValue: 'An: {{name}}' })}
                            </p>
                        </div>
                    </div>
                }
            >
                <form onSubmit={handleSendInquiry} className="space-y-4 pt-2">
                    {!hasReachableInquirySync && (
                        <div className="p-3 rounded-xl bg-amber-50 border border-amber-200 text-xs text-amber-900 space-y-2">
                            <p>{t('marketplace.inquiryNoSyncHint')}</p>
                            <Button
                                type="button"
                                size="xs"
                                variant="secondary"
                                onClick={() => {
                                    setInquiryModalOpen(false);
                                    setIsContactDialogOpen(true);
                                }}
                            >
                                {t('marketplace.addToContacts', 'Ins Adressbuch')}
                            </Button>
                        </div>
                    )}
                    {inquiryFeedback && (
                        <div
                            className={`p-3 rounded-xl text-xs border ${
                                inquiryFeedback.type === 'success'
                                    ? 'bg-emerald-50 text-emerald-800 border-emerald-200'
                                    : 'bg-rose-50 text-rose-800 border-rose-200'
                            }`}
                        >
                            {inquiryFeedback.message}
                        </div>
                    )}

                    <div className="p-3 rounded-xl bg-bg-app border border-theme-subtle text-xs text-theme-secondary space-y-1">
                        <span className="text-[10px] uppercase font-bold text-theme-light tracking-wider block">
                            {t('marketplace.inquiryTargetLabel', 'Betrifft')}
                        </span>
                        <p className="font-semibold">
                            {inquiryTarget.kind === 'ad'
                                ? `Inserat: ${inquiryTarget.ad_title}`
                                : inquiryTarget.skill_title
                                ? `Fähigkeit: ${inquiryTarget.skill_title}`
                                : t('marketplace.generalInquiry', 'Allgemeine Anfrage')}
                        </p>
                    </div>

                    <div className="space-y-1.5">
                        <div className="flex items-center justify-between">
                            <label className="text-[11px] font-bold text-theme-secondary">
                                {t('marketplace.inquiryMessageLabel', 'Nachricht (max. 280 Zeichen)')}
                            </label>
                            <span className="text-[10px] text-theme-light">
                                {inquiryMessage.trim().length} / 280
                            </span>
                        </div>
                        <textarea
                            value={inquiryMessage}
                            onChange={(e) => setInquiryMessage(e.target.value)}
                            placeholder={t(
                                'marketplace.inquiryMessagePlaceholder',
                                'Kurze Notiz, z. B. gewünschter Abholzeitpunkt...'
                            )}
                            className="w-full bg-white border border-theme-subtle rounded-2xl p-3 text-xs font-medium text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none min-h-[100px] resize-y"
                            maxLength={280}
                            required
                        />
                    </div>

                    <div className="flex items-center justify-end gap-2 pt-2 border-t border-theme-subtle/50">
                        <Button
                            variant="secondary"
                            type="button"
                            onClick={() => setInquiryModalOpen(false)}
                            disabled={isSendingInquiry}
                        >
                            {t('common.cancel', 'Abbrechen')}
                        </Button>
                        <Button
                            variant="primary"
                            type="submit"
                            disabled={isSendingInquiry || !inquiryMessage.trim()}
                        >
                            {isSendingInquiry
                                ? t('marketplace.sendingInquiry', 'Wird gesendet...')
                                : t('marketplace.sendInquiryBtn', 'Anfrage senden')}
                        </Button>
                    </div>
                </form>
            </ModalShell>
        </div>
    );
};
