// src/components/AddressBook.tsx
import React, { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { PageLayout } from './ui/PageLayout';
import { Button } from './ui/Button';
import { contactService } from '../services/contactService';
import { Contact } from '../types';
import Avatar from 'boring-avatars';
import ContactDialog from './ContactDialog';
import { logger } from '../utils/log';
import { stringifyError } from '../utils/errorHelper';
import { ConfirmationModal } from './ui/ConfirmationModal';
import { useSession } from '../context/SessionContext';
import { 
    UserPlus, 
    Search, 
    Trash2, 
    Edit3, 
    Tag, 
    Shield, 
    Building2, 
    Users,
    X
} from 'lucide-react';

interface AddressBookProps {
    onBack: () => void;
    initialSearchQuery?: string;
}

const AddressBook: React.FC<AddressBookProps> = ({ onBack, initialSearchQuery }) => {
    const { t } = useTranslation();
    const { protectAction } = useSession();
    const [contacts, setContacts] = useState<Contact[]>([]);
    const [searchQuery, setSearchQuery] = useState(initialSearchQuery || '');
    const [selectedTag, setSelectedTag] = useState<string | null>(null);
    const [isDialogOpen, setIsDialogOpen] = useState(false);
    const [editingContact, setEditingContact] = useState<Contact | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [deleteRequest, setDeleteRequest] = useState<string | null>(null);

    const loadContacts = useCallback(async () => {
        setIsLoading(true);
        try {
            const result: Contact[] = await contactService.getContacts();
            setContacts(result);
        } catch (err) {
            logger.error("Failed to load contacts: " + stringifyError(err));
        } finally {
            setIsLoading(false);
        }
    }, []);

    useEffect(() => {
        loadContacts();
    }, [loadContacts]);

    useEffect(() => {
        if (initialSearchQuery) {
            setSearchQuery(initialSearchQuery);
        }
    }, [initialSearchQuery]);


    const handleSaveContact = async (contact: Contact) => {
        try {
            await protectAction(async (pwd) => {
                await contactService.saveContact(contact, pwd || undefined);
            });
            await loadContacts();
            logger.info("Contact saved successfully");
        } catch (err) {
            logger.error("Failed to save contact: " + stringifyError(err));
            throw err;
        }
    };

    const handleDeleteContact = async () => {
        if (!deleteRequest) return;
        
        try {
            await protectAction(async (pwd) => {
                await contactService.deleteContact(deleteRequest, pwd || undefined);
            });
            await loadContacts();
            logger.info("Contact deleted");
        } catch (err) {
            logger.error("Failed to delete contact: " + stringifyError(err));
        } finally {
            setDeleteRequest(null);
        }
    };

    const allTags = Array.from(new Set(contacts.flatMap(c => c.tags)));

    const filteredContacts = contacts.filter(contact => {
        const matchesSearch = 
            (contact.profile.firstName || '').toLowerCase().includes(searchQuery.toLowerCase()) ||
            (contact.profile.lastName || '').toLowerCase().includes(searchQuery.toLowerCase()) ||
            (contact.profile.organization || '').toLowerCase().includes(searchQuery.toLowerCase()) ||
            contact.did.toLowerCase().includes(searchQuery.toLowerCase());
        
        const matchesTag = !selectedTag || contact.tags.includes(selectedTag);
        
        return matchesSearch && matchesTag;
    });

    const truncateDid = (did: string) => {
        if (did.length <= 20) return did;
        return `${did.substring(0, 12)}...${did.substring(did.length - 8)}`;
    };

    return (
        <PageLayout 
            title={t('contacts.title')} 
            description={t('contacts.description')} 
            onBack={onBack}
            actions={
                <Button
                    onClick={() => {
                        setEditingContact(null);
                        setIsDialogOpen(true);
                    }}
                    className="gap-2"
                >
                    <UserPlus size={18} />
                    {t('contacts.newContact')}
                </Button>
            }
        >
            <div className="max-w-6xl mx-auto space-y-8">
                {/* Search and Filters */}
                <div className="flex flex-col md:flex-row gap-6">
                    <div className="relative flex-1 group">
                        <div className="absolute left-4 top-1/2 -translate-y-1/2 text-theme-light group-focus-within:text-theme-primary transition-colors">
                            <Search size={18} />
                        </div>
                        <input
                            type="text"
                            placeholder={t('contacts.searchPlaceholder')}
                            value={searchQuery}
                            onChange={(e) => setSearchQuery(e.target.value)}
                            className="w-full bg-white/50 backdrop-blur-sm border border-theme-subtle rounded-2xl pl-12 pr-10 py-4 text-theme-secondary placeholder:text-theme-placeholder focus:outline-none focus:ring-2 focus:ring-theme-primary/10 transition-all font-medium shadow-inner-soft"
                        />
                        {searchQuery && (
                            <button
                                onClick={() => setSearchQuery('')}
                                className="absolute right-3 top-1/2 -translate-y-1/2 text-theme-light hover:text-rose-500 hover:bg-rose-50 rounded-full p-1 transition-colors"
                                title={t('contacts.clearSearch')}
                            >
                                <X size={16} />
                            </button>
                        )}
                    </div>
                    
                    <div className="flex items-center gap-2 overflow-x-auto pb-2 no-scrollbar scroll-smooth px-1">
                        <button
                            onClick={() => setSelectedTag(null)}
                            className={`px-4 py-2.5 rounded-xl text-[10px] font-black uppercase tracking-widest transition-all whitespace-nowrap border ${
                                !selectedTag 
                                    ? 'bg-theme-secondary border-theme-secondary text-white shadow-lg shadow-theme-secondary/20' 
                                    : 'bg-white border-theme-subtle text-theme-light hover:border-theme-primary/30'
                            }`}
                        >
                            {t('contacts.filterAll')}
                        </button>
                        {allTags.map(tag => (
                            <button
                                key={tag}
                                onClick={() => setSelectedTag(tag)}
                                className={`px-4 py-2.5 rounded-xl text-[10px] font-black uppercase tracking-widest transition-all whitespace-nowrap border ${
                                    selectedTag === tag
                                        ? 'bg-theme-secondary border-theme-secondary text-white shadow-lg shadow-theme-secondary/20'
                                        : 'bg-white border-theme-subtle text-theme-light hover:border-theme-primary/30'
                                }`}
                            >
                                {tag}
                            </button>
                        ))}
                    </div>
                </div>

                {/* Contact List */}
                {isLoading ? (
                    <div className="py-20 text-center animate-pulse text-theme-light font-black uppercase tracking-[0.2em]">
                        {t('contacts.loading')}
                    </div>
                ) : filteredContacts.length === 0 ? (
                    <div className="py-24 flex flex-col items-center justify-center text-center bg-white/30 backdrop-blur-sm rounded-[40px] border-2 border-dashed border-theme-subtle/50">
                        <div className="p-6 bg-theme-subtle/10 rounded-full text-theme-light mb-6">
                            <Users size={48} />
                        </div>
                        <h3 className="text-2xl font-black text-theme-primary tracking-tight">{t('contacts.emptyTitle')}</h3>
                        <p className="text-theme-light mt-2 max-w-sm font-medium">{t('contacts.emptyDescription')}</p>
                        <Button
                            variant="secondary"
                            onClick={() => {
                                setEditingContact(null);
                                setIsDialogOpen(true);
                            }}
                            className="mt-8 rounded-2xl"
                        >
                            {t('contacts.addFirstContact')}
                        </Button>
                    </div>
                ) : (
                    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                        {filteredContacts.map(contact => (
                            <div key={contact.did} className="group h-full">
                                <div className="h-full bg-white border border-theme-subtle rounded-[32px] p-6 flex flex-col group-hover:border-theme-primary/30 group-hover:shadow-premium transition-all">
                                    <div className="flex items-start justify-between mb-4">
                                        <div className="relative">
                                            <div className="w-16 h-16 rounded-[24px] overflow-hidden border-2 border-theme-subtle group-hover:border-theme-primary/30 transition-all p-1 bg-white shadow-inner">
                                                <div className="w-full h-full rounded-[18px] overflow-hidden">
                                                    <Avatar
                                                        size={64}
                                                        name={contact.did}
                                                        variant="beam"
                                                        colors={["#E63946", "#2B1B17", "#E76F51", "#F4A261", "#2A9D8F"]}
                                                    />
                                                </div>
                                            </div>
                                            <div className="absolute -bottom-1 -right-1 bg-white p-1 rounded-full shadow-sm border border-theme-subtle">
                                                <div className="bg-emerald-500 w-3 h-3 rounded-full border-2 border-white"></div>
                                            </div>
                                        </div>
                                        <div className="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                                            <button 
                                                onClick={() => {
                                                    setEditingContact(contact);
                                                    setIsDialogOpen(true);
                                                }}
                                                className="p-2 hover:bg-theme-subtle/20 rounded-xl text-theme-light hover:text-theme-primary transition-all"
                                                title={t('contacts.editProfile')}
                                            >
                                                <Edit3 size={18} />
                                            </button>
                                            <button 
                                                onClick={() => setDeleteRequest(contact.did)}
                                                className="p-2 hover:bg-rose-50 rounded-xl text-theme-light hover:text-rose-500 transition-all"
                                                title={t('contacts.removeContact')}
                                            >
                                                <Trash2 size={18} />
                                            </button>
                                        </div>
                                    </div>

                                    <div className="flex-1">
                                        <h3 className="text-lg font-black text-theme-primary tracking-tight leading-tight group-hover:text-theme-accent transition-colors">
                                            {contact.profile.firstName || contact.profile.lastName 
                                                ? `${contact.profile.firstName || ''} ${contact.profile.lastName || ''}`.trim()
                                                : contact.profile.organization || t('contacts.unnamedIdentity')}
                                        </h3>
                                        
                                        <div className="flex items-center gap-1.5 mt-2 mb-4">
                                            <Shield size={12} className="text-theme-light/60" />
                                            <p className="text-[10px] font-mono font-bold text-theme-light/60 truncate" title={contact.did}>
                                                {truncateDid(contact.did)}
                                            </p>
                                        </div>

                                        {(contact.profile.organization || contact.profile.community) && (
                                            <div className="flex items-center gap-2 mb-4 p-2.5 bg-theme-subtle/10 rounded-2xl border border-theme-subtle/20">
                                                <Building2 size={14} className="text-theme-light" />
                                                <span className="text-xs font-bold text-theme-secondary truncate">
                                                    {contact.profile.organization || contact.profile.community}
                                                </span>
                                            </div>
                                        )}

                                        <div className="flex flex-wrap gap-1.5 mt-auto">
                                            {contact.tags.map(tag => (
                                                <span key={tag} className="flex items-center gap-1 px-2.5 py-1 bg-white border border-theme-subtle/50 rounded-lg text-[9px] uppercase tracking-widest font-black text-theme-light shadow-sm">
                                                    <Tag size={8} />
                                                    {tag}
                                                </span>
                                            ))}
                                            {contact.tags.length === 0 && (
                                                <span className="text-[9px] font-black uppercase tracking-widest text-theme-light/40 italic">{t('contacts.noTagsAssigned')}</span>
                                            )}
                                        </div>
                                    </div>
                                </div>
                            </div>
                        ))}
                    </div>
                )}
            </div>

            <ContactDialog
                isOpen={isDialogOpen}
                onClose={() => setIsDialogOpen(false)}
                onSave={handleSaveContact}
                existingContact={editingContact}
            />

            <ConfirmationModal
                isOpen={!!deleteRequest}
                title={t('contacts.deleteTitle')}
                description={t('contacts.deleteDescription')}
                confirmText={t('contacts.deleteConfirmButton')}
                confirmVariant="danger"
                onConfirm={handleDeleteContact}
                onCancel={() => setDeleteRequest(null)}
            />
        </PageLayout>
    );
};

export default AddressBook;
