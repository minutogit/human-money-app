import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { EmergencyKitModal } from '../EmergencyKitModal';
import { recoveryService } from '../../../services/recoveryService';

vi.mock('../../../services/recoveryService', () => ({
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
        userPrefix: 'al1',
        hasBip39Passphrase: false,
        kitUri: 'humoco://kit?did=did:key:z6Mku&words=12',
        qrSvg: '<svg viewBox="0 0 10 10"><rect width="99" height="99"/></svg>',
        syncUrls: ['https://sync1.humoco.org'],
        guardianNames: ['Alice', 'Bob'],
    };

    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('unlocks with password and displays emergency sheet with 12 words', async () => {
        vi.mocked(recoveryService.exportProfileMnemonic).mockResolvedValue(mockKitData);

        const { baseElement } = render(
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
            expect(baseElement.querySelector('#emergency-sheet-printable svg rect[width="99"]')).not.toBeNull();
        });
    });

    it('confirms control word and finishes emergency sheet setup', async () => {
        vi.mocked(recoveryService.exportProfileMnemonic).mockResolvedValue(mockKitData);
        vi.mocked(recoveryService.confirmEmergencySheetSaved).mockResolvedValue({
            confirmedAt: Date.now() / 1000,
            backupSyncResult: null,
        });
        const randomSpy = vi.spyOn(Math, 'random').mockReturnValue(0.25);

        try {
            render(
                <EmergencyKitModal
                    isOpen={true}
                    onClose={mockOnClose}
                    onSuccess={mockOnSuccess}
                />
            );

            const passwordInput = screen.getByPlaceholderText(/Password|Passwort/i);
            fireEvent.change(passwordInput, { target: { value: 'CorrectPassword123' } });

            fireEvent.click(screen.getByTestId('unlock-emergency-kit-btn'));

            await screen.findByText('apple');

            // Go to confirmation step
            fireEvent.click(screen.getByTestId('proceed-confirm-sheet-btn'));

            // Word 4 is 'date' (0.25 * 12 + 1 = 4)
            expect(
                screen.getByText('To confirm, enter word #4 from your printout:')
            ).toBeInTheDocument();

            const controlInput = await screen.findByTestId('control-word-input');
            fireEvent.change(controlInput, { target: { value: '  date  ' } });

            fireEvent.click(screen.getByTestId('submit-confirm-word-btn'));

            await waitFor(() => {
                expect(recoveryService.confirmEmergencySheetSaved).toHaveBeenCalledWith(
                    'CorrectPassword123',
                    4,
                    'date'
                );
                expect(
                    screen.getByText('Emergency sheet successfully confirmed and status updated!')
                ).toBeInTheDocument();
                expect(mockOnSuccess).toHaveBeenCalledTimes(1);
            });
        } finally {
            randomSpy.mockRestore();
        }
    });

    it('supports 24-word profiles dynamically in display and control-word index bounds', async () => {
        const phrase24 =
            'apple banana cherry date elderberry fig grape honey iris jasmine kiwi lemon mango nut olive peach quince raspberry strawberry tangerine umbrella violet watermelon xylem';
        vi.mocked(recoveryService.exportProfileMnemonic).mockResolvedValue({
            phrase: phrase24,
            languageCode: 0,
            userPrefix: 'al1',
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

        try {
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

            expect(
                screen.getByText('To confirm, enter word #24 from your printout:')
            ).toBeInTheDocument();

            const controlInput = await screen.findByTestId('control-word-input');
            fireEvent.change(controlInput, { target: { value: 'xylem' } });
            fireEvent.click(screen.getByTestId('submit-confirm-word-btn'));

            await waitFor(() => {
                expect(recoveryService.confirmEmergencySheetSaved).toHaveBeenCalledWith(
                    'CorrectPassword123',
                    24,
                    'xylem'
                );
                expect(mockOnSuccess).toHaveBeenCalledTimes(1);
            });
        } finally {
            randomSpy.mockRestore();
        }
    });

    it('exports localized HTML without hardcoded German strings when downloaded', async () => {
        vi.mocked(recoveryService.exportProfileMnemonic).mockResolvedValue(mockKitData);
        const blobParts: string[] = [];
        const OrigBlob = globalThis.Blob;
        class CapturingBlob extends OrigBlob {
            constructor(parts: BlobPart[], options?: BlobPropertyBag) {
                super(parts, options);
                blobParts.push(...parts.map(String));
            }
        }
        vi.stubGlobal('Blob', CapturingBlob);
        const origCreateObjectURL = URL.createObjectURL;
        const origRevokeObjectURL = URL.revokeObjectURL;
        URL.createObjectURL = vi.fn(() => 'blob:mock-url');
        URL.revokeObjectURL = vi.fn();
        const clickSpy = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});

        try {
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
                expect(screen.getByText('apple')).toBeInTheDocument();
            });

            fireEvent.click(screen.getByRole('button', { name: /Save as offline HTML|Als Offline-HTML speichern/i }));

            expect(blobParts.length).toBeGreaterThan(0);
            const html = blobParts.join('');
            expect(html).toContain('lang="en"');
            expect(html).toContain('HuMoCo Profile Recovery (Emergency Kit)');
            expect(html).toContain('<svg');
            expect(html).not.toContain('Wiederherstellungswörter');
            expect(html).not.toContain('Anleitung für den Ernstfall');
            expect(clickSpy).toHaveBeenCalled();
        } finally {
            vi.unstubAllGlobals();
            URL.createObjectURL = origCreateObjectURL;
            URL.revokeObjectURL = origRevokeObjectURL;
            clickSpy.mockRestore();
        }
    });

    it('sanitizes/rejects unsafe SVG markup in both modal preview and downloaded HTML export', async () => {
        vi.mocked(recoveryService.exportProfileMnemonic).mockResolvedValue({
            ...mockKitData,
            qrSvg: '<svg onload="alert(1)"><script>alert(1)</script><rect width="99" height="99"/></svg>',
        });

        const blobParts: string[] = [];
        const OrigBlob = globalThis.Blob;
        class CapturingBlob extends OrigBlob {
            constructor(parts: BlobPart[], options?: BlobPropertyBag) {
                super(parts, options);
                blobParts.push(...parts.map(String));
            }
        }
        vi.stubGlobal('Blob', CapturingBlob);
        const origCreateObjectURL = URL.createObjectURL;
        const origRevokeObjectURL = URL.revokeObjectURL;
        URL.createObjectURL = vi.fn(() => 'blob:mock-url');
        URL.revokeObjectURL = vi.fn();
        const clickSpy = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});

        try {
            const { baseElement } = render(
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
                expect(screen.getByText('apple')).toBeInTheDocument();
            });

            expect(baseElement.querySelector('script')).toBeNull();
            expect(baseElement.innerHTML).not.toContain('onload=');
            expect(baseElement.querySelector('#emergency-sheet-printable rect[width="99"]')).toBeNull();

            fireEvent.click(screen.getByRole('button', { name: /Save as offline HTML|Als Offline-HTML speichern/i }));
            const html = blobParts.join('');
            expect(html).not.toContain('<script');
            expect(html).not.toContain('onload=');
            expect(html).not.toContain('width="99"');
        } finally {
            vi.unstubAllGlobals();
            URL.createObjectURL = origCreateObjectURL;
            URL.revokeObjectURL = origRevokeObjectURL;
            clickSpy.mockRestore();
        }
    });

    it('shows not-retained warning without advancing to sheet when exportProfileMnemonic returns null', async () => {
        vi.mocked(recoveryService.exportProfileMnemonic).mockResolvedValue(null);

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
            expect(
                screen.getByText(
                    'For this profile, it was specified at creation that the recovery phrase would not be stored on the device. An emergency sheet cannot be exported later.'
                )
            ).toBeInTheDocument();
        });

        expect(screen.queryByTestId('proceed-confirm-sheet-btn')).toBeNull();
        expect(recoveryService.confirmEmergencySheetSaved).not.toHaveBeenCalled();
        expect(mockOnSuccess).not.toHaveBeenCalled();
    });

    it('displays unlock error and stays on password step when exportProfileMnemonic rejects', async () => {
        vi.mocked(recoveryService.exportProfileMnemonic).mockRejectedValue(
            new Error('Invalid password')
        );

        render(
            <EmergencyKitModal
                isOpen={true}
                onClose={mockOnClose}
                onSuccess={mockOnSuccess}
            />
        );

        fireEvent.change(screen.getByPlaceholderText(/Password|Passwort/i), {
            target: { value: 'WrongPass' },
        });
        fireEvent.click(screen.getByTestId('unlock-emergency-kit-btn'));

        await waitFor(() => {
            expect(screen.getByText('Invalid password')).toBeInTheDocument();
        });

        expect(screen.getByTestId('unlock-emergency-kit-btn')).toBeInTheDocument();
        expect(screen.queryByTestId('proceed-confirm-sheet-btn')).toBeNull();
        expect(mockOnSuccess).not.toHaveBeenCalled();
    });

    it('displays error and keeps modal on confirm_word step without calling onSuccess when control word verification fails', async () => {
        vi.mocked(recoveryService.exportProfileMnemonic).mockResolvedValue(mockKitData);
        vi.mocked(recoveryService.confirmEmergencySheetSaved).mockRejectedValue(
            new Error('Control word does not match')
        );
        const randomSpy = vi.spyOn(Math, 'random').mockReturnValue(0.25);

        try {
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
            await screen.findByText('apple');

            fireEvent.click(screen.getByTestId('proceed-confirm-sheet-btn'));
            fireEvent.change(await screen.findByTestId('control-word-input'), {
                target: { value: 'wrongword' },
            });
            fireEvent.click(screen.getByTestId('submit-confirm-word-btn'));

            await waitFor(() => {
                expect(recoveryService.confirmEmergencySheetSaved).toHaveBeenCalledWith(
                    'CorrectPassword123',
                    4,
                    'wrongword'
                );
                expect(screen.getByText('Control word does not match')).toBeInTheDocument();
            });

            expect(screen.getByTestId('control-word-input')).toBeInTheDocument();
            expect(
                screen.queryByText('Emergency sheet successfully confirmed and status updated!')
            ).toBeNull();
            expect(mockOnSuccess).not.toHaveBeenCalled();
        } finally {
            randomSpy.mockRestore();
        }
    });
});
