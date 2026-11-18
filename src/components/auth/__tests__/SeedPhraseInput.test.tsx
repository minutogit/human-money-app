import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { SeedPhraseInput, SeedPhraseValue } from '../SeedPhraseInput';
import { profileService } from '../../../services/profileService';
import { recoveryService } from '../../../services/recoveryService';

vi.mock('../../../services/profileService', () => ({
    profileService: {
        getWordlist: vi.fn(),
        validateMnemonic: vi.fn(),
    },
}));

vi.mock('../../../services/recoveryService', () => ({
    recoveryService: {
        inspectEmergencyKitUri: vi.fn(),
    },
}));

const SEED_12 =
    'apple banana cherry date elderberry fig grape honey iris jasmine kiwi lemon';
const SEED_24 =
    'apple banana cherry date elderberry fig grape honey iris jasmine kiwi lemon mango nut olive peach quince raspberry strawberry tangerine umbrella violet watermelon xylem';

describe('SeedPhraseInput Component', () => {
    const onChange = vi.fn<(val: SeedPhraseValue) => void>();

    beforeEach(() => {
        vi.clearAllMocks();
        localStorage.clear();
        vi.mocked(profileService.getWordlist).mockResolvedValue([
            'apple',
            'banana',
            'cherry',
            'date',
        ]);
        vi.mocked(profileService.validateMnemonic).mockResolvedValue(undefined);
    });

    it('renders 12 word boxes by default and toggles to 24', async () => {
        render(<SeedPhraseInput onChange={onChange} idPrefix="test" />);

        expect(screen.getAllByTestId(/^test-word-/)).toHaveLength(12);

        fireEvent.click(screen.getByTestId('seed-word-count-24'));

        expect(screen.getAllByTestId(/^test-word-/)).toHaveLength(24);
    });

    it('auto-expands from 12 to 24 words when pasting a 24-word phrase into box 0', async () => {
        render(<SeedPhraseInput onChange={onChange} idPrefix="test" />);

        fireEvent.change(screen.getByTestId('test-word-0'), {
            target: { value: SEED_24 },
        });

        await waitFor(() => {
            expect(screen.getAllByTestId(/^test-word-/)).toHaveLength(24);
            expect(screen.getByTestId('test-word-0')).toHaveValue('apple');
            expect(screen.getByTestId('test-word-23')).toHaveValue('xylem');
        });

        await waitFor(() => {
            expect(profileService.validateMnemonic).toHaveBeenCalledWith(SEED_24, expect.any(String));
        });
    });

    it('detects humoco://kit URI on paste, populates fields, and auto-opens disclosure when passphrase flag is set', async () => {
        vi.mocked(recoveryService.inspectEmergencyKitUri).mockResolvedValue({
            phrase: SEED_12,
            wordCount: 12,
            language: 'german',
            hasBip39Passphrase: true,
            userPrefix: 'mobil',
            syncUrls: ['https://sync.humoco.org/sync.php'],
            syncDescriptorsJson: [],
        });

        render(<SeedPhraseInput onChange={onChange} allowKitUri idPrefix="test" />);

        fireEvent.change(screen.getByTestId('test-word-0'), {
            target: { value: 'humoco://kit?v=1&ent=abcd&lang=1&pfx=mobil&pp=1' },
        });

        await waitFor(() => {
            expect(screen.getByTestId('detected-kit-banner')).toBeInTheDocument();
            expect(screen.getByTestId('test-word-11')).toHaveValue('lemon');
            expect(screen.getByTestId('seed-advanced-content')).toBeInTheDocument();
            expect(screen.getByTestId('kit-passphrase-required-hint')).toBeInTheDocument();
            expect(screen.getByTestId('seed-user-prefix-input')).toHaveValue('mobil');
        });

        // Toggle passphrase visibility with the eye icon
        const ppInput = screen.getByTestId('seed-passphrase-input');
        expect(ppInput).toHaveAttribute('type', 'password');
        fireEvent.click(screen.getByTestId('input-visibility-toggle'));
        expect(ppInput).toHaveAttribute('type', 'text');

        // Clear kit banner via Change button
        fireEvent.click(screen.getByTestId('clear-detected-kit-btn'));
        expect(screen.queryByTestId('detected-kit-banner')).not.toBeInTheDocument();
    });

    it('auto-detects matching BIP-39 language when primary language validation fails', async () => {
        vi.mocked(profileService.validateMnemonic).mockImplementation(async (_phrase, lang) => {
            if (lang !== 'german') {
                throw new Error('Invalid word for language');
            }
        });

        render(<SeedPhraseInput onChange={onChange} idPrefix="test" />);

        fireEvent.change(screen.getByTestId('test-word-0'), {
            target: { value: SEED_12 },
        });

        await waitFor(() => {
            expect(screen.getByTestId('seed-validation-status')).toHaveTextContent(
                'Seed phrase is valid.'
            );
            const calls = onChange.mock.calls;
            const lastCall = calls[calls.length - 1]?.[0];
            expect(lastCall?.language).toBe('german');
            expect(lastCall?.isValid).toBe(true);
        });
    });

    it('auto-opens disclosure and respects showUserPrefix=false & focusPrefixTrigger', async () => {
        const { rerender } = render(
            <SeedPhraseInput
                onChange={onChange}
                showUserPrefix={false}
                forceOpenAdvanced
                idPrefix="nopfx"
            />
        );

        expect(screen.getByTestId('seed-advanced-content')).toBeInTheDocument();
        expect(screen.queryByTestId('seed-user-prefix-input')).not.toBeInTheDocument();

        rerender(
            <SeedPhraseInput
                onChange={onChange}
                showUserPrefix
                focusPrefixTrigger={1}
                idPrefix="withpfx"
            />
        );

        await waitFor(() => {
            expect(screen.getByTestId('seed-user-prefix-input')).toBeInTheDocument();
        });
    });
});
