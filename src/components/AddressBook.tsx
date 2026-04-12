import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Contact } from '../types';
import Avatar from 'boring-avatars';
import ContactDialog from './ContactDialog';
import { logger } from '../utils/log';
import { ConfirmationModal } from './ui/ConfirmationModal';

interface AddressBookProps {
    onBack: () => void;
}

const AddressBook: React.FC<AddressBookProps> = ({ onBack }) => {
    const [contacts, setContacts] = useState<Contact[]>([]);
    const [searchQuery, setSearchQuery] = useState('');
    const [selectedTag, setSelectedTag] = useState<string | null>(null);
    const [isDialogOpen, setIsDialogOpen] = useState(false);
    const [editingContact, setEditingContact] = useState<Contact | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [deleteRequest, setDeleteRequest] = useState<string | null>(null);

    useEffect(() => {
        loadContacts();
    }, []);

    const loadContacts = async () => {
        setIsLoading(true);
        try {
            const result: Contact[] = await invoke('get_contacts');
            setContacts(result);
        } catch (err) {
            logger.error("Failed to load contacts: " + err);
        } finally {
            setIsLoading(false);
        }
    };

    const handleSaveContact = async (contact: Contact) => {
        try {
            await invoke('save_contact', { contact });
            await loadContacts();
            logger.info("Contact saved successfully");
        } catch (err) {
            logger.error("Failed to save contact: " + err);
            throw err;
        }
    };

    const handleDeleteContact = async () => {
        if (!deleteRequest) return;
        
        try {
            await invoke('delete_contact', { did: deleteRequest });
            await loadContacts();
            logger.info("Contact deleted");
        } catch (err) {
            logger.error("Failed to delete contact: " + err);
        } finally {
            setDeleteRequest(null);
        }
    };

    const allTags = Array.from(new Set(contacts.flatMap(c => c.tags)));

    const filteredContacts = contacts.filter(contact => {
        const matchesSearch = 
            (contact.profile.first_name || '').toLowerCase().includes(searchQuery.toLowerCase()) ||
            (contact.profile.last_name || '').toLowerCase().includes(searchQuery.toLowerCase()) ||
            (contact.profile.organization || '').toLowerCase().includes(searchQuery.toLowerCase()) ||
            contact.did.toLowerCase().includes(searchQuery.toLowerCase());
        
        const matchesTag = !selectedTag || contact.tags.includes(selectedTag);
        
        return matchesSearch && matchesTag;
    });

    const truncateDid = (did: string) => {
        if (did.length <= 16) return did;
        return `${did.substring(0, 10)}...${did.substring(did.length - 6)}`;
    };

    return (
        <div className="flex flex-col h-full bg-bg-app text-theme-secondary">
            {/* Header with Back Button */}
            <div className="flex items-center gap-4 mb-8">
                <button
                    onClick={onBack}
                    className="p-2.5 rounded-full bg-white border border-theme-subtle hover:bg-bg-input-readonly transition-all text-theme-light hover:text-theme-primary shadow-sm active:scale-95"
                    title="Back to Dashboard"
                >
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 19l-7-7m0 0l7-7m-7 7h18" />
                    </svg>
                </button>
                <div>
                    <h1 className="text-3xl font-extrabold text-theme-secondary tracking-tight">Address Book</h1>
                    <p className="text-theme-light mt-0.5">Manage your trusted contacts and local DIDs.</p>
                </div>
                <div className="ml-auto">
                    <button
                        onClick={() => {
                            setEditingContact(null);
                            setIsDialogOpen(true);
                        }}
                        className="bg-theme-primary hover:bg-theme-accent text-white px-6 py-2.5 rounded-xl font-bold transition-all shadow-lg shadow-theme-primary/20 active:scale-95 flex items-center gap-2"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 6v6m0 0v6m0-6h6m-6 0H6" />
                        </svg>
                        Add Contact
                    </button>
                </div>
            </div>

            {/* Search and Filters */}
            <div className="flex flex-col md:flex-row gap-4 mb-8">
                <div className="relative flex-1">
                    <span className="absolute left-4 top-1/2 -translate-y-1/2 text-theme-light">
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                        </svg>
                    </span>
                    <input
                        type="text"
                        placeholder="Search name, organization or DID..."
                        value={searchQuery}
                        onChange={(e) => setSearchQuery(e.target.value)}
                        className="w-full bg-white border border-theme-subtle rounded-2xl pl-12 pr-4 py-3 text-theme-secondary placeholder:text-theme-placeholder focus:outline-none focus:ring-2 focus:ring-theme-primary/20 transition-all font-medium shadow-sm"
                    />
                </div>
                <div className="flex gap-2 overflow-x-auto pb-2 no-scrollbar">
                    <button
                        onClick={() => setSelectedTag(null)}
                        className={`px-4 py-2 rounded-xl text-sm font-semibold transition-all whitespace-nowrap border ${
                            !selectedTag 
                                ? 'bg-theme-secondary border-theme-secondary text-white shadow-md' 
                                : 'bg-white border-theme-subtle text-theme-light hover:border-theme-light-border'
                        }`}
                    >
                        All
                    </button>
                    {allTags.map(tag => (
                        <button
                            key={tag}
                            onClick={() => setSelectedTag(tag)}
                            className={`px-4 py-2 rounded-xl text-sm font-semibold transition-all whitespace-nowrap border ${
                                selectedTag === tag
                                    ? 'bg-theme-secondary border-theme-secondary text-white shadow-md'
                                    : 'bg-white border-theme-subtle text-theme-light hover:border-theme-light-border'
                            }`}
                        >
                            {tag}
                        </button>
                    ))}
                </div>
            </div>

            {/* Contact List */}
            {isLoading ? (
                <div className="flex-1 flex items-center justify-center">
                    <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-theme-primary"></div>
                </div>
            ) : filteredContacts.length === 0 ? (
                <div className="flex-1 flex flex-col items-center justify-center text-center p-12 bg-white rounded-3xl border-2 border-dashed border-theme-subtle shadow-sm">
                    <div className="text-theme-subtle mb-4">
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-16 w-16 mx-auto" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4.354a4 4 0 110 5.292M15 21H3v-1a6 6 0 0112 0v1zm0 0h6v-1a6 6 0 00-9-5.197M13 7a4 4 0 11-8 0 4 4 0 018 0z" />
                        </svg>
                    </div>
                    <h3 className="text-xl font-bold text-theme-secondary">No contacts found</h3>
                    <p className="text-theme-light mt-2">Try adjusting your search or add a new contact.</p>
                </div>
            ) : (
                <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
                    {filteredContacts.map(contact => (
                        <div 
                            key={contact.did}
                            className="bg-white hover:bg-bg-input-readonly border border-theme-subtle rounded-2xl p-4 flex items-center gap-4 group transition-all shadow-sm hover:shadow-md"
                        >
                            <div className="w-14 h-14 rounded-full overflow-hidden border-2 border-theme-subtle group-hover:border-theme-primary transition-colors bg-bg-app">
                                <Avatar
                                    size={56}
                                    name={contact.did}
                                    variant="beam"
                                    colors={["#E63946", "#2B1B17", "#E76F51", "#F4A261", "#2A9D8F"]}
                                />
                            </div>
                            <div className="flex-1 min-w-0">
                                <h3 className="text-lg font-bold text-theme-secondary truncate">
                                    {contact.profile.first_name || contact.profile.last_name 
                                        ? `${contact.profile.first_name || ''} ${contact.profile.last_name || ''}`.trim()
                                        : contact.profile.organization || 'Anonymous Contact'}
                                </h3>
                                <p className="text-theme-light text-sm font-mono truncate">{truncateDid(contact.did)}</p>
                                <div className="flex flex-wrap gap-1.5 mt-2">
                                    {contact.tags.map(tag => (
                                        <span key={tag} className="px-2 py-0.5 bg-bg-app border border-theme-subtle rounded-md text-[10px] uppercase tracking-wider font-bold text-theme-light">
                                            {tag}
                                        </span>
                                    ))}
                                </div>
                            </div>
                            <div className="flex gap-1">
                                <button 
                                    onClick={() => {
                                        setEditingContact(contact);
                                        setIsDialogOpen(true);
                                    }}
                                    className="p-2 hover:bg-bg-app rounded-lg text-theme-light hover:text-theme-primary transition-colors"
                                    title="Edit Contact"
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" />
                                    </svg>
                                </button>
                                <button 
                                    onClick={() => setDeleteRequest(contact.did)}
                                    className="p-2 hover:bg-red-50 rounded-lg text-theme-light hover:text-red-500 transition-colors"
                                    title="Delete Contact"
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-4v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                                    </svg>
                                </button>
                            </div>
                        </div>
                    ))}
                </div>
            )}

            <ContactDialog
                isOpen={isDialogOpen}
                onClose={() => setIsDialogOpen(false)}
                onSave={handleSaveContact}
                existingContact={editingContact}
            />

            <ConfirmationModal
                isOpen={!!deleteRequest}
                title="Delete Contact"
                description="Are you sure you want to remove this contact from your address book? This action cannot be undone."
                confirmText="Delete"
                onConfirm={handleDeleteContact}
                onCancel={() => setDeleteRequest(null)}
            />
        </div>
    );
};

export default AddressBook;
