import { describe, it, expect } from 'vitest';
import { cleanSeedText } from '../seedUtils';

describe('seedUtils - cleanSeedText', () => {
    it('converts letters to lowercase', () => {
        expect(cleanSeedText('APPLE BANANA CHERRY')).toBe('apple banana cherry');
    });

    it('removes numbers and numbered prefixes', () => {
        expect(cleanSeedText('1. apple 2. banana 3. cherry')).toBe('apple banana cherry');
        expect(cleanSeedText('12: apple 24: banana')).toBe('apple banana');
        expect(cleanSeedText('1-apple 2-banana')).toBe('apple banana');
    });

    it('removes punctuation like commas, dots, colons, and dashes', () => {
        expect(cleanSeedText('apple, banana, cherry.')).toBe('apple banana cherry');
        expect(cleanSeedText('apple-banana-cherry')).toBe('apple banana cherry');
        expect(cleanSeedText('apple:banana:cherry')).toBe('apple banana cherry');
    });

    it('replaces tabs, newlines and carriage returns with spaces', () => {
        expect(cleanSeedText('apple\tbanana\ncherry\rdate')).toBe('apple banana cherry date');
    });

    it('collapses multiple consecutive spaces and trims leading/trailing spaces', () => {
        expect(cleanSeedText('   apple    banana   ')).toBe('apple banana');
    });
});
