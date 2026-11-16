import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { WalletRecovery } from '../WalletRecovery';
import { invoke } from '@tauri-apps/api/core';
import { ProfileInfo } from '../../types';

vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-log', () => ({
    info: vi.fn(),
    error: vi.fn(),
}));

vi.mock('../../utils/log', () => ({
    logger: {
        info: vi.fn(),
        error: vi.fn(),
    },
}));

describe('WalletRecovery Component', () => {
    const mockOnRecoverySuccess = vi.fn();
    const mockOnSwitchToLogin = vi.fn();

    const mockSeed12 =
        'apple banana cherry date elderberry fig grape honey iris jasmine kiwi lemon';
    const mockSeed24 =
        'apple banana cherry date elderberry fig grape honey iris jasmine kiwi lemon mango nut olive peach quince raspberry strawberry tangerine umbrella violet watermelon xylem';
    const mockWordlist = ['apple', 'banana', 'cherry', 'date'];
    const mockProfiles: ProfileInfo[] = [
        { profileName: 'Test Profile 1', folderName: 'profile1' },
        { profileName: 'Test Profile 2', folderName: 'profile2' },
    ];

    beforeEach(() => {
        vi.clearAllMocks();
        localStorage.clear();
        (invoke as Mock).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
            if (cmd === 'list_profiles') return Promise.resolve(mockProfiles);
            if (cmd === 'get_bip39_wordlist') return Promise.resolve(mockWordlist);
            if (cmd === 'get_active_recovery_session') return Promise.resolve(null);
            if (cmd === 'validate_mnemonic') {
                const mnemonic = args?.mnemonic || '';
                if (mnemonic === mockSeed12 || mnemonic === mockSeed24) {
                    return Promise.resolve(undefined);
                }
                return Promise.reject(new Error('Invalid mnemonic'));
            }
            if (cmd === 'inspect_emergency_kit_uri') {
                return Promise.resolve({
                    phrase: mockSeed12,
                    wordCount: 12,
                    language: 'english',
                    hasBip39Passphrase: false,
                    userPrefix: null,
                    syncUrls: [],
                    syncDescriptorsJson: [],
                });
            }
            if (cmd === 'get_local_instance_id') return Promise.resolve('test-instance-id');
            if (cmd === 'recover_wallet_and_set_new_password') return Promise.resolve(undefined);
            return Promise.resolve(undefined);
        });
    });

    it('renders profile selector without preselecting when multiple profiles exist, and requires conscious choice', async () => {
        render(
            <WalletRecovery
                onRecoverySuccess={mockOnRecoverySuccess}
                onSwitchToLogin={mockOnSwitchToLogin}
            />
        );

        await waitFor(() => {
            expect(screen.getByTestId('profile-select')).toHaveValue('');
            expect(screen.getByTestId('wallet-recovery-choose-social')).toBeInTheDocument();
            expect(screen.getByTestId('wallet-recovery-choose-words')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('wallet-recovery-choose-words'));
        expect(screen.getByTestId('feedback-message')).toBeInTheDocument();

        fireEvent.change(screen.getByTestId('profile-select'), { target: { value: 'profile1' } });
        expect(screen.getByTestId('profile-select')).toHaveValue('profile1');

        fireEvent.click(screen.getByText('Back to Login'));
        expect(mockOnSwitchToLogin).toHaveBeenCalled();
    });

    it('switches to words mode via SeedPhraseInput without prefix field and resets password', async () => {
        render(
            <WalletRecovery
                onRecoverySuccess={mockOnRecoverySuccess}
                onSwitchToLogin={mockOnSwitchToLogin}
            />
        );

        await waitFor(() => {
            expect(screen.getByTestId('wallet-recovery-choose-words')).toBeInTheDocument();
        });

        fireEvent.change(screen.getByTestId('profile-select'), { target: { value: 'profile1' } });
        fireEvent.click(screen.getByTestId('wallet-recovery-choose-words'));

        expect(screen.getAllByTestId(/^recovery-word-word-/)).toHaveLength(12);
        // Subaccount prefix should not appear in local password reset
        fireEvent.click(screen.getByTestId('seed-advanced-toggle'));
        expect(screen.queryByTestId('seed-user-prefix-input')).not.toBeInTheDocument();

        fireEvent.change(screen.getByTestId('recovery-word-word-0'), {
            target: { value: mockSeed12 },
        });

        await waitFor(() => {
            expect(screen.getByTestId('seed-validation-status')).toHaveTextContent(
                'Seed phrase is valid.'
            );
        });

        fireEvent.change(screen.getByTestId('seed-passphrase-input'), {
            target: { value: 'mysecret' },
        });
        fireEvent.change(screen.getByTestId('recovery-new-password-input'), {
            target: { value: 'NewPassword123!' },
        });
        fireEvent.change(screen.getByTestId('recovery-confirm-password-input'), {
            target: { value: 'NewPassword123!' },
        });

        fireEvent.click(screen.getByTestId('recovery-submit-btn'));

        await waitFor(() => {
            expect(invoke).toHaveBeenCalledWith('recover_wallet_and_set_new_password', {
                folderName: 'profile1',
                mnemonic: mockSeed12,
                passphrase: 'mysecret',
                newPassword: 'NewPassword123!',
                localInstanceId: 'test-instance-id',
                language: 'english',
            });
            expect(mockOnRecoverySuccess).toHaveBeenCalled();
        });
    });

    it('supports 24-word seed phrase paste and humoco://kit initialPayload prefill', async () => {
        render(
            <WalletRecovery
                onRecoverySuccess={mockOnRecoverySuccess}
                onSwitchToLogin={mockOnSwitchToLogin}
                initialPayload="humoco://kit?v=1&ent=abcd"
            />
        );

        await waitFor(() => {
            expect(invoke).toHaveBeenCalledWith('inspect_emergency_kit_uri', {
                uri: 'humoco://kit?v=1&ent=abcd',
            });
            expect(screen.getByTestId('detected-kit-banner')).toBeInTheDocument();
        });

        // Also test 24-word paste
        fireEvent.click(screen.getByTestId('clear-detected-kit-btn'));
        fireEvent.change(screen.getByTestId('recovery-word-word-0'), {
            target: { value: mockSeed24 },
        });

        await waitFor(() => {
            expect(screen.getAllByTestId(/^recovery-word-word-/)).toHaveLength(24);
            expect(screen.getByTestId('recovery-word-word-23')).toHaveValue('xylem');
        });
    });

    it('validates mismatched and short passwords before backend call', async () => {
        render(
            <WalletRecovery
                onRecoverySuccess={mockOnRecoverySuccess}
                onSwitchToLogin={mockOnSwitchToLogin}
                defaultMethod="words"
            />
        );

        await waitFor(() => expect(screen.getByTestId('profile-select')).toBeInTheDocument());
        fireEvent.change(screen.getByTestId('profile-select'), { target: { value: 'profile1' } });

        fireEvent.change(screen.getByTestId('recovery-word-word-0'), {
            target: { value: mockSeed12 },
        });
        await waitFor(() =>
            expect(screen.getByTestId('recovery-submit-btn')).not.toBeDisabled()
        );

        fireEvent.change(screen.getByTestId('recovery-new-password-input'), {
            target: { value: 'password123' },
        });
        fireEvent.change(screen.getByTestId('recovery-confirm-password-input'), {
            target: { value: 'different123' },
        });
        fireEvent.click(screen.getByTestId('recovery-submit-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('feedback-message')).toHaveTextContent(
                /passwords do not match/i
            );
        });
    });

    it('switches to social recovery card cleanly', async () => {
        render(
            <WalletRecovery
                onRecoverySuccess={mockOnRecoverySuccess}
                onSwitchToLogin={mockOnSwitchToLogin}
            />
        );

        await waitFor(() =>
            expect(screen.getByTestId('wallet-recovery-choose-social')).toBeInTheDocument()
        );
        fireEvent.change(screen.getByTestId('profile-select'), { target: { value: 'profile1' } });
        fireEvent.click(screen.getByTestId('wallet-recovery-choose-social'));

        await waitFor(() => {
            expect(screen.getByTestId('start-social-recovery-btn')).toBeInTheDocument();
        });
    });

    it('restores targetFolderName from active reset_local_password session instead of defaulting to first profile', async () => {
        const baseImpl = (invoke as Mock).getMockImplementation();
        (invoke as Mock).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
            if (cmd === 'get_active_recovery_session') {
                return Promise.resolve({
                    sessionId: 'sess-p2',
                    sessionMode: 'reset_local_password',
                    targetFolderName: 'profile2',
                    sasCode: '123456',
                    thresholdM: 2,
                    collectedCount: 1,
                    questionIds: [1, 2, 3],
                    createdAtSecs: 1000,
                });
            }
            return baseImpl ? baseImpl(cmd, args) : Promise.resolve(undefined);
        });

        render(
            <WalletRecovery
                onRecoverySuccess={mockOnRecoverySuccess}
                onSwitchToLogin={mockOnSwitchToLogin}
            />
        );

        await waitFor(() => {
            expect(screen.getByTestId('profile-select')).toHaveValue('profile2');
            expect(screen.getByTestId('wallet-recovery-resume-banner')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('wallet-recovery-resume-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('profile-select')).toBeDisabled();
            expect(screen.getByTestId('profile-select')).toHaveValue('profile2');
        });
    });
});
