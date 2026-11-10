// src/components/marketplace/MarketPersonView.tsx
import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import Avatar from 'boring-avatars';
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
    Store,
    Calendar,
    ArrowLeft,
    Share2,
    RefreshCw
} from 'lucide-react';
import { FrontendMarketPerson, Contact, FrontendMarketListing, FrontendForwardCardExport } from '../../types';
import { useNavigation } from '../../context/NavigationContext';
import { useSession } from '../../context/SessionContext';
import { contactService } from '../../services/contactService';
import { marketService } from '../../services/marketService';
import { useRateLimitCooldown } from '../../hooks/useRateLimitCooldown';
import ContactDialog from '../ContactDialog';
import { ShareCardModal } from './ShareCardModal';
import { copyToClipboard } from '../../utils/clipboard';
import { truncateUserId } from '../../utils/userIdHelper';
import { translateError } from '../../utils/errorHelper';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { openUrl } from '@tauri-apps/plugin-opener';

export interface MarketPersonViewProps {
    person: FrontendMarketPerson;
    onBack: () => void;
}

export function MarketPersonView({ person, onBack }: MarketPersonViewProps) {
    const { t } = useTranslation();
    const { navigate } = useNavigation();
    const { protectAction } = useSession();

    const [currentPerson, setCurrentPerson] = useState<FrontendMarketPerson>(person);
    const [isManualSyncing, setIsManualSyncing] = useState(false);
    const { cooldownSeconds, isCoolingDown, checkAndTriggerCooldown } = useRateLimitCooldown();
    const [contacts, setContacts] = useState<Contact[]>([]);
    const [isContactDialogOpen, setIsContactDialogOpen] = useState(false);
    const [copiedDid, setCopiedDid] = useState(false);
    const [isForwarding, setIsForwarding] = useState(false);
    const [forwardCardData, setForwardCardData] = useState<FrontendForwardCardExport | null>(null);
    const [isShareModalOpen, setIsShareModalOpen] = useState(false);
    const [forwardError, setForwardError] = useState<string | null>(null);

    useEffect(() => {
        setCurrentPerson(person);
    }, [person]);

    const handleManualSync = async () => {
        if (isCoolingDown) return;
        setIsManualSyncing(true);
        try {
            const res = await marketService.syncMarketChannels();
            if (res) {
                checkAndTriggerCooldown(res);
            }
            const list = await marketService.getMarketView();
            const updated = (list || []).find(p => p.did === currentPerson.did);
            if (updated) {
                setCurrentPerson(updated);
            }
        } catch (err) {
            checkAndTriggerCooldown(err);
        } finally {
            setIsManualSyncing(false);
        }
    };

    const loadContacts = useCallback(async () => {
        try {
            const list = await contactService.getContacts();
            setContacts(list || []);
        } catch {
            // Contacts might not be available if not unlocked
        }
    }, []);

    useEffect(() => {
        loadContacts();
    }, [loadContacts]);

    const existingContact = contacts.find(c => c.did === currentPerson.did);

    const handleCopyDid = async () => {
        const success = await copyToClipboard(currentPerson.did);
        if (success) {
            setCopiedDid(true);
            setTimeout(() => setCopiedDid(false), 2000);
        }
    };

    const handleSaveContact = async (saved: Contact) => {
        await protectAction(async (pwd) => {
            await contactService.saveContact(saved, pwd || undefined);
        });
        await loadContacts();
    };

    const handlePay = () => {
        navigate({ view: 'send_vouchers', initialRecipientId: currentPerson.did });
    };

    const hasCardSource = currentPerson.sources.some(s => s.kind === 'card');

    const handleForwardCard = async () => {
        setIsForwarding(true);
        setForwardError(null);
        try {
            const exportData = await protectAction(async (pwd) => {
                return await marketService.exportForwardCard(currentPerson.did, pwd || undefined);
            });
            if (exportData) {
                setForwardCardData(exportData);
                setIsShareModalOpen(true);
            }
        } catch (err) {
            setForwardError(translateError(err, t));
        } finally {
            setIsForwarding(false);
        }
    };

    const handleOpenWebsite = async (url: string) => {
        try {
            const fullUrl = url.startsWith('http://') || url.startsWith('https://') ? url : `https://${url}`;
            await openUrl(fullUrl);
        } catch {
            // Open fallback
        }
    };

    const fullName = [currentPerson.profile.firstName, currentPerson.profile.lastName].filter(Boolean).join(' ');
    const displayName = fullName || currentPerson.profile.organization || currentPerson.profile.community || truncateUserId(currentPerson.did);

    const hasCardListings = currentPerson.listings.some(l => l.origin === 'card');
    const hasVoucherListings = currentPerson.listings.some(l => l.origin === 'voucher');
    const hasBothOrigins = hasCardListings && hasVoucherListings;

    const sortListings = (list: FrontendMarketListing[]) => {
        return [...list].sort((a, b) => {
            if (a.origin === 'card' && b.origin === 'voucher') return -1;
            if (a.origin === 'voucher' && b.origin === 'card') return 1;
            return 0;
        });
    };

    const offers = sortListings(currentPerson.listings.filter(l => l.listingType === 'offer'));
    const requests = sortListings(currentPerson.listings.filter(l => l.listingType === 'request'));

    const formattedDate = currentPerson.asOf ? new Date(currentPerson.asOf * 1000).toLocaleDateString() : '';

    const address = currentPerson.profile.address;
    const addressLine1 = [address?.street, address?.houseNumber].filter(Boolean).join(' ');
    const addressLine2 = [address?.zipCode, address?.city].filter(Boolean).join(' ');
    const hasAddress = !!(address?.fullAddress || addressLine1 || addressLine2 || address?.country);

    return (
        <div className="max-w-4xl mx-auto space-y-6">
            {/* Header with Back button */}
            <div className="flex items-center justify-between gap-4">
                <Button 
                    variant="outline" 
                    size="sm" 
                    onClick={onBack}
                    className="gap-2 text-theme-light hover:text-theme-secondary"
                >
                    <ArrowLeft size={16} />
                    <span>{t('common.back')}</span>
                </Button>

                <div className="flex items-center gap-2">
                    {/* Primary Pay Action */}
                    <Button 
                        onClick={handlePay} 
                        size="sm" 
                        className="gap-2 shadow-sm font-semibold"
                    >
                        <Send size={15} />
                        <span>{t('marketplace.pay')}</span>
                    </Button>

                    {/* Contact Action */}
                    <Button 
                        variant={existingContact ? "secondary" : "outline"} 
                        size="sm" 
                        onClick={() => setIsContactDialogOpen(true)}
                        className="gap-2 shadow-sm"
                    >
                        {existingContact ? (
                            <>
                                <UserCheck size={15} className="text-theme-success" />
                                <span>{t('marketplace.editContact')}</span>
                            </>
                        ) : (
                            <>
                                <UserPlus size={15} />
                                <span>{t('marketplace.addToContacts')}</span>
                            </>
                        )}
                    </Button>

                    {/* Recommend / Forward Action */}
                    {(currentPerson.canForward || hasCardSource) && (
                        <Button 
                            variant="outline" 
                            size="sm" 
                            disabled={!currentPerson.canForward || isForwarding}
                            onClick={handleForwardCard}
                            title={!currentPerson.canForward ? t('marketplace.forwardMaxHopsReached') : t('marketplace.forwardCardBtn')}
                            className="gap-2 shadow-sm"
                        >
                            <Share2 size={15} />
                            <span>{t('marketplace.forwardCardBtn')}</span>
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
            <Card className="p-6 sm:p-8 bg-white border border-theme-subtle rounded-3xl shadow-soft">
                <div className="flex flex-col sm:flex-row items-start sm:items-center gap-6">
                    <div className="ring-4 ring-theme-primary/10 rounded-full p-1 shrink-0 bg-white shadow-sm">
                        <Avatar
                            size={72}
                            name={displayName}
                            variant="beam"
                            colors={["#E63946", "#F4A261", "#E76F51", "#2A9D8F", "#2B1B17"]}
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
                                    {t('marketplace.trustTrusted')}
                                </span>
                            ) : (
                                <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-bold bg-slate-100 text-slate-700 border border-slate-200 shadow-xs">
                                    <HelpCircle size={13} className="text-slate-500" />
                                    {t('marketplace.trustUnknown')}
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

                        {/* DID and Metadata */}
                        <div className="flex flex-wrap items-center gap-3 pt-1 text-[11px] text-theme-light">
                            <button
                                type="button"
                                onClick={handleCopyDid}
                                title={currentPerson.did}
                                className="inline-flex items-center gap-1.5 font-mono bg-bg-app hover:bg-theme-subtle/30 px-2.5 py-1 rounded-lg border border-theme-subtle/50 transition-colors cursor-pointer"
                            >
                                <span>{truncateUserId(currentPerson.did)}</span>
                                {copiedDid ? <Check size={12} className="text-theme-success" /> : <Copy size={12} />}
                            </button>

                            {formattedDate && (
                                <span className="flex items-center gap-1 opacity-75">
                                    <Calendar size={12} />
                                    {t('marketplace.asOf', { date: formattedDate })}
                                </span>
                            )}

                            {/* Source indicators */}
                            <div className="flex items-center gap-1.5">
                                {currentPerson.sources.map((src, i) => {
                                    if (src.kind === 'voucher') {
                                        return (
                                            <span 
                                                key={i} 
                                                title={t('marketplace.sourceVoucher')}
                                                className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-amber-50 text-amber-800 border border-amber-200 text-[10px] font-semibold"
                                            >
                                                <Ticket size={11} />
                                                {t('marketplace.sourceVoucher')}
                                            </span>
                                        );
                                    } else if (src.kind === 'card') {
                                        return (
                                            <span 
                                                key={i} 
                                                title={t('marketplace.sourceCard', { hops: src.hops })}
                                                className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-indigo-50 text-indigo-800 border border-indigo-200 text-[10px] font-semibold"
                                            >
                                                <CreditCard size={11} />
                                                {t('marketplace.sourceCard', { hops: src.hops })}
                                            </span>
                                        );
                                    } else if (src.kind === 'sync') {
                                        return (
                                            <span 
                                                key={i} 
                                                title={t('marketplace.sourceSync')}
                                                className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-sky-50 text-sky-800 border border-sky-200 text-[10px] font-semibold"
                                            >
                                                <Box size={11} />
                                                {t('marketplace.sourceSync')}
                                            </span>
                                        );
                                    }
                                    return null;
                                })}
                            </div>
                        </div>
                    </div>
                </div>

                {/* Issued / Accepted Voucher Standards */}
                {currentPerson.standards.length > 0 && (
                    <div className="mt-6 pt-5 border-t border-theme-subtle/50">
                        <h4 className="text-[11px] font-black text-theme-light uppercase tracking-wider mb-2.5 flex items-center gap-1.5">
                            <Ticket size={12} />
                            {t('marketplace.standardsIssued')}
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

            {/* Inserate / Listings Section */}
            <div className="space-y-4">
                {currentPerson.listings.length === 0 ? (
                    <Card className="p-8 text-center bg-white border border-theme-subtle rounded-3xl">
                        {Array.isArray(currentPerson.cardSyncs) && currentPerson.cardSyncs.length > 0 ? (
                            <div className="max-w-md mx-auto space-y-3">
                                <div className="w-12 h-12 rounded-2xl bg-amber-500/10 text-amber-600 flex items-center justify-center mx-auto">
                                    <RefreshCw size={24} className={isManualSyncing ? 'animate-spin' : ''} />
                                </div>
                                <h3 className="text-base font-bold text-theme-secondary">
                                    {t('marketplace.waitingForMutualHandshakeTitle', 'Kontakt gespeichert – Freischaltung ausstehend')}
                                </h3>
                                <p className="text-xs text-theme-light leading-relaxed">
                                    {t('marketplace.waitingForMutualHandshakeDesc', 'Inserate werden geladen, sobald euch eure Adressbücher gegenseitig freigeschaltet haben.')}
                                </p>
                                <div className="pt-2">
                                    <Button
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        onClick={handleManualSync}
                                        disabled={isManualSyncing || isCoolingDown}
                                        className="gap-1.5 text-xs mx-auto"
                                    >
                                        <RefreshCw size={13} className={isManualSyncing ? 'animate-spin' : ''} />
                                        <span>
                                            {isCoolingDown
                                                ? t('marketplace.syncRateLimitWait', { seconds: cooldownSeconds, defaultValue: `Bitte noch ${cooldownSeconds}s warten...` })
                                                : isManualSyncing
                                                ? t('common.syncing', 'Synchronisiere...')
                                                : t('marketplace.fetchNowBtn', 'Jetzt abrufen')}
                                        </span>
                                    </Button>
                                </div>
                            </div>
                        ) : (
                            <div className="max-w-md mx-auto space-y-3">
                                <div className="w-12 h-12 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center mx-auto">
                                    <Store size={24} />
                                </div>
                                <h3 className="text-base font-bold text-theme-secondary">
                                    {t('marketplace.acceptsOwnVouchers')}
                                </h3>
                                <p className="text-xs text-theme-light leading-relaxed">
                                    {t('marketplace.noListings')}
                                </p>
                            </div>
                        )}
                    </Card>
                ) : (
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                        {/* Biete / Offers */}
                        <div className="space-y-3">
                            <h3 className="text-xs font-black text-theme-light uppercase tracking-widest flex items-center gap-2 px-1">
                                <span className="w-2 h-2 rounded-full bg-emerald-500"></span>
                                {t('marketplace.tabOffers')} ({offers.length})
                            </h3>
                            {offers.length === 0 ? (
                                <p className="text-xs text-theme-light/60 italic p-4 bg-white border border-theme-subtle rounded-2xl">
                                    {t('marketplace.noListings')}
                                </p>
                            ) : (
                                <div className="space-y-3">
                                    {offers.map((listing, idx) => (
                                        <Card key={idx} className="p-4 bg-white border border-theme-subtle rounded-2xl shadow-xs space-y-2.5">
                                            <div className="flex items-start justify-between gap-3">
                                                <h4 className="text-sm font-bold text-theme-secondary leading-snug">
                                                    {listing.title}
                                                </h4>
                                                {listing.pricing && (
                                                    <span className="shrink-0 px-2 py-0.5 rounded-lg bg-emerald-50 text-emerald-800 border border-emerald-200 text-xs font-bold">
                                                        {listing.pricing}
                                                    </span>
                                                )}
                                            </div>
                                            {listing.text && (
                                                <p className="text-xs text-theme-secondary/80 leading-relaxed whitespace-pre-wrap">
                                                    {listing.text}
                                                </p>
                                            )}
                                            <div className="pt-2 border-t border-theme-subtle/30 flex items-center justify-between text-[10px] text-theme-light">
                                                <span className="flex items-center gap-1 font-medium">
                                                    {listing.origin === 'card'
                                                        ? t('marketplace.originCard')
                                                        : hasBothOrigins
                                                        ? t('marketplace.originFromVoucher')
                                                        : t('marketplace.originVoucher')}
                                                </span>
                                            </div>
                                        </Card>
                                    ))}
                                </div>
                            )}
                        </div>

                        {/* Suche / Requests */}
                        <div className="space-y-3">
                            <h3 className="text-xs font-black text-theme-light uppercase tracking-widest flex items-center gap-2 px-1">
                                <span className="w-2 h-2 rounded-full bg-amber-500"></span>
                                {t('marketplace.tabRequests')} ({requests.length})
                            </h3>
                            {requests.length === 0 ? (
                                <p className="text-xs text-theme-light/60 italic p-4 bg-white border border-theme-subtle rounded-2xl">
                                    {t('marketplace.noListings')}
                                </p>
                            ) : (
                                <div className="space-y-3">
                                    {requests.map((listing, idx) => (
                                        <Card key={idx} className="p-4 bg-white border border-theme-subtle rounded-2xl shadow-xs space-y-2.5">
                                            <div className="flex items-start justify-between gap-3">
                                                <h4 className="text-sm font-bold text-theme-secondary leading-snug">
                                                    {listing.title}
                                                </h4>
                                                {listing.pricing && (
                                                    <span className="shrink-0 px-2 py-0.5 rounded-lg bg-amber-50 text-amber-800 border border-amber-200 text-xs font-bold">
                                                        {listing.pricing}
                                                    </span>
                                                )}
                                            </div>
                                            {listing.text && (
                                                <p className="text-xs text-theme-secondary/80 leading-relaxed whitespace-pre-wrap">
                                                    {listing.text}
                                                </p>
                                            )}
                                            <div className="pt-2 border-t border-theme-subtle/30 flex items-center justify-between text-[10px] text-theme-light">
                                                <span className="flex items-center gap-1 font-medium">
                                                    {listing.origin === 'card'
                                                        ? t('marketplace.originCard')
                                                        : hasBothOrigins
                                                        ? t('marketplace.originFromVoucher')
                                                        : t('marketplace.originVoucher')}
                                                </span>
                                            </div>
                                        </Card>
                                    ))}
                                </div>
                            )}
                        </div>
                    </div>
                )}
            </div>

            {/* Contact Details Card */}
            {(currentPerson.profile.email || currentPerson.profile.phone || currentPerson.profile.url || hasAddress || currentPerson.profile.coordinates) && (
                <Card className="p-6 bg-white border border-theme-subtle rounded-3xl shadow-soft space-y-4">
                    <h3 className="text-xs font-black text-theme-light uppercase tracking-widest flex items-center gap-2">
                        {t('marketplace.contactDetails')}
                    </h3>
                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 text-xs">
                        {currentPerson.profile.email && (
                            <a 
                                href={`mailto:${currentPerson.profile.email}`} 
                                className="flex items-center gap-2.5 p-3 rounded-2xl bg-bg-app border border-theme-subtle/40 text-theme-secondary hover:text-theme-primary transition-colors"
                            >
                                <Mail size={15} className="text-theme-light shrink-0" />
                                <span className="truncate">{currentPerson.profile.email}</span>
                            </a>
                        )}

                        {currentPerson.profile.phone && (
                            <a 
                                href={`tel:${currentPerson.profile.phone}`} 
                                className="flex items-center gap-2.5 p-3 rounded-2xl bg-bg-app border border-theme-subtle/40 text-theme-secondary hover:text-theme-primary transition-colors"
                            >
                                <Phone size={15} className="text-theme-light shrink-0" />
                                <span className="truncate">{currentPerson.profile.phone}</span>
                            </a>
                        )}

                        {currentPerson.profile.url && (
                            <button 
                                type="button"
                                onClick={() => handleOpenWebsite(currentPerson.profile.url || '')}
                                className="flex items-center gap-2.5 p-3 rounded-2xl bg-bg-app border border-theme-subtle/40 text-theme-secondary hover:text-theme-primary transition-colors text-left cursor-pointer"
                            >
                                <Globe size={15} className="text-theme-light shrink-0" />
                                <span className="truncate">{currentPerson.profile.url}</span>
                            </button>
                        )}

                        {hasAddress && (
                            <div className="flex items-start gap-2.5 p-3 rounded-2xl bg-bg-app border border-theme-subtle/40 text-theme-secondary">
                                <MapPin size={15} className="text-theme-light shrink-0 mt-0.5" />
                                <div className="space-y-0.5">
                                    {address?.fullAddress ? (
                                        <p>{address.fullAddress}</p>
                                    ) : (
                                        <>
                                            {addressLine1 && <p>{addressLine1}</p>}
                                            {addressLine2 && <p>{addressLine2}</p>}
                                            {address?.country && <p>{address.country}</p>}
                                        </>
                                    )}
                                </div>
                            </div>
                        )}

                        {currentPerson.profile.coordinates && (
                            <div className="flex items-center gap-2.5 p-3 rounded-2xl bg-bg-app border border-theme-subtle/40 text-theme-secondary font-mono text-[11px]">
                                <MapPin size={15} className="text-theme-light shrink-0" />
                                <span className="truncate">{currentPerson.profile.coordinates}</span>
                            </div>
                        )}
                    </div>
                </Card>
            )}

            {/* Contact Dialog */}
            <ContactDialog
                isOpen={isContactDialogOpen}
                onClose={() => setIsContactDialogOpen(false)}
                onSave={handleSaveContact}
                existingContact={existingContact}
                initialDid={currentPerson.did}
                initialProfile={currentPerson.profile}
            />

            {/* Share / Forward Modal */}
            {forwardCardData && (
                <ShareCardModal
                    isOpen={isShareModalOpen}
                    onClose={() => {
                        setIsShareModalOpen(false);
                        setForwardCardData(null);
                    }}
                    title={t('marketplace.forwardCardTitle')}
                    subtitle={t('marketplace.forwardCardSubtitle', { name: displayName })}
                    envelopeJson={forwardCardData.envelopeJson}
                    shareUri={forwardCardData.shareUri}
                    qrSvg={forwardCardData.qrSvg}
                    did={forwardCardData.did}
                />
            )}
        </div>
    );
}
