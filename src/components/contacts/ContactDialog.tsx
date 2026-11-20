import React, { useState, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { Contact, PublicProfile, FrontendTrustChangeImpact } from '../../types';
import { FrontendSyncDescriptor } from '../../types/market';
import { contactService } from '../../services/contactService';
import { marketService } from '../../services/marketService';
import { profileService } from '../../services/profileService';
import { useOptionalSession } from '../../context/SessionContext';
import { UserAvatar } from '../ui/UserAvatar';
import { translateError } from '../../utils/errorHelper';
import { HelpIcon } from '../ui/HelpIcon';
import { Advanced } from '../ui/Advanced';
import { TechId } from '../ui/TechId';
import { ModalShell } from '../ui/ModalShell';
import { TrustLevelRadioCards } from './TrustLevelRadioCards';
import { readFromClipboard } from '../../utils/clipboard';
import { parseContactCard, parseHumocoInput, normalizeSyncUrlInput } from '../../utils/humocoUri';
import { isExactSameUserId, isSameRootIdentity, truncateUserId } from '../../utils/userIdHelper';
import { notifySyncStatusUpdated } from '../../utils/syncEvents';
import { Clipboard, CheckCircle2, AlertCircle, Server, Trash2 } from 'lucide-react';

interface ContactDialogProps {
    isOpen: boolean;
    onClose: () => void;
    onSave: (contact: Contact) => Promise<void>;
    existingContact?: Contact | null;
    initialProfile?: Partial<PublicProfile> | null;
    initialDid?: string;
    initialSyncUrls?: string[];
}

const ContactDialog: React.FC<ContactDialogProps> = ({
    isOpen,
    onClose,
    onSave,
    existingContact,
    initialProfile,
    initialDid,
    initialSyncUrls,
}) => {
    const { t } = useTranslation();
    const session = useOptionalSession();
    const [did, setDid] = useState(initialDid || '');
    const [firstName, setFirstName] = useState('');
    const [lastName, setLastName] = useState('');
    const [organization, setOrganization] = useState('');
    const [tags, setTags] = useState<string[]>([]);
    const [notes, setNotes] = useState('');
    const [newTag, setNewTag] = useState('');
    const [trustLevel, setTrustLevel] = useState<number | null>(existingContact?.trustLevel ?? null);
    const [trustImpact, setTrustImpact] = useState<FrontendTrustChangeImpact | null>(null);
    const [syncs, setSyncs] = useState<FrontendSyncDescriptor[]>(existingContact?.syncs || []);
    const [syncInputUrl, setSyncInputUrl] = useState('');
    const [isProbingSync, setIsProbingSync] = useState(false);
    const [syncError, setSyncError] = useState<string | null>(null);
    const [confirmDeleteKey, setConfirmDeleteKey] = useState<string | null>(null);
    const [isSaving, setIsSaving] = useState(false);
    const [isResolvingCard, setIsResolvingCard] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [ownUserId, setOwnUserId] = useState<string>('');
    const [ownUserIdLoaded, setOwnUserIdLoaded] = useState<boolean>(false);
    const [allContacts, setAllContacts] = useState<Contact[]>([]);
    const [hasManualTrustOverride, setHasManualTrustOverride] = useState<boolean>(false);
    const isMountedRef = useRef(true);
    const cardResolveGenRef = useRef(0);
    const firstNameRef = useRef(firstName);
    firstNameRef.current = firstName;
    const lastNameRef = useRef(lastName);
    lastNameRef.current = lastName;
    const organizationRef = useRef(organization);
    organizationRef.current = organization;

    useEffect(() => {
        isMountedRef.current = true;
        return () => {
            isMountedRef.current = false;
            cardResolveGenRef.current += 1;
        };
    }, []);

    useEffect(() => {
        if (!isOpen) {
            setOwnUserIdLoaded(false);
            setAllContacts([]);
            return;
        }
        let cancelled = false;
        setOwnUserIdLoaded(false);
        Promise.resolve(profileService.getUserId?.())
            .then((uid) => {
                if (!cancelled && typeof uid === 'string' && uid.trim()) {
                    setOwnUserId(uid.trim());
                }
            })
            .catch(() => {})
            .finally(() => {
                if (!cancelled) setOwnUserIdLoaded(true);
            });
        if (!existingContact) {
            Promise.resolve(contactService.getContacts?.())
                .then((list) => {
                    if (!cancelled && Array.isArray(list)) {
                        setAllContacts(list);
                    }
                })
                .catch(() => {});
        }
        return () => {
            cancelled = true;
        };
    }, [isOpen, existingContact]);

    const isExactSelf = isExactSameUserId(did, ownUserId);
    const isOwnSubaccount = !isExactSelf && isSameRootIdentity(did, ownUserId);
    const siblingContact =
        !existingContact && !isExactSelf && !isOwnSubaccount && did.trim()
            ? allContacts.find(
                  (c) => !isExactSameUserId(c.did, did) && isSameRootIdentity(c.did, did),
              ) || null
            : null;

    useEffect(() => {
        setHasManualTrustOverride(false);
    }, [did, existingContact, isOpen]);

    const displayedTrustLevel =
        !existingContact && siblingContact && !hasManualTrustOverride
            ? (siblingContact.trustLevel ?? null)
            : trustLevel;

    useEffect(() => {
        let cancelled = false;
        if (!did || !did.trim() || !ownUserIdLoaded || isExactSelf || isOwnSubaccount) {
            setTrustImpact(null);
            if ((isExactSelf || isOwnSubaccount) && trustLevel !== null) {
                setTrustLevel(null);
            }
            return;
        }
        const targetLvl = displayedTrustLevel ?? 0;
        marketService.previewContactTrustChange(did.trim(), targetLvl)
            .then((impact) => {
                if (!cancelled) {
                    setTrustImpact(impact);
                }
            })
            .catch(() => {
                if (!cancelled) setTrustImpact(null);
            });
        return () => {
            cancelled = true;
        };
    }, [did, trustLevel, displayedTrustLevel, ownUserIdLoaded, isExactSelf, isOwnSubaccount]);

    const predefinedTags = ["Friends", "Business", "Favorites"];
    const availableTags = Array.from(new Set([...predefinedTags, ...tags]));

    useEffect(() => {
        if (existingContact) {
            setDid(existingContact.did);
            setFirstName(existingContact.profile.firstName || '');
            setLastName(existingContact.profile.lastName || '');
            setOrganization(existingContact.profile.organization || '');
            setTags(existingContact.tags);
            setNotes(existingContact.notes || '');
            setTrustLevel(existingContact.trustLevel ?? null);
            setSyncs(existingContact.syncs ? [...existingContact.syncs].slice(0, 2) : []);
        } else {
            const initSyncs: FrontendSyncDescriptor[] = (initialSyncUrls || []).slice(0, 2).map(url => ({
                writeUrl: url,
                readUrl: '',
                dropPub: '',
                failCount: 0,
                status: 'unverified',
            }));
            if (initialProfile) {
                setFirstName(initialProfile.firstName || '');
                setLastName(initialProfile.lastName || '');
                setOrganization(initialProfile.organization || '');
                if (initialDid) setDid(initialDid);
                setTags([]);
                setNotes('');
                setTrustLevel(null);
                setSyncs(initSyncs);
            } else {
                // Reset for new contact
                setDid(initialDid || '');
                setFirstName('');
                setLastName('');
                setOrganization('');
                setTags([]);
                setNotes('');
                setTrustLevel(null);
                setSyncs(initSyncs);
            }
        }
        cardResolveGenRef.current += 1;
        setIsResolvingCard(false);
        setError(null);
        setSyncInputUrl('');
        setSyncError(null);
        setConfirmDeleteKey(null);
    }, [existingContact, initialProfile, initialDid, initialSyncUrls, isOpen]);

    const applyContactCard = async (rawText: string) => {
        const parsedInput = parseHumocoInput(rawText);
        if (parsedInput?.kind === 'card') {
            const resolveGen = ++cardResolveGenRef.current;
            setIsResolvingCard(true);
            try {
                const resolved = await marketService.resolveCardRecipient(parsedInput.payload);
                if (!isMountedRef.current || cardResolveGenRef.current !== resolveGen) {
                    return;
                }
                setDid(resolved.recipientId);
                if (
                    resolved.displayName &&
                    !firstNameRef.current &&
                    !lastNameRef.current &&
                    !organizationRef.current
                ) {
                    const parts = resolved.displayName.trim().split(/\s+/);
                    if (parts.length === 1) {
                        setFirstName(parts[0]);
                    } else {
                        setFirstName(parts[0]);
                        setLastName(parts.slice(1).join(' '));
                    }
                }
            } catch (e) {
                if (!isMountedRef.current || cardResolveGenRef.current !== resolveGen) {
                    return;
                }
                setError(translateError(e, t));
            } finally {
                if (isMountedRef.current && cardResolveGenRef.current === resolveGen) {
                    setIsResolvingCard(false);
                }
            }
            return;
        }
        cardResolveGenRef.current += 1;
        setIsResolvingCard(false);
        const parsed = parseContactCard(rawText);
        if (parsed) {
            setDid(parsed.did);
            if (
                parsed.name &&
                !firstNameRef.current &&
                !lastNameRef.current &&
                !organizationRef.current
            ) {
                const parts = parsed.name.trim().split(/\s+/);
                if (parts.length === 1) {
                    setFirstName(parts[0]);
                } else {
                    setFirstName(parts[0]);
                    setLastName(parts.slice(1).join(' '));
                }
            }
            if (parsed.syncUrls && parsed.syncUrls.length > 0) {
                setSyncs(prev => {
                    const newSyncs = [...prev];
                    for (const url of parsed.syncUrls!.slice(0, 2)) {
                        if (newSyncs.length < 2 && !newSyncs.some(d => d.writeUrl === url || d.readUrl === url)) {
                            newSyncs.push({
                                writeUrl: url,
                                readUrl: '',
                                dropPub: '',
                                failCount: 0,
                                status: 'unverified',
                            });
                        }
                    }
                    return newSyncs;
                });
            }
        }
    };

    const handlePasteFromClipboard = async () => {
        setError(null);
        try {
            const { text, error: readError } = await readFromClipboard();
            if (readError) {
                setError(t('contacts.clipboardReadFailed'));
                return;
            }
            if (!text || !text.trim()) {
                setError(t('contacts.emptyClipboard'));
                return;
            }
            const parsedInput = parseHumocoInput(text);
            const parsed = parseContactCard(text);
            if (!parsed && parsedInput?.kind !== 'card') {
                setError(t('contacts.invalidClipboardContent'));
                return;
            }
            await applyContactCard(text);
        } catch (e) {
            console.error("Failed to read from clipboard:", e);
            setError(t('contacts.clipboardReadFailed'));
        }
    };

    const handleProbeAndAddSync = async () => {
        const normalized = normalizeSyncUrlInput(syncInputUrl);
        if (!normalized) return;
        if (normalized !== syncInputUrl) {
            setSyncInputUrl(normalized);
        }
        if (syncs.length >= 2) {
            setSyncError(t('contacts.syncMaxReached'));
            return;
        }
        if (syncs.some(d => d.writeUrl.toLowerCase() === normalized.toLowerCase() || d.readUrl.toLowerCase() === normalized.toLowerCase())) {
            setSyncError(t('contacts.syncAlreadyAdded'));
            return;
        }
        setIsProbingSync(true);
        setSyncError(null);
        try {
            const info = await marketService.probePublicSync(normalized);
            if (!info || !info.dropPub) {
                setSyncError(t('contacts.syncProbeFailed'));
                return;
            }
            const newSync: FrontendSyncDescriptor = {
                writeUrl: info.writeUrl,
                readUrl: info.readUrl,
                dropPub: info.dropPub,
                lastSeenAt: Math.floor(Date.now() / 1000),
                failCount: 0,
                status: 'unverified',
                lastProbedAt: undefined,
            };
            setSyncs(prev => [...prev, newSync].slice(0, 2));
            setSyncInputUrl('');
        } catch (err) {
            console.error("Failed to probe sync service:", err);
            setSyncError(t('contacts.syncProbeFailed'));
        } finally {
            setIsProbingSync(false);
        }
    };

    const handleDeleteSync = (targetKey: string) => {
        setSyncs(prev => prev.filter(d => (d.writeUrl || d.readUrl) !== targetKey));
        setConfirmDeleteKey(null);
    };

    if (!isOpen) return null;

    const handleSave = async () => {
        if (isResolvingCard || !did.trim()) return;
        if (isExactSelf) {
            setError(t('contacts.selfActiveProfileHint'));
            return;
        }

        setIsSaving(true);
        setError(null);
        try {
            const effectiveTrustLevel = isOwnSubaccount ? null : displayedTrustLevel;
            const contact: Contact = {
                ...(existingContact || {}),
                did: did.trim(),
                profile: {
                    ...(existingContact?.profile || initialProfile || {}),
                    id: did.trim(),
                    firstName: firstName.trim() || undefined,
                    lastName: lastName.trim() || undefined,
                    organization: organization.trim() || undefined,
                },
                tags,
                notes: notes.trim() || undefined,
                addedAt: existingContact?.addedAt || new Date().toISOString(),
                trustLevel: effectiveTrustLevel,
                desiredHops: effectiveTrustLevel === 0 ? 0 : null,
                hasPeerList: existingContact?.hasPeerList,
                peerListUpdatedAt: existingContact?.peerListUpdatedAt,
                syncs: syncs.length > 0 ? syncs : [],
            };
            await onSave(contact);
            if (session?.hasCachedSession !== false) {
                marketService.probeContactSyncs?.(false)
                    ?.then(() => notifySyncStatusUpdated())
                    ?.catch(() => {});
            }
            onClose();
        } catch (e: unknown) {
            console.error("Failed to save contact:", e);
            setError(translateError(e, t));
        } finally {
            setIsSaving(false);
        }
    };

    const toggleTag = (tag: string) => {
        if (tags.includes(tag)) {
            setTags(tags.filter(t => t !== tag));
        } else {
            setTags([...tags, tag]);
        }
    };

    const handleAddNewTag = () => {
        if (newTag.trim() && !tags.includes(newTag.trim())) {
            setTags([...tags, newTag.trim()]);
            setNewTag('');
        }
    };

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={onClose}
            closeOnBackdrop={false}
            maxWidth="lg"
            zIndexClass="z-50"
            className="bg-bg-app rounded-2xl"
            title={
                <div className="flex items-center gap-4">
                    <UserAvatar
                        size={64}
                        pictureUrl={existingContact?.profile.pictureUrl || initialProfile?.pictureUrl}
                        name={`${firstName} ${lastName}`.trim() || organization || did}
                    />
                    <div className="min-w-0 flex-1">
                        <h2 className="text-xl font-extrabold text-theme-secondary">
                            {existingContact ? t('contacts.editContact') : t('contacts.addNewContact')}
                        </h2>
                        <div className="mt-0.5 text-theme-light text-sm">
                            {did ? <TechId value={did} kind="did" /> : t('contacts.enterDidBelow')}
                        </div>
                    </div>
                </div>
            }
            footer={
                <div className="flex gap-3">
                    <button
                        type="button"
                        onClick={onClose}
                        className="flex-1 py-3 px-4 rounded-xl border border-theme-subtle text-theme-light font-bold hover:bg-bg-app transition-colors"
                    >
                        {t('common.cancel')}
                    </button>
                    <button
                        type="button"
                        onClick={handleSave}
                        disabled={isSaving || isResolvingCard || !did.trim() || isExactSelf}
                        className="flex-[2] py-3 px-4 rounded-xl bg-theme-primary text-white font-bold hover:bg-theme-accent disabled:opacity-50 disabled:hover:bg-theme-primary transition-all shadow-lg shadow-theme-primary/20 active:scale-[0.98]"
                    >
                        {isSaving ? t('contacts.saving') : (existingContact ? t('contacts.updateContact') : t('contacts.saveContact'))}
                    </button>
                </div>
            }
        >
            <div className="space-y-4 py-2">
                    {(() => {
                        const didField = (
                            <div>
                                <div className="flex items-center justify-between mb-1.5">
                                    <label className="block text-xs font-bold text-theme-light uppercase tracking-widest">{t('contacts.userId')}</label>
                                    {!existingContact && (
                                        <button
                                            type="button"
                                            onClick={handlePasteFromClipboard}
                                            className="text-xs font-bold text-theme-primary hover:text-theme-accent transition-colors flex items-center gap-1"
                                        >
                                            <Clipboard size={14} />
                                            {t('contacts.pasteContactCard')}
                                        </button>
                                    )}
                                </div>
                                <input
                                    type="text"
                                    value={did}
                                    onChange={(e) => {
                                        const rawVal = e.target.value;
                                        const parsed = parseHumocoInput(rawVal);
                                        if (parsed?.kind === 'contact') {
                                            applyContactCard(rawVal);
                                            return;
                                        }
                                        if (parsed?.kind === 'card') {
                                            void applyContactCard(rawVal);
                                            return;
                                        }
                                        if (parsed?.kind === 'did') {
                                            cardResolveGenRef.current += 1;
                                            setIsResolvingCard(false);
                                            setDid(parsed.did);
                                            return;
                                        }
                                        cardResolveGenRef.current += 1;
                                        setIsResolvingCard(false);
                                        const val = rawVal.replace(/\s+/g, '');
                                        setDid(val);
                                    }}
                                    disabled={!!existingContact}
                                    data-testid="contact-did-input"
                                    placeholder={t('contacts.didPlaceholder')}
                                    className="w-full bg-white border border-theme-subtle rounded-xl px-4 py-2.5 text-theme-secondary placeholder:text-theme-placeholder focus:outline-none focus:ring-2 focus:ring-theme-primary/20 disabled:opacity-50 font-mono text-sm shadow-sm"
                                />
                                {isExactSelf && (
                                    <div
                                        data-testid="self-active-profile-warning"
                                        className="mt-2 p-3 rounded-xl bg-amber-50 border border-amber-200 text-xs font-semibold text-amber-800 flex items-start gap-2"
                                    >
                                        <AlertCircle size={15} className="shrink-0 mt-0.5 text-amber-600" />
                                        <span>{t('contacts.selfActiveProfileHint')}</span>
                                    </div>
                                )}
                            </div>
                        );
                        return existingContact ? <Advanced>{didField}</Advanced> : didField;
                    })()}

                    <div className="grid grid-cols-2 gap-4">
                        <div>
                            <label className="block text-xs font-bold text-theme-light uppercase tracking-widest mb-1.5">{t('contacts.firstName')}</label>
                            <input
                                type="text"
                                value={firstName}
                                onChange={(e) => setFirstName(e.target.value)}
                                data-testid="contact-firstname-input"
                                placeholder={t('contacts.firstNamePlaceholder')}
                                className="w-full bg-white border border-theme-subtle rounded-xl px-4 py-2.5 text-theme-secondary placeholder:text-theme-placeholder focus:outline-none focus:ring-2 focus:ring-theme-primary/20 shadow-sm"
                            />
                        </div>
                        <div>
                            <label className="block text-xs font-bold text-theme-light uppercase tracking-widest mb-1.5">{t('contacts.lastName')}</label>
                            <input
                                type="text"
                                value={lastName}
                                onChange={(e) => setLastName(e.target.value)}
                                placeholder={t('contacts.lastNamePlaceholder')}
                                className="w-full bg-white border border-theme-subtle rounded-xl px-4 py-2.5 text-theme-secondary placeholder:text-theme-placeholder focus:outline-none focus:ring-2 focus:ring-theme-primary/20 shadow-sm"
                            />
                        </div>
                    </div>

                    <div>
                        <label className="block text-xs font-bold text-theme-light uppercase tracking-widest mb-1.5">{t('contacts.organizationCommunity')}</label>
                        <input
                            type="text"
                            value={organization}
                            onChange={(e) => setOrganization(e.target.value)}
                            placeholder={t('contacts.organizationPlaceholder')}
                            className="w-full bg-white border border-theme-subtle rounded-xl px-4 py-2.5 text-theme-secondary placeholder:text-theme-placeholder focus:outline-none focus:ring-2 focus:ring-theme-primary/20 shadow-sm"
                        />
                    </div>

                    {/* Sync endpoints (Direct Sync Services) */}
                    <Advanced>
                        <div className="space-y-2 pt-1 border-t border-theme-subtle/50">
                            <div>
                                <div className="flex items-center gap-1.5">
                                    <label className="text-xs font-bold text-theme-light uppercase tracking-widest">
                                        {t('contacts.syncEndpointsLabel')}
                                    </label>
                                    <Server size={14} className="text-theme-light" />
                                </div>
                                <p className="text-[11px] text-theme-light leading-tight mt-0.5">
                                    {t('contacts.syncEndpointsHint')}
                                </p>
                            </div>

                        {/* List of configured Sync-Ablagen */}
                        {syncs.length > 0 && (
                            <div className="space-y-2">
                                {syncs.map((d, index) => {
                                    const host = (() => {
                                        try {
                                            return new URL(d.writeUrl || d.readUrl).hostname;
                                        } catch {
                                            return d.writeUrl || d.readUrl;
                                        }
                                    })();
                                    const syncKey = d.writeUrl || d.readUrl;
                                    const isRevoked = d.status === 'revoked';
                                    const isDeprecating = d.status === 'deprecating';
                                    const isPendingAuth = d.status === 'pending_auth';
                                    const isUnreachable = d.status === 'unreachable' || (!isPendingAuth && (d.failCount ?? 0) > 0);
                                    const isHardUnreachable = d.status === 'unreachable' || ((d.failCount ?? 0) >= 3);
                                    const isActive = d.status === 'active';

                                    const dotColor = isRevoked
                                        ? 'bg-rose-600'
                                        : isDeprecating
                                        ? 'bg-amber-500'
                                        : isUnreachable
                                        ? (isHardUnreachable ? 'bg-rose-500' : 'bg-amber-500')
                                        : isActive
                                        ? 'bg-emerald-500'
                                        : isPendingAuth
                                        ? 'bg-amber-500'
                                        : 'bg-slate-400';
                                    const statusLabel = isRevoked
                                        ? t('settings.syncStatusRevoked', 'Geschlossen')
                                        : isDeprecating
                                        ? (d.sunsetAt
                                            ? t('settings.syncStatusDeprecatingDate', {
                                                  date: new Date(d.sunsetAt * 1000).toLocaleDateString(),
                                                  defaultValue: `Schließt am ${new Date(d.sunsetAt * 1000).toLocaleDateString()}`,
                                              })
                                            : t('settings.syncStatusDeprecating', 'Wird bald geschlossen'))
                                        : isUnreachable
                                        ? (isHardUnreachable ? t('contacts.syncUnreachable') : t('contacts.syncWarning'))
                                        : isActive
                                        ? t('contacts.syncReachable')
                                        : isPendingAuth
                                        ? t('contacts.syncPendingAuth')
                                        : t('contacts.syncUntested');

                                    const isConfirming = confirmDeleteKey === syncKey;

                                    return (
                                        <div
                                            key={`${syncKey}-${index}`}
                                            className="flex items-center justify-between p-2.5 bg-white border border-theme-subtle rounded-xl text-xs shadow-sm"
                                        >
                                            <div className="flex items-center gap-2 min-w-0 flex-1 mr-2">
                                                <span className={`w-2 h-2 rounded-full shrink-0 ${dotColor}`} />
                                                <div className="min-w-0 flex-1">
                                                    <div className="font-mono text-xs font-semibold text-theme-secondary truncate" title={syncKey}>
                                                        {host}
                                                    </div>
                                                    <div className="text-[10px] text-theme-light truncate">
                                                        {statusLabel}
                                                        {d.lastSeenAt ? ` • ${t('contacts.syncLastSeen')} ${new Date(d.lastSeenAt * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}` : ''}
                                                    </div>
                                                </div>
                                            </div>

                                            {isConfirming ? (
                                                <div className="flex items-center gap-1.5 shrink-0">
                                                    <span className="text-[10px] font-bold text-rose-600">
                                                        {t('contacts.syncDeleteConfirm')}
                                                    </span>
                                                    <button
                                                        type="button"
                                                        onClick={() => handleDeleteSync(syncKey)}
                                                        className="px-2 py-1 bg-rose-600 text-white text-[10px] font-bold rounded-lg hover:bg-rose-700 transition-colors"
                                                    >
                                                        {t('contacts.syncDeleteConfirmBtn')}
                                                    </button>
                                                    <button
                                                        type="button"
                                                        onClick={() => setConfirmDeleteKey(null)}
                                                        className="px-2 py-1 bg-slate-100 text-slate-700 text-[10px] font-bold rounded-lg hover:bg-slate-200 transition-colors"
                                                    >
                                                        {t('contacts.syncDeleteCancelBtn')}
                                                    </button>
                                                </div>
                                            ) : (
                                                <button
                                                    type="button"
                                                    onClick={() => setConfirmDeleteKey(syncKey)}
                                                    className="p-1.5 text-theme-light hover:text-rose-600 hover:bg-rose-50 rounded-lg transition-colors shrink-0"
                                                    title={t('contacts.syncDelete')}
                                                >
                                                    <Trash2 size={14} />
                                                </button>
                                            )}
                                        </div>
                                    );
                                })}
                            </div>
                        )}

                        {/* Input & Probe button for new Sync-Ablage (if < 2) */}
                        {syncs.length < 2 ? (
                            <div className="space-y-1.5">
                                <div className="flex gap-2">
                                    <input
                                        type="url"
                                        value={syncInputUrl}
                                        onChange={(e) => {
                                            setSyncInputUrl(e.target.value);
                                            setSyncError(null);
                                        }}
                                        placeholder={t('contacts.syncInputPlaceholder')}
                                        disabled={isProbingSync}
                                        className="flex-1 bg-white border border-theme-subtle rounded-xl px-3 py-2 text-xs font-mono text-theme-secondary placeholder:text-theme-placeholder focus:outline-none focus:ring-2 focus:ring-theme-primary/20 shadow-sm disabled:opacity-50"
                                    />
                                    <button
                                        type="button"
                                        onClick={handleProbeAndAddSync}
                                        disabled={isProbingSync || !syncInputUrl.trim()}
                                        className="px-3 py-2 bg-theme-primary text-white rounded-xl text-xs font-bold hover:bg-theme-accent disabled:opacity-50 disabled:hover:bg-theme-primary transition-all shadow-sm shrink-0 flex items-center gap-1.5"
                                    >
                                        {isProbingSync ? (
                                            <>
                                                <span className="w-3 h-3 border-2 border-white/30 border-t-white rounded-full animate-spin" />
                                                <span>{t('contacts.syncProbing')}</span>
                                            </>
                                        ) : (
                                            t('contacts.syncProbeAndAdd')
                                        )}
                                    </button>
                                </div>
                                {syncError && (
                                    <p className="text-[11px] font-semibold text-rose-600">
                                        {syncError}
                                    </p>
                                )}
                            </div>
                        ) : (
                            <p className="text-[11px] text-theme-light italic">
                                {t('contacts.syncMaxReached')}
                            </p>
                        )}
                        </div>
                    </Advanced>

                    {/* Personal Trust Rating */}
                    <div className="space-y-2 pt-1 border-t border-theme-subtle/50">
                        <div className="flex items-center gap-1.5">
                            <label className="text-xs font-bold text-theme-light uppercase tracking-widest">
                                {t('contacts.trustSectionLabel')}
                            </label>
                            <HelpIcon topic="wotNetwork" size={14} />
                        </div>
                        <p className="text-[11px] text-theme-light leading-tight">
                            {t('contacts.trustNetworkHint')}
                        </p>
                        <div className="pt-1 space-y-2">
                            {siblingContact && (
                                <div
                                    data-testid="sibling-prefix-trust-notice"
                                    className="p-3 rounded-xl bg-sky-50 border border-sky-200 text-xs font-medium text-sky-900 flex items-start gap-2"
                                >
                                    <AlertCircle size={15} className="shrink-0 mt-0.5 text-sky-600" />
                                    <span>
                                        {t('contacts.siblingPrefixTrustHint', {
                                            name:
                                                [
                                                    siblingContact.profile.firstName,
                                                    siblingContact.profile.lastName,
                                                ]
                                                    .filter(Boolean)
                                                    .join(' ') ||
                                                siblingContact.profile.organization ||
                                                truncateUserId(siblingContact.did),
                                        })}
                                    </span>
                                </div>
                            )}
                            {isOwnSubaccount ? (
                                <div
                                    data-testid="own-subaccount-trust-notice"
                                    className="p-3 rounded-xl bg-sky-50 border border-sky-200 text-xs font-medium text-sky-900 flex items-start gap-2"
                                >
                                    <AlertCircle size={15} className="shrink-0 mt-0.5 text-sky-600" />
                                    <span>{t('contacts.ownSubaccountTrustHint')}</span>
                                </div>
                            ) : (
                                <TrustLevelRadioCards
                                    value={displayedTrustLevel}
                                    onChange={(lvl) => {
                                        setHasManualTrustOverride(true);
                                        setTrustLevel(lvl);
                                    }}
                                    allowUnrated={true}
                                />
                            )}
                        </div>

                        {/* Permission Change Preview */}
                        {trustImpact && (
                            (trustImpact.canReadMyMarketBefore !== trustImpact.canReadMyMarketAfter ||
                             trustImpact.canWriteMySyncBefore !== trustImpact.canWriteMySyncAfter ||
                             trustImpact.canReadMyWotBefore !== trustImpact.canReadMyWotAfter ||
                             trustImpact.canSendMePostBefore !== trustImpact.canSendMePostAfter) && (
                                <div className="mt-2 space-y-1.5 p-2 bg-bg-app rounded-xl border border-theme-subtle/70">
                                    {/* Reading permission change */}
                                    {trustImpact.canReadMyMarketBefore !== trustImpact.canReadMyMarketAfter && (
                                        trustImpact.canReadMyMarketAfter ? (
                                            <div data-testid="trust-preview-gain-read" className="flex items-center gap-1.5 text-[11px] font-semibold text-emerald-700 bg-emerald-50 px-2.5 py-1 rounded-lg border border-emerald-200">
                                                <CheckCircle2 size={13} className="shrink-0" />
                                                <span>{t('contacts.trustPreviewGainRead')}</span>
                                            </div>
                                        ) : (
                                            <div data-testid="trust-preview-lose-read" className="flex items-center gap-1.5 text-[11px] font-semibold text-amber-700 bg-amber-50 px-2.5 py-1 rounded-lg border border-amber-200">
                                                <AlertCircle size={13} className="shrink-0" />
                                                <span>{t('contacts.trustPreviewLoseRead')}</span>
                                            </div>
                                        )
                                    )}

                                    {/* Writing permission change */}
                                    {trustImpact.canWriteMySyncBefore !== trustImpact.canWriteMySyncAfter && (
                                        trustImpact.canWriteMySyncAfter ? (
                                            <div data-testid="trust-preview-gain-write" className="flex items-center gap-1.5 text-[11px] font-semibold text-emerald-700 bg-emerald-50 px-2.5 py-1 rounded-lg border border-emerald-200">
                                                <CheckCircle2 size={13} className="shrink-0" />
                                                <span>{t('contacts.trustPreviewGainWrite')}</span>
                                            </div>
                                        ) : (
                                            <div data-testid="trust-preview-lose-write" className="flex items-center gap-1.5 text-[11px] font-semibold text-rose-700 bg-rose-50 px-2.5 py-1 rounded-lg border border-rose-200">
                                                <AlertCircle size={13} className="shrink-0" />
                                                <span>{t('contacts.trustPreviewLoseWrite')}</span>
                                            </div>
                                        )
                                    )}

                                    {/* WoT reading permission change */}
                                    {trustImpact.canReadMyWotBefore !== trustImpact.canReadMyWotAfter && (
                                        trustImpact.canReadMyWotAfter ? (
                                            <div data-testid="trust-preview-gain-wot" className="flex items-center gap-1.5 text-[11px] font-semibold text-emerald-700 bg-emerald-50 px-2.5 py-1 rounded-lg border border-emerald-200">
                                                <CheckCircle2 size={13} className="shrink-0" />
                                                <span>{t('contacts.trustPreviewGainWot')}</span>
                                            </div>
                                        ) : (
                                            <div data-testid="trust-preview-lose-wot" className="flex items-center gap-1.5 text-[11px] font-semibold text-amber-700 bg-amber-50 px-2.5 py-1 rounded-lg border border-amber-200">
                                                <AlertCircle size={13} className="shrink-0" />
                                                <span>{t('contacts.trustPreviewLoseWot')}</span>
                                            </div>
                                        )
                                    )}

                                    {/* Post sending permission change */}
                                    {trustImpact.canSendMePostBefore !== trustImpact.canSendMePostAfter && (
                                        trustImpact.canSendMePostAfter ? (
                                            <div data-testid="trust-preview-gain-post" className="flex items-center gap-1.5 text-[11px] font-semibold text-emerald-700 bg-emerald-50 px-2.5 py-1 rounded-lg border border-emerald-200">
                                                <CheckCircle2 size={13} className="shrink-0" />
                                                <span>{t('contacts.trustPreviewGainPost')}</span>
                                            </div>
                                        ) : (
                                            <div data-testid="trust-preview-lose-post" className="flex items-center gap-1.5 text-[11px] font-semibold text-amber-700 bg-amber-50 px-2.5 py-1 rounded-lg border border-amber-200">
                                                <AlertCircle size={13} className="shrink-0" />
                                                <span>{t('contacts.trustPreviewLosePost')}</span>
                                            </div>
                                        )
                                    )}
                                </div>
                            )
                        )}
                    </div>


                    <div>
                        <label className="block text-xs font-bold text-theme-light uppercase tracking-widest mb-1.5">{t('contacts.tags')}</label>
                        <div className="flex flex-wrap gap-2 mb-3">
                            {availableTags.map(tag => (
                                <button
                                    key={tag}
                                    onClick={() => toggleTag(tag)}
                                    className={`px-3 py-1 rounded-full text-xs font-bold border transition-all ${
                                        tags.includes(tag)
                                            ? 'bg-theme-secondary border-theme-secondary text-white shadow-md'
                                            : 'bg-white border-theme-subtle text-theme-light hover:border-theme-light-border'
                                    }`}
                                >
                                    {tag}
                                </button>
                            ))}
                        </div>
                        <div className="flex gap-2">
                            <input
                                type="text"
                                value={newTag}
                                onChange={(e) => setNewTag(e.target.value)}
                                placeholder={t('contacts.addCustomTagPlaceholder')}
                                className="flex-1 bg-white border border-theme-subtle rounded-lg px-3 py-1.5 text-xs text-theme-secondary placeholder:text-theme-placeholder focus:outline-none focus:ring-1 focus:ring-theme-primary/30 shadow-sm"
                            />
                            <button
                                type="button"
                                onClick={handleAddNewTag}
                                className="bg-theme-light hover:bg-theme-secondary-accent text-white p-1.5 rounded-lg transition-colors shadow-sm"
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
                                </svg>
                            </button>
                        </div>
                    </div>

                    <div>
                        <label className="block text-xs font-bold text-theme-light uppercase tracking-widest mb-1.5">{t('contacts.notes')}</label>
                        <textarea
                            value={notes}
                            onChange={(e) => setNotes(e.target.value)}
                            placeholder={t('contacts.notesPlaceholder')}
                            rows={3}
                            className="w-full bg-white border border-theme-subtle rounded-xl px-4 py-2.5 text-theme-secondary placeholder:text-theme-placeholder focus:outline-none focus:ring-2 focus:ring-theme-primary/20 resize-none shadow-sm"
                        />
                    </div>

                    {error && (
                        <div className="p-4 bg-rose-50 border border-rose-200 rounded-xl flex items-start gap-3 animate-in slide-in-from-top-1">
                            <div className="bg-rose-500 rounded-full p-1 text-white shrink-0 mt-0.5">
                                <svg xmlns="http://www.w3.org/2000/svg" className="h-3 w-3" viewBox="0 0 20 20" fill="currentColor">
                                    <path fillRule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7 4a1 1 0 11-2 0 1 1 0 012 0zm-1-9a1 1 0 00-1 1v4a1 1 0 102 0V6a1 1 0 00-1-1z" clipRule="evenodd" />
                                </svg>
                            </div>
                            <p className="text-xs font-bold text-rose-600 leading-tight">{error}</p>
                        </div>
                    )}
            </div>
        </ModalShell>
    );
};

export default ContactDialog;
