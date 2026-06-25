/**
 * Cleans a BIP-39 mnemonic phrase or word input by:
 * - Converting all characters to lowercase
 * - Replacing common delimiters, numbers, and punctuation with spaces
 * - Replacing tabs, carriage returns, and newlines with spaces
 * - Collapsing multiple consecutive spaces into a single space
 * - Trimming leading and trailing spaces
 */
export const cleanSeedText = (text: string): string => {
    return text
        .toLowerCase()
        .replace(/[0-9.,\-:]/g, ' ') // Remove digits and punctuation
        .replace(/[\r\n\t]/g, ' ')      // Replace tabs and newlines with space
        .replace(/\s+/g, ' ')          // Collapse multiple spaces
        .trim();
};
