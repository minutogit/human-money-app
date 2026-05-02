import React, { useState, useEffect } from 'react';
import { Contact, PublicProfile } from '../types';
import Avatar from 'boring-avatars';

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
    const [did, setDid] = useState(initialDid || '');
    const [firstName, setFirstName] = useState('');
    const [lastName, setLastName] = useState('');
    const [organization, setOrganization] = useState('');
    const [tags, setTags] = useState<string[]>([]);
    const [notes, setNotes] = useState('');
    const [newTag, setNewTag] = useState('');
    const [isSaving, setIsSaving] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const predefinedTags = ["Friends", "Business", "Favorites", "Trusted"];
    const availableTags = Array.from(new Set([...predefinedTags, ...tags]));

    useEffect(() => {
        if (existingContact) {
            setDid(existingContact.did);
            setFirstName(existingContact.profile.first_name || '');
            setLastName(existingContact.profile.last_name || '');
            setOrganization(existingContact.profile.organization || '');
            setTags(existingContact.tags);
            setNotes(existingContact.notes || '');
        } else if (initialProfile) {
            setFirstName(initialProfile.first_name || '');
            setLastName(initialProfile.last_name || '');
            setOrganization(initialProfile.organization || '');
            if (initialDid) setDid(initialDid);
        } else {
            // Reset for new contact
            setDid(initialDid || '');
            setFirstName('');
            setLastName('');
            setOrganization('');
            setTags([]);
            setNotes('');
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
                    first_name: firstName.trim() || undefined,
                    last_name: lastName.trim() || undefined,
                    organization: organization.trim() || undefined,
                },
                tags,
                notes: notes.trim() || undefined,
                added_at: existingContact?.added_at || new Date().toISOString(),
            };
            await onSave(contact);
            onClose();
        } catch (e: any) {
            console.error("Failed to save contact:", e);
            setError(e.message || String(e));
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
                            {existingContact ? 'Edit Contact' : 'Add New Contact'}
                        </h2>
                        <p className="text-theme-light text-sm font-mono truncate max-w-[280px]">
                            {did || 'Enter DID below'}
                        </p>
                    </div>
                </div>

                <div className="p-6 space-y-4 max-h-[60vh] overflow-y-auto custom-scrollbar">
                    <div>
                        <label className="block text-xs font-bold text-theme-light uppercase tracking-widest mb-1.5">User ID (DID)</label>
                        <input
                            type="text"
                            value={did}
                            onChange={(e) => setDid(e.target.value)}
                            disabled={!!existingContact}
                            placeholder="did:key:z..."
                            className="w-full bg-white border border-theme-subtle rounded-xl px-4 py-2.5 text-theme-secondary placeholder:text-theme-placeholder focus:outline-none focus:ring-2 focus:ring-theme-primary/20 disabled:opacity-50 font-mono text-sm shadow-sm"
                        />
                    </div>

                    <div className="grid grid-cols-2 gap-4">
                        <div>
                            <label className="block text-xs font-bold text-theme-light uppercase tracking-widest mb-1.5">First Name</label>
                            <input
                                type="text"
                                value={firstName}
                                onChange={(e) => setFirstName(e.target.value)}
                                placeholder="e.g. Alice"
                                className="w-full bg-white border border-theme-subtle rounded-xl px-4 py-2.5 text-theme-secondary placeholder:text-theme-placeholder focus:outline-none focus:ring-2 focus:ring-theme-primary/20 shadow-sm"
                            />
                        </div>
                        <div>
                            <label className="block text-xs font-bold text-theme-light uppercase tracking-widest mb-1.5">Last Name</label>
                            <input
                                type="text"
                                value={lastName}
                                onChange={(e) => setLastName(e.target.value)}
                                placeholder="e.g. Smith"
                                className="w-full bg-white border border-theme-subtle rounded-xl px-4 py-2.5 text-theme-secondary placeholder:text-theme-placeholder focus:outline-none focus:ring-2 focus:ring-theme-primary/20 shadow-sm"
                            />
                        </div>
                    </div>

                    <div>
                        <label className="block text-xs font-bold text-theme-light uppercase tracking-widest mb-1.5">Organization / Community</label>
                        <input
                            type="text"
                            value={organization}
                            onChange={(e) => setOrganization(e.target.value)}
                            placeholder="e.g. Green Valley Inc."
                            className="w-full bg-white border border-theme-subtle rounded-xl px-4 py-2.5 text-theme-secondary placeholder:text-theme-placeholder focus:outline-none focus:ring-2 focus:ring-theme-primary/20 shadow-sm"
                        />
                    </div>

                    <div>
                        <label className="block text-xs font-bold text-theme-light uppercase tracking-widest mb-1.5">Tags</label>
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
                                placeholder="Add custom tag..."
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
                        <label className="block text-xs font-bold text-theme-light uppercase tracking-widest mb-1.5">Notes</label>
                        <textarea
                            value={notes}
                            onChange={(e) => setNotes(e.target.value)}
                            placeholder="Internal notes about this contact..."
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
                        Cancel
                    </button>
                    <button
                        onClick={handleSave}
                        disabled={isSaving || !did.trim()}
                        className="flex-[2] py-3 px-4 rounded-xl bg-theme-primary text-white font-bold hover:bg-theme-accent disabled:opacity-50 disabled:hover:bg-theme-primary transition-all shadow-lg shadow-theme-primary/20 active:scale-[0.98]"
                    >
                        {isSaving ? 'Saving...' : (existingContact ? 'Update Contact' : 'Save Contact')}
                    </button>
                </div>
            </div>
        </div>
    );
};

export default ContactDialog;
