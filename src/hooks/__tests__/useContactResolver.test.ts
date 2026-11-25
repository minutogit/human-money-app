import { renderHook, act, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { useContactResolver, invalidateContactResolverCache } from '../useContactResolver';
import { contactService } from '../../services/contactService';
import { Contact } from '../../types';

vi.mock('../../services/contactService', () => ({
    contactService: {
        getContacts: vi.fn(),
    },
}));

const alice: Contact = {
    did: 'did:key:zAlice123456',
    profile: {
        id: 'did:key:zAlice123456',
        firstName: 'Alice',
        lastName: 'Wonder',
    },
    tags: [],
    addedAt: '2026-01-01T00:00:00.000Z',
};

const orgContact: Contact = {
    did: 'did:key:zOrg987654',
    profile: {
        id: 'did:key:zOrg987654',
        organization: 'Bakery Guild',
    },
    tags: [],
    addedAt: '2026-01-01T00:00:00.000Z',
};

const bob: Contact = {
    did: 'did:key:zBob654321',
    profile: {
        id: 'did:key:zBob654321',
        firstName: 'Bob',
        lastName: 'Builder',
    },
    tags: [],
    addedAt: '2026-01-02T00:00:00.000Z',
};

describe('useContactResolver', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        invalidateContactResolverCache();
    });

    afterEach(() => {
        invalidateContactResolverCache();
    });

    it('resolves identities following the address book priority hierarchy', async () => {
        vi.mocked(contactService.getContacts).mockResolvedValue([alice, orgContact]);

        const { result } = renderHook(() => useContactResolver());

        await waitFor(() => {
            expect(result.current.isLoading).toBe(false);
        });

        expect(result.current.isKnownContact(alice.did)).toBe(true);
        expect(result.current.isKnownContact('did:key:unknown')).toBe(false);
        expect(result.current.isKnownContact(null)).toBe(false);

        // Address book wins over provided name
        expect(result.current.resolveIdentity(alice.did, 'Spoofed Name')).toBe('Alice Wonder');
        // Organization fallback in address book
        expect(result.current.resolveIdentity(orgContact.did)).toBe('Bakery Guild');
        // Unknown DID uses provided name unless Anonymous/empty
        expect(result.current.resolveIdentity('did:key:zOther123456', 'External User')).toBe('External User');
        expect(result.current.resolveIdentity('did:key:zOther123456', 'Anonymous')).toContain('did:key');
        // Fallback when neither DID nor name is available
        expect(result.current.resolveIdentity(null, '  ')).toBe('Unknown Sender');
    });

    it('deduplicates concurrent fetches and shares cached contacts across mounted hooks', async () => {
        let resolveFetch!: (contacts: Contact[]) => void;
        vi.mocked(contactService.getContacts).mockImplementation(
            () =>
                new Promise<Contact[]>((resolve) => {
                    resolveFetch = resolve;
                })
        );

        const hookA = renderHook(() => useContactResolver());
        const hookB = renderHook(() => useContactResolver());

        expect(contactService.getContacts).toHaveBeenCalledTimes(1);

        await act(async () => {
            resolveFetch([alice]);
        });

        await waitFor(() => {
            expect(hookA.result.current.isLoading).toBe(false);
            expect(hookB.result.current.isLoading).toBe(false);
        });

        expect(hookA.result.current.resolveIdentity(alice.did)).toBe('Alice Wonder');
        expect(hookB.result.current.resolveIdentity(alice.did)).toBe('Alice Wonder');

        // Sequential mount within TTL uses cache without another IPC call
        const hookC = renderHook(() => useContactResolver());
        expect(hookC.result.current.isLoading).toBe(false);
        expect(hookC.result.current.resolveIdentity(alice.did)).toBe('Alice Wonder');
        expect(contactService.getContacts).toHaveBeenCalledTimes(1);
    });

    it('forces a fresh fetch on refresh() and updates all mounted subscribers', async () => {
        vi.mocked(contactService.getContacts)
            .mockResolvedValueOnce([alice])
            .mockResolvedValueOnce([alice, bob]);

        const hookA = renderHook(() => useContactResolver());
        const hookB = renderHook(() => useContactResolver());

        await waitFor(() => {
            expect(hookA.result.current.isLoading).toBe(false);
        });

        expect(hookB.result.current.isKnownContact(bob.did)).toBe(false);

        await act(async () => {
            await hookA.result.current.refresh();
        });

        expect(contactService.getContacts).toHaveBeenCalledTimes(2);
        expect(hookA.result.current.isKnownContact(bob.did)).toBe(true);
        expect(hookB.result.current.isKnownContact(bob.did)).toBe(true);
    });

    it('refreshes on humoco:contacts-updated and clears on humoco:session-locked', async () => {
        vi.mocked(contactService.getContacts)
            .mockResolvedValueOnce([alice])
            .mockResolvedValueOnce([alice, bob]);

        const { result } = renderHook(() => useContactResolver());

        await waitFor(() => {
            expect(result.current.isLoading).toBe(false);
        });
        expect(result.current.isKnownContact(bob.did)).toBe(false);

        act(() => {
            window.dispatchEvent(new CustomEvent('humoco:contacts-updated'));
        });

        await waitFor(() => {
            expect(result.current.isKnownContact(bob.did)).toBe(true);
        });
        expect(contactService.getContacts).toHaveBeenCalledTimes(2);

        act(() => {
            window.dispatchEvent(new CustomEvent('humoco:session-locked'));
        });

        expect(result.current.contacts).toEqual([]);
        expect(result.current.isKnownContact(alice.did)).toBe(false);
    });

    it('recovers cleanly after a fetch rejection and refetches after TTL expiry', async () => {
        const consoleErrSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
        const nowSpy = vi.spyOn(Date, 'now').mockReturnValue(10_000);
        try {
            vi.mocked(contactService.getContacts)
                .mockRejectedValueOnce(new Error('IPC down'))
                .mockResolvedValueOnce([alice])
                .mockResolvedValueOnce([alice, bob]);

            const firstHook = renderHook(() => useContactResolver());
            await waitFor(() => {
                expect(firstHook.result.current.isLoading).toBe(false);
            });
            expect(firstHook.result.current.contacts).toEqual([]);
            firstHook.unmount();

            // Retry mounts must NOT stick to a rejected inFlightFetch promise
            const secondHook = renderHook(() => useContactResolver());
            await waitFor(() => {
                expect(secondHook.result.current.isLoading).toBe(false);
            });
            expect(contactService.getContacts).toHaveBeenCalledTimes(2);
            expect(secondHook.result.current.resolveIdentity(alice.did)).toBe('Alice Wonder');
            secondHook.unmount();

            // Within 2000ms TTL: uses cached snapshot
            nowSpy.mockReturnValue(11_000);
            const cachedHook = renderHook(() => useContactResolver());
            expect(cachedHook.result.current.isLoading).toBe(false);
            expect(contactService.getContacts).toHaveBeenCalledTimes(2);
            cachedHook.unmount();

            // After >= 2000ms TTL: triggers fresh IPC fetch
            nowSpy.mockReturnValue(12_001);
            const expiredHook = renderHook(() => useContactResolver());
            await waitFor(() => {
                expect(expiredHook.result.current.isKnownContact(bob.did)).toBe(true);
            });
            expect(contactService.getContacts).toHaveBeenCalledTimes(3);
        } finally {
            nowSpy.mockRestore();
            consoleErrSpy.mockRestore();
        }
    });
});
