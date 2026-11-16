import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { EmergencyKitModal } from '../EmergencyKitModal';
import { recoveryService } from '../../services/recoveryService';

vi.mock('../../services/recoveryService', () => ({
    recoveryService: {
        exportProfileMnemonic: vi.fn(),
        confirmEmergencySheetSaved: vi.fn(),
    },
}));

describe('EmergencyKitModal Component', () => {
    const mockOnClose = vi.fn();
    const mockOnSuccess = vi.fn();

    const mockKitData = {
        phrase: 'apple banana cherry date elderberry fig grape honey iris jasmine kiwi lemon',
        languageCode: 0,
        hasBip39Passphrase: false,
        kitUri: 'humoco://kit?did=did:key:z6Mku&words=12',
        qrSvg: '<svg data-testid="qr-svg"></svg>',
        syncUrls: ['https://sync1.humoco.org'],
        guardianNames: ['Alice', 'Bob'],
    };

    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('unlocks with password and displays emergency sheet with 12 words', async () => {
        vi.mocked(recoveryService.exportProfileMnemonic).mockResolvedValue(mockKitData);

        render(
            <EmergencyKitModal
                isOpen={true}
                onClose={mockOnClose}
                onSuccess={mockOnSuccess}
            />
        );

        const passwordInput = screen.getByPlaceholderText(/Password|Passwort/i);
        fireEvent.change(passwordInput, { target: { value: 'CorrectPassword123' } });

        const unlockBtn = screen.getByTestId('unlock-emergency-kit-btn');
        fireEvent.click(unlockBtn);

        await waitFor(() => {
            expect(recoveryService.exportProfileMnemonic).toHaveBeenCalledWith('CorrectPassword123');
            expect(screen.getByText('apple')).toBeInTheDocument();
            expect(screen.getByText('banana')).toBeInTheDocument();
            expect(screen.getByText('lemon')).toBeInTheDocument();
        });
    });

    it('confirms control word and finishes emergency sheet setup', async () => {
        vi.mocked(recoveryService.exportProfileMnemonic).mockResolvedValue(mockKitData);
        vi.mocked(recoveryService.confirmEmergencySheetSaved).mockResolvedValue({
            confirmedAt: Date.now() / 1000,
            backupSyncResult: null,
        });

        render(
            <EmergencyKitModal
                isOpen={true}
                onClose={mockOnClose}
                onSuccess={mockOnSuccess}
            />
        );

        const passwordInput = screen.getByPlaceholderText(/Password|Passwort/i);
        fireEvent.change(passwordInput, { target: { value: 'CorrectPassword123' } });

        const unlockBtn = screen.getByTestId('unlock-emergency-kit-btn');
        fireEvent.click(unlockBtn);

        await screen.findByText('apple');

        // Go to confirmation step
        const proceedConfirmBtn = screen.getByTestId('proceed-confirm-sheet-btn');
        fireEvent.click(proceedConfirmBtn);

        // Word 4 is 'date'
        const controlInput = await screen.findByTestId('control-word-input');
        fireEvent.change(controlInput, { target: { value: 'date' } });

        const submitConfirmBtn = screen.getByTestId('submit-confirm-word-btn');
        fireEvent.click(submitConfirmBtn);

        await waitFor(() => {
            expect(recoveryService.confirmEmergencySheetSaved).toHaveBeenCalledWith(
                'CorrectPassword123',
                expect.any(Number),
                'date'
            );
        });
    });

    it('supports 24-word profiles dynamically in display and control-word index bounds', async () => {
        const phrase24 =
            'apple banana cherry date elderberry fig grape honey iris jasmine kiwi lemon mango nut olive peach quince raspberry strawberry tangerine umbrella violet watermelon xylem';
        vi.mocked(recoveryService.exportProfileMnemonic).mockResolvedValue({
            phrase: phrase24,
            languageCode: 0,
            hasBip39Passphrase: true,
            kitUri: 'humoco://kit?v=1&words=24',
            qrSvg: '<svg></svg>',
            syncUrls: [],
            guardianNames: [],
        });
        vi.mocked(recoveryService.confirmEmergencySheetSaved).mockResolvedValue({
            confirmedAt: Date.now() / 1000,
            backupSyncResult: null,
        });
        const randomSpy = vi.spyOn(Math, 'random').mockReturnValue(0.99);

        render(
            <EmergencyKitModal
                isOpen={true}
                onClose={mockOnClose}
                onSuccess={mockOnSuccess}
            />
        );

        fireEvent.change(screen.getByPlaceholderText(/Password|Passwort/i), {
            target: { value: 'CorrectPassword123' },
        });
        fireEvent.click(screen.getByTestId('unlock-emergency-kit-btn'));

        await waitFor(() => {
            expect(screen.getByText('24 recovery words')).toBeInTheDocument();
            expect(screen.getByText('xylem')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('proceed-confirm-sheet-btn'));

        const controlInput = await screen.findByTestId('control-word-input');
        fireEvent.change(controlInput, { target: { value: 'xylem' } });
        fireEvent.click(screen.getByTestId('submit-confirm-word-btn'));

        await waitFor(() => {
            expect(recoveryService.confirmEmergencySheetSaved).toHaveBeenCalledWith(
                'CorrectPassword123',
                24,
                'xylem'
            );
        });
        randomSpy.mockRestore();
    });
});
