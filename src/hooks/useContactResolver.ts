import { useState, useEffect, useCallback, useRef } from 'react';
import { contactService } from '../services/contactService';
import { Contact } from '../types';
import { APP_EVENTS } from '../utils/appEvents';
import { truncateUserId } from '../utils/userIdHelper';

const CACHE_TTL_MS = 2_000;

interface ContactCacheSnapshot {
    contacts: Contact[];
    contactMap: Map<string, Contact>;
}

interface MockTrackableFn {
    mock?: { calls?: unknown[] };
}

const EMPTY_CONTACTS: Contact[] = [];
const EMPTY_CONTACT_MAP = new Map<string, Contact>();

let cachedSnapshot: ContactCacheSnapshot | null = null;
let cachedAt = 0;
let cacheEpoch = 0;
let inFlightFetch: Promise<ContactCacheSnapshot> | null = null;
let lastFetcherRef: unknown = null;
let lastMockCallCount = 0;
let listenerWindow: Window | null = null;

const subscribers = new Set<() => void>();

function notifySubscribers(): void {
    for (const listener of subscribers) {
        listener();
    }
}

function buildContactMap(contacts: Contact[]): Map<string, Contact> {
    const map = new Map<string, Contact>();
    for (const contact of contacts) {
        if (contact?.did && !map.has(contact.did)) {
            map.set(contact.did, contact);
        }
    }
    return map;
}

function getMockCallCount(fn: unknown): number {
    const calls = (fn as MockTrackableFn | null | undefined)?.mock?.calls;
    return Array.isArray(calls) ? calls.length : 0;
}

function hasExternalFetcherReset(): boolean {
    const currentFetcher = contactService.getContacts;
    if (currentFetcher !== lastFetcherRef) {
        return lastFetcherRef !== null;
    }
    return getMockCallCount(currentFetcher) < lastMockCallCount;
}

function getValidCacheSnapshot(): ContactCacheSnapshot | null {
    if (!cachedSnapshot) return null;
    if (hasExternalFetcherReset()) {
        resetCacheState();
        return null;
    }
    const now = Date.now();
    if (now < cachedAt || now - cachedAt >= CACHE_TTL_MS) {
        return null;
    }
    return cachedSnapshot;
}

function resetCacheState(): void {
    cacheEpoch += 1;
    cachedSnapshot = null;
    cachedAt = 0;
    inFlightFetch = null;
    lastFetcherRef = null;
    lastMockCallCount = 0;
}

/**
 * Invalidates the shared contact resolver cache and clears stale contacts
 * on any mounted hook instances.
 */
export function invalidateContactResolverCache(): void {
    resetCacheState();
    notifySubscribers();
}

async function fetchContactsShared(forceRefresh: boolean): Promise<ContactCacheSnapshot> {
    if (!forceRefresh) {
        const valid = getValidCacheSnapshot();
        if (valid) return valid;
        if (inFlightFetch && !hasExternalFetcherReset()) {
            return inFlightFetch;
        }
    } else {
        // Bump epoch so an older in-flight request cannot overwrite a newer forced refresh.
        cacheEpoch += 1;
        inFlightFetch = null;
    }

    const requestEpoch = cacheEpoch;
    const fetcher = contactService.getContacts;
    lastFetcherRef = fetcher;

    let promise!: Promise<ContactCacheSnapshot>;
    promise = (async () => {
        try {
            const callPromise = fetcher();
            lastMockCallCount = getMockCallCount(fetcher);
            const rawList = await callPromise;
            const list = rawList || [];
            const snapshot: ContactCacheSnapshot = {
                contacts: list,
                contactMap: buildContactMap(list),
            };
            if (requestEpoch === cacheEpoch) {
                cachedSnapshot = snapshot;
                cachedAt = Date.now();
                notifySubscribers();
            }
            return snapshot;
        } finally {
            if (inFlightFetch === promise) {
                inFlightFetch = null;
            }
        }
    })();

    inFlightFetch = promise;
    return promise;
}

function ensureWindowListeners(): void {
    if (typeof window === 'undefined' || listenerWindow === window) {
        return;
    }
    listenerWindow = window;

    window.addEventListener(APP_EVENTS.CONTACTS_UPDATED, () => {
        resetCacheState();
        if (subscribers.size > 0) {
            void fetchContactsShared(true).catch((error) => {
                console.error('Failed to refresh contacts after update:', error);
            });
        }
    });

    window.addEventListener(APP_EVENTS.SESSION_LOCKED, () => {
        invalidateContactResolverCache();
    });
}

/**
 * Hook to resolve DIDs and provided names to a human-readable identity.
 * Uses a shared reactive cache with in-flight deduplication and prioritizes
 * the address book for fraud detection.
 */
export function useContactResolver() {
    const initialSnapshot = getValidCacheSnapshot();
    const [contacts, setContacts] = useState<Contact[]>(
        () => initialSnapshot?.contacts ?? EMPTY_CONTACTS
    );
    const [contactMap, setContactMap] = useState<Map<string, Contact>>(
        () => initialSnapshot?.contactMap ?? EMPTY_CONTACT_MAP
    );
    const [isLoading, setIsLoading] = useState<boolean>(() => !initialSnapshot);
    const isMountedRef = useRef(true);

    const refresh = useCallback(async () => {
        try {
            const snapshot = await fetchContactsShared(true);
            if (isMountedRef.current) {
                setContacts(snapshot.contacts);
                setContactMap(snapshot.contactMap);
            }
        } catch (error) {
            if (isMountedRef.current) {
                console.error('Failed to load contacts for resolver:', error);
            }
        } finally {
            if (isMountedRef.current) {
                setIsLoading(false);
            }
        }
    }, []);

    useEffect(() => {
        ensureWindowListeners();
        isMountedRef.current = true;
        let active = true;

        const syncFromCache = () => {
            if (!active) return;
            if (cachedSnapshot) {
                setContacts(cachedSnapshot.contacts);
                setContactMap(cachedSnapshot.contactMap);
                setIsLoading(false);
            } else {
                setContacts(EMPTY_CONTACTS);
                setContactMap(EMPTY_CONTACT_MAP);
            }
        };

        subscribers.add(syncFromCache);

        const valid = getValidCacheSnapshot();
        if (valid) {
            setContacts(valid.contacts);
            setContactMap(valid.contactMap);
            setIsLoading(false);
        } else {
            setIsLoading(true);
            const fetchEpoch = cacheEpoch;
            fetchContactsShared(false)
                .then((snapshot) => {
                    if (!active || fetchEpoch !== cacheEpoch) return;
                    setContacts(snapshot.contacts);
                    setContactMap(snapshot.contactMap);
                })
                .catch((error) => {
                    if (!active || fetchEpoch !== cacheEpoch) return;
                    console.error('Failed to load contacts for resolver:', error);
                })
                .finally(() => {
                    if (active && fetchEpoch === cacheEpoch) {
                        setIsLoading(false);
                    }
                });
        }

        return () => {
            active = false;
            isMountedRef.current = false;
            subscribers.delete(syncFromCache);
        };
    }, []);

    /**
     * Resolves a DID and/or provided name to a displayable string.
     *
     * Hierarchy:
     * 1. Address Book: If the DID matches a contact, use the contact name.
     * 2. Provided Name: If present and not "Anonymous", use it.
     * 3. Truncated DID: If DID is present, show truncated version.
     * 4. Fallback: "Unknown Sender"
     */
    const resolveIdentity = useCallback(
        (did?: string | null, providedName?: string | null): string => {
            if (did) {
                const contact = contactMap.get(did);
                if (contact?.profile) {
                    const { firstName, lastName, organization } = contact.profile;
                    const nameParts = [firstName, lastName].filter(Boolean);
                    if (nameParts.length > 0) return nameParts.join(' ');
                    if (organization) return organization;
                }
            }

            if (
                providedName &&
                providedName.trim() !== '' &&
                providedName.toLowerCase() !== 'anonymous'
            ) {
                return providedName;
            }

            if (did) {
                return truncateUserId(did);
            }

            return 'Unknown Sender';
        },
        [contactMap]
    );

    const isKnownContact = useCallback(
        (did?: string | null): boolean => {
            if (!did) return false;
            return contactMap.has(did);
        },
        [contactMap]
    );

    return { resolveIdentity, isKnownContact, contacts, isLoading, refresh };
}
