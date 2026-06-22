import { useState, useEffect, useCallback } from 'react';
import { contactService } from '../services/contactService';
import { Contact } from '../types';
import { truncateUserId } from '../utils/userIdHelper';

/**
 * Hook to resolve DIDs and provided names to a human-readable identity.
 * Prioritizes the address book for fraud detection.
 */
export function useContactResolver() {
    const [contacts, setContacts] = useState<Contact[]>([]);
    const [isLoading, setIsLoading] = useState(true);

    const loadContacts = useCallback(async () => {
        try {
            const list = await contactService.getContacts();
            setContacts(list || []);
        } catch (error) {
            console.error('Failed to load contacts for resolver:', error);
        } finally {
            setIsLoading(false);
        }
    }, []);

    useEffect(() => {
        loadContacts();
    }, [loadContacts]);

    /**
     * Resolves a DID and/or provided name to a displayable string.
     * 
     * Hierarchy:
     * 1. Address Book: If the DID matches a contact, use the contact name.
     * 2. Provided Name: If present and not "Anonymous", use it.
     * 3. Truncated DID: If DID is present, show truncated version.
     * 4. Fallback: "Unknown Sender"
     */
    const resolveIdentity = useCallback((did?: string | null, providedName?: string | null): string => {
        // 1. Is the did in the address book? -> Priority for fraud detection
        if (did) {
            const contact = contacts.find(c => c.did === did);
            if (contact && contact.profile) {
                const { firstName, lastName, organization } = contact.profile;
                const nameParts = [firstName, lastName].filter(Boolean);
                if (nameParts.length > 0) return nameParts.join(' ');
                if (organization) return organization;
            }
        }

        // 2. Is there a provided name (that is NOT empty and NOT "Anonymous")?
        if (providedName && providedName.trim() !== '' && providedName.toLowerCase() !== 'anonymous') {
            return providedName;
        }

        // 3. Is the did present? -> Show truncated DID
        if (did) {
            return truncateUserId(did);
        }

        // 4. Fallback
        return "Unknown Sender";
    }, [contacts]);

    return { resolveIdentity, isLoading, refresh: loadContacts };
}
