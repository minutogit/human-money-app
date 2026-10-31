import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Contact, PublicProfile } from '../types';
import Avatar from 'boring-avatars';
import { translateError } from '../utils/errorHelper';
import { HelpIcon } from './ui/HelpIcon';

interface ContactDialogProps {
    isOpen: boolean;
    onClose: () => void;
    onSave: (contact: Contact) => Promise<void>;
    existingContact?: Contact | null;
    initialProfile?: Partial<PublicProfile> | null;
    initialDid?: string;
}

const ContactDialog: React.FC<ContactDialogProps> = ({
    isOpen,
    onClose,
    onSave,
    existingContact,
    initialProfile,
    initialDid
}) => {
    const { t } = useTranslation();
    const [did, setDid] = useState(initialDid || '');
    const [firstName, setFirstName] = useState('');
    const [lastName, setLastName] = useState('');
    const [organization, setOrganization] = useState('');
    const [tags, setTags] = useState<string[]>([]);
    const [notes, setNotes] = useState('');
    const [newTag, setNewTag] = useState('');
    const [trustLevel, setTrustLevel] = useState<number | null>(existingContact?.trustLevel ?? null);
    const [desiredHops, setDesiredHops] = useState<number>(existingContact?.desiredHops ?? 3);
    const [isSaving, setIsSaving] = useState(false);
    const [error, setError] = useState<string | null>(null);

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
            setDesiredHops(existingContact.desiredHops ?? 3);
        } else if (initialProfile) {
            setFirstName(initialProfile.firstName || '');
            setLastName(initialProfile.lastName || '');
            setOrganization(initialProfile.organization || '');
            if (initialDid) setDid(initialDid);
            setTags([]);
            setNotes('');
            setTrustLevel(null);
            setDesiredHops(3);
        } else {
            // Reset for new contact
            setDid(initialDid || '');
            setFirstName('');
            setLastName('');
            setOrganization('');
            setTags([]);
            setNotes('');
            setTrustLevel(null);
            setDesiredHops(3);
        }
    }, [existingContact, initialProfile, initialDid, isOpen]);

    if (!isOpen) return null;

    const handleSave = async () => {
        if (!did.trim()) return;
        
        setIsSaving(true);
        setError(null);
        try {
            const contact: Contact = {
                did: did.trim(),
                profile: {
                    id: did.trim(),
                    firstName: firstName.trim() || undefined,
                    lastName: lastName.trim() || undefined,
                    organization: organization.trim() || undefined,
                },
                tags,
                notes: notes.trim() || undefined,
                addedAt: existingContact?.addedAt || new Date().toISOString(),
                trustLevel: trustLevel,
                desiredHops: trustLevel === null ? null : (trustLevel === 0 ? 0 : desiredHops),
                hasPeerList: existingContact?.hasPeerList,
                peerListUpdatedAt: existingContact?.peerListUpdatedAt,
            };
            await onSave(contact);
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
        <div className="fixed inset-0 bg-black/40 backdrop-blur-[2px] z-50 flex items-center justify-center p-4">
            <div className="bg-bg-app border border-theme-subtle rounded-2xl w-full max-w-lg shadow-2xl overflow-hidden animate-in fade-in zoom-in duration-200">
                <div className="p-6 border-b border-theme-subtle flex items-center gap-4 bg-white">
                    <div className="w-16 h-16 rounded-full overflow-hidden border-2 border-theme-subtle bg-bg-app">
                        <Avatar
                            size={64}
                            name={did || "default"}
                            variant="beam"
                            colors={["#E63946", "#2B1B17", "#E76F51", "#F4A261", "#2A9D8F"]}
                        />
                    </div>
                    <div>
                        <h2 className="text-xl font-extrabold text-theme-secondary">
                            {existingContact ? t('contacts.editContact') : t('contacts.addNewContact')}
                        </h2>
                        <p className="text-theme-light text-sm font-mono truncate max-w-[280px]">
                            {did || t('contacts.enterDidBelow')}
                        </p>
                    </div>
                </div>

                <div className="p-6 space-y-4 max-h-[60vh] overflow-y-auto custom-scrollbar">
                    <div>
                        <label className="block text-xs font-bold text-theme-light uppercase tracking-widest mb-1.5">{t('contacts.userId')}</label>
                        <input
                            type="text"
                            value={did}
                            onChange={(e) => {
                                const val = e.target.value.replace(/\s+/g, '');
                                e.target.value = val;
                                setDid(val);
                            }}
                            disabled={!!existingContact}
                            placeholder={t('contacts.didPlaceholder')}
                            className="w-full bg-white border border-theme-subtle rounded-xl px-4 py-2.5 text-theme-secondary placeholder:text-theme-placeholder focus:outline-none focus:ring-2 focus:ring-theme-primary/20 disabled:opacity-50 font-mono text-sm shadow-sm"
                        />
                    </div>

                    <div className="grid grid-cols-2 gap-4">
                        <div>
                            <label className="block text-xs font-bold text-theme-light uppercase tracking-widest mb-1.5">{t('contacts.firstName')}</label>
                            <input
                                type="text"
                                value={firstName}
                                onChange={(e) => setFirstName(e.target.value)}
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
                        <div className="space-y-1.5 pt-1">
                            {[
                                {
                                    level: null,
                                    labelKey: 'contacts.trustLevelUnrated',
                                    descKey: 'contacts.trustLevelUnratedDesc',
                                    colorClass: 'border-slate-200 hover:border-slate-300',
                                    activeClass: 'border-slate-400 bg-slate-50 text-slate-800 ring-2 ring-slate-400/20 shadow-sm'
                                },
                                {
                                    level: 0,
                                    labelKey: 'contacts.trustLevel0',
                                    descKey: 'contacts.trustLevel0Desc',
                                    colorClass: 'border-sky-200 hover:border-sky-300',
                                    activeClass: 'border-sky-500 bg-sky-50/70 text-sky-900 ring-2 ring-sky-400/20 shadow-sm'
                                },
                                {
                                    level: 1,
                                    labelKey: 'contacts.trustLevel1',
                                    descKey: 'contacts.trustLevel1Desc',
                                    colorClass: 'border-amber-200 hover:border-amber-300',
                                    activeClass: 'border-amber-500 bg-amber-50/70 text-amber-900 ring-2 ring-amber-400/20 shadow-sm'
                                },
                                {
                                    level: 2,
                                    labelKey: 'contacts.trustLevel2',
                                    descKey: 'contacts.trustLevel2Desc',
                                    colorClass: 'border-teal-200 hover:border-teal-300',
                                    activeClass: 'border-teal-500 bg-teal-50/70 text-teal-900 ring-2 ring-teal-400/20 shadow-sm'
                                },
                                {
                                    level: 3,
                                    labelKey: 'contacts.trustLevel3',
                                    descKey: 'contacts.trustLevel3Desc',
                                    colorClass: 'border-emerald-300 hover:border-emerald-400',
                                    activeClass: 'border-emerald-600 bg-emerald-50 text-emerald-950 ring-2 ring-emerald-500/20 shadow-sm'
                                }
                            ].map((opt) => {
                                const isSelected = trustLevel === opt.level;
                                return (
                                    <button
                                        key={opt.level ?? 'unrated'}
                                        type="button"
                                        onClick={() => setTrustLevel(opt.level)}
                                        className={`w-full text-left p-2.5 rounded-xl border transition-all text-xs flex flex-col gap-0.5 ${
                                            isSelected ? opt.activeClass : `bg-white ${opt.colorClass} text-theme-secondary`
                                        }`}
                                    >
                                        <span className="font-bold flex items-center justify-between">
                                            <span>{t(opt.labelKey)}</span>
                                            {isSelected && <span className="text-[10px] font-black uppercase tracking-wider opacity-75">✓</span>}
                                        </span>
                                        <span className="text-[11px] opacity-80 font-normal">
                                            {t(opt.descKey)}
                                        </span>
                                    </button>
                                );
                            })}
                        </div>

                        {/* Recommendation Reach Selector */}
                        {trustLevel !== null && trustLevel >= 1 && (
                            <div className="space-y-1.5 pt-2 animate-in fade-in duration-200">
                                <label className="block text-xs font-bold text-theme-light uppercase tracking-widest">
                                    {t('contacts.reachabilityLabel')}
                                </label>
                                <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
                                    {[
                                        { hops: 0, labelKey: 'contacts.reachPrivate' },
                                        { hops: 1, labelKey: 'contacts.reachDirect' },
                                        { hops: 3, labelKey: 'contacts.reachStandard' },
                                        { hops: 6, labelKey: 'contacts.reachWide' }
                                    ].map(({ hops, labelKey }) => {
                                        const isSelected = desiredHops === hops;
                                        return (
                                            <button
                                                key={hops}
                                                type="button"
                                                onClick={() => setDesiredHops(hops)}
                                                className={`py-2 px-2.5 rounded-xl text-xs font-bold border text-center transition-all ${
                                                    isSelected
                                                        ? 'bg-theme-secondary border-theme-secondary text-white shadow-sm'
                                                        : 'bg-white border-theme-subtle text-theme-light hover:border-theme-primary/30'
                                                }`}
                                            >
                                                {t(labelKey)}
                                            </button>
                                        );
                                    })}
                                </div>
                            </div>
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

                <div className="p-6 bg-white border-t border-theme-subtle flex gap-3">
                    <button
                        onClick={onClose}
                        className="flex-1 py-3 px-4 rounded-xl border border-theme-subtle text-theme-light font-bold hover:bg-bg-app transition-colors"
                    >
                        {t('common.cancel')}
                    </button>
                    <button
                        onClick={handleSave}
                        disabled={isSaving || !did.trim()}
                        className="flex-[2] py-3 px-4 rounded-xl bg-theme-primary text-white font-bold hover:bg-theme-accent disabled:opacity-50 disabled:hover:bg-theme-primary transition-all shadow-lg shadow-theme-primary/20 active:scale-[0.98]"
                    >
                        {isSaving ? t('contacts.saving') : (existingContact ? t('contacts.updateContact') : t('contacts.saveContact'))}
                    </button>
                </div>
            </div>
        </div>
    );
};

export default ContactDialog;
