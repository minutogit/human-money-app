import { describe, it, expect } from 'vitest';
import {
    extractDisplayName,
    truncateUserId,
    suggestFilename,
    extractCanonicalDid,
    isExactSameUserId,
    isSameRootIdentity,
} from '../userIdHelper';

describe('userIdHelper', () => {
    const saiId = 'alice:abc123@did:key:z6MkpTHR8VNsBxYqzFoHqi1oMrY';
    const rootId = 'did:key:z6MkpTHR8VNsBxYqzFoHqi1oMrY';

    describe('extractDisplayName', () => {
        it('extracts name from SAI ID', () => {
            expect(extractDisplayName(saiId)).toBe('alice');
        });

        it('extracts start of key from root ID', () => {
            expect(extractDisplayName(rootId)).toBe('z6MkpTHR');
        });

        it('returns placeholder for empty ID', () => {
            expect(extractDisplayName('')).toBe('Unknown');
        });
    });

    describe('truncateUserId', () => {
        it('truncates SAI ID correctly', () => {
            // key is 'did:key:z6MkpTHR8VNsBxYqzFoHqi1oMrY', first 12 after did:key: is 'z6MkpTHR8VNs', last 5 is '1oMrY'
            expect(truncateUserId(saiId)).toBe('alice:abc123@did:key:z6MkpTHR8VNs...1oMrY');
        });

        it('truncates root ID correctly', () => {
            // key part 'z6MkpTHR8VNsBxYqzFoHqi1oMrY', start 20 is 'z6MkpTHR8VNsBxYqzFoH', last 5 is '1oMrY'
            expect(truncateUserId(rootId)).toBe('did:key:z6MkpTHR8VNsBxYqzFoH...1oMrY');
        });
    });

    describe('suggestFilename', () => {
        it('suggests name from SAI ID', () => {
            expect(suggestFilename(saiId)).toBe('alice');
        });

        it('suggests key part from root ID', () => {
            expect(suggestFilename(rootId)).toBe('z6MkpTHR');
        });

        it('returns fallback for empty ID', () => {
            expect(suggestFilename('')).toBe('transfer');
        });
    });

    describe('identity comparison helpers', () => {
        const ladenId = 'laden:xyz789@did:key:z6MkpTHR8VNsBxYqzFoHqi1oMrY';
        const bobId = 'did:key:z6MkOtherPerson123456789';

        it('extractCanonicalDid strips subaccount prefix', () => {
            expect(extractCanonicalDid(saiId)).toBe(rootId);
            expect(extractCanonicalDid(rootId)).toBe(rootId);
            expect(extractCanonicalDid('')).toBe('');
        });

        it('isExactSameUserId matches exact trimmed IDs including prefix', () => {
            expect(isExactSameUserId(saiId, ` ${saiId} `)).toBe(true);
            expect(isExactSameUserId(saiId, ladenId)).toBe(false);
            expect(isExactSameUserId(saiId, rootId)).toBe(false);
            expect(isExactSameUserId('', '')).toBe(false);
        });

        it('isSameRootIdentity detects own root and sibling subaccounts', () => {
            expect(isSameRootIdentity(saiId, rootId)).toBe(true);
            expect(isSameRootIdentity(saiId, ladenId)).toBe(true);
            expect(isSameRootIdentity(saiId, bobId)).toBe(false);
            expect(isSameRootIdentity('not-a-did', 'not-a-did')).toBe(false);
        });
    });
});
