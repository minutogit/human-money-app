import { describe, it, expect } from 'vitest';
import { extractDisplayName, truncateUserId, suggestFilename } from '../userIdHelper';

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
            // key is 'did:key:z6MkpTHR8VNsBxYqzFoHqi1oMrY', last 6 is 'i1oMrY'
            expect(truncateUserId(saiId)).toBe('alice:abc123@...i1oMrY');
        });

        it('truncates root ID correctly', () => {
            // key part 'z6MkpTHR8VNsBxYqzFoHqi1oMrY', start 8 is 'z6MkpTHR', last 5 is '1oMrY'
            expect(truncateUserId(rootId)).toBe('did:key:z6MkpTHR...1oMrY');
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
});
