import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { QuickPinCard } from '../../settings/QuickPinCard';
import { RecoveryRehearsalModal } from '../RecoveryRehearsalModal';
import { authService } from '../../../services/authService';
import { recoveryService } from '../../../services/recoveryService';

const mockElevateSession = vi.fn().mockResolvedValue(undefined);
const mockRefreshSessionStatus = vi.fn().mockResolvedValue(undefined);
const mockProtectAction = vi.fn();

let mockAccessMode: 'full' | 'dailyPay' = 'full';
let mockOptionalSession: { protectAction: typeof mockProtectAction } | null = null;

vi.mock('../../../context/SessionContext', () => ({
    useSession: () => ({
        folderName: 'alice_folder',
        accessMode: mockAccessMode,
        elevateSession: mockElevateSession,
        refreshSessionStatus: mockRefreshSessionStatus,
    }),
    useOptionalSession: () => mockOptionalSession,
}));

vi.mock('../../../services/authService', () => ({
    authService: {
        getProfilePinStatus: vi.fn(),
        enableProfilePin: vi.fn(),
        disableProfilePin: vi.fn(),
    },
}));

vi.mock('../../../services/recoveryService', () => ({
    recoveryService: {
        verifyRecoveryRehearsal: vi.fn(),
        snoozeRecoveryRehearsal: vi.fn(),
    },
}));

describe('QuickPinCard & RecoveryRehearsalModal', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        mockAccessMode = 'full';
        mockOptionalSession = null;
        mockElevateSession.mockResolvedValue(undefined);
        mockRefreshSessionStatus.mockResolvedValue(undefined);
        vi.mocked(recoveryService.snoozeRecoveryRehearsal).mockResolvedValue(Date.now() + 86400);
    });

    it('QuickPinCard sanitizes non-digits, validates mismatch and backend errors, enables DailyPay PIN, and elevates non-full sessions', async () => {
        mockAccessMode = 'dailyPay';
        vi.mocked(authService.getProfilePinStatus).mockResolvedValue({
            enabled: false,
            scope: null,
            failedAttempts: 0,
            maxAttempts: 5,
            remainingAttempts: 5,
        });
        vi.mocked(authService.enableProfilePin)
            .mockRejectedValueOnce(new Error('Wrong master password'))
            .mockResolvedValueOnce({
                enabled: true,
                scope: 'dailyPay',
                failedAttempts: 0,
                maxAttempts: 5,
                remainingAttempts: 5,
            });

        render(<QuickPinCard />);

        const badge = await screen.findByTestId('quick-pin-status-badge');
        expect(badge).toHaveTextContent('Not configured');

        fireEvent.click(screen.getByTestId('quick-pin-configure-btn'));
        expect(screen.queryByTestId('pin-scope-receive-only')).not.toBeInTheDocument();

        const pinInput = screen.getByTestId('quick-pin-input');
        const confirmInput = screen.getByTestId('quick-pin-confirm-input');
        const masterInput = screen.getByTestId('quick-pin-master-password-input');
        const saveBtn = screen.getByTestId('quick-pin-save-btn');

        // Non-digit characters are stripped; < 6 digits keeps save disabled
        fireEvent.change(pinInput, { target: { value: '12ab345' } });
        expect(pinInput).toHaveValue('12345');
        fireEvent.change(confirmInput, { target: { value: '12345' } });
        fireEvent.change(masterInput, { target: { value: 'MasterPass123' } });
        expect(saveBtn).toBeDisabled();

        // Mismatch validation blocks service call
        fireEvent.change(pinInput, { target: { value: '123456' } });
        fireEvent.change(confirmInput, { target: { value: '654321' } });
        fireEvent.click(saveBtn);

        expect(await screen.findByText('The two PIN entries do not match.')).toBeInTheDocument();
        expect(authService.enableProfilePin).not.toHaveBeenCalled();

        // Backend error keeps status disabled
        fireEvent.change(confirmInput, { target: { value: '123456' } });
        fireEvent.click(saveBtn);
        expect(await screen.findByText('Wrong master password')).toBeInTheDocument();
        expect(screen.getByTestId('quick-pin-status-badge')).toHaveTextContent('Not configured');

        // Matching valid credentials save PIN, elevate non-full session, and update status badge
        fireEvent.click(saveBtn);

        await waitFor(() => {
            expect(authService.enableProfilePin).toHaveBeenCalledWith(
                'MasterPass123',
                '123456',
                'dailyPay',
                'alice_folder'
            );
            expect(mockElevateSession).toHaveBeenCalledWith('MasterPass123');
            expect(mockRefreshSessionStatus).toHaveBeenCalled();
        });
        expect(await screen.findByTestId('quick-pin-status-badge')).toHaveTextContent('Daily PIN');
        expect(
            screen.getByText('Quick PIN has been securely configured for this device.')
        ).toBeInTheDocument();
    });

    it('QuickPinCard keeps Daily PIN enabled when disable fails and removes it on valid master password', async () => {
        vi.mocked(authService.getProfilePinStatus).mockResolvedValue({
            enabled: true,
            scope: 'dailyPay',
            failedAttempts: 0,
            maxAttempts: 5,
            remainingAttempts: 5,
        });
        vi.mocked(authService.disableProfilePin)
            .mockRejectedValueOnce(new Error('Wrong master password'))
            .mockResolvedValueOnce(undefined);

        render(<QuickPinCard />);

        expect(await screen.findByTestId('quick-pin-status-badge')).toHaveTextContent('Daily PIN');

        fireEvent.click(screen.getByTestId('quick-pin-disable-open-btn'));
        fireEvent.change(screen.getByTestId('quick-pin-disable-password-input'), {
            target: { value: 'WrongPass' },
        });
        fireEvent.click(screen.getByTestId('quick-pin-disable-confirm-btn'));

        expect(await screen.findByText('Wrong master password')).toBeInTheDocument();
        expect(screen.getByTestId('quick-pin-status-badge')).toHaveTextContent('Daily PIN');

        fireEvent.change(screen.getByTestId('quick-pin-disable-password-input'), {
            target: { value: 'MasterPass123' },
        });
        fireEvent.click(screen.getByTestId('quick-pin-disable-confirm-btn'));

        await waitFor(() => {
            expect(authService.disableProfilePin).toHaveBeenCalledWith(
                'MasterPass123',
                'alice_folder'
            );
            expect(mockRefreshSessionStatus).toHaveBeenCalled();
        });
        expect(await screen.findByTestId('quick-pin-status-badge')).toHaveTextContent(
            'Not configured'
        );
        expect(
            screen.getByText('Quick PIN removed. Sign-in now requires your master password.')
        ).toBeInTheDocument();
    });

    it('RecoveryRehearsalModal requires all 3 answers, sends exact answers, shows mismatch badges, and verifies full match', async () => {
        const onClose = vi.fn();
        const onVerified = vi.fn();
        const onResetQuestions = vi.fn();

        render(
            <RecoveryRehearsalModal
                isOpen={true}
                questionIds={[1, 7, 12]}
                onClose={onClose}
                onVerified={onVerified}
                onResetQuestions={onResetQuestions}
            />
        );

        expect(screen.getByText(/First true love/i)).toBeInTheDocument();
        expect(screen.getByText(/Personal passphrase/i)).toBeInTheDocument();

        const submitBtn = screen.getByRole('button', { name: 'Verify Answers' });
        expect(submitBtn).toBeDisabled();

        const inputs = screen.getAllByPlaceholderText('Enter your answer');
        expect(inputs).toHaveLength(3);

        // Whitespace-only answers keep submit disabled
        fireEvent.change(inputs[0], { target: { value: 'OakStreet' } });
        fireEvent.change(inputs[1], { target: { value: '   ' } });
        fireEvent.change(inputs[2], { target: { value: 'BlueSky42' } });
        expect(submitBtn).toBeDisabled();

        fireEvent.change(inputs[1], { target: { value: 'WrongLove' } });
        expect(submitBtn).not.toBeDisabled();

        vi.mocked(recoveryService.verifyRecoveryRehearsal).mockResolvedValueOnce({
            allCorrect: false,
            questionMatches: [true, false, true],
            rehearsalStage: 0,
            nextRehearsalAt: 1000,
        });

        fireEvent.click(submitBtn);

        await waitFor(() => {
            expect(recoveryService.verifyRecoveryRehearsal).toHaveBeenCalledWith([
                'OakStreet',
                'WrongLove',
                'BlueSky42',
            ]);
        });
        expect(await screen.findByText('Mismatch')).toBeInTheDocument();
        expect(screen.getAllByText('Match')).toHaveLength(2);
        expect(screen.getByText('Reset security questions now')).toBeInTheDocument();
        expect(onVerified).not.toHaveBeenCalled();

        // Fixing mismatched answer succeeds and calls onVerified
        fireEvent.change(inputs[1], { target: { value: 'Anna' } });
        vi.mocked(recoveryService.verifyRecoveryRehearsal).mockResolvedValueOnce({
            allCorrect: true,
            questionMatches: [true, true, true],
            rehearsalStage: 1,
            nextRehearsalAt: 2000,
        });

        fireEvent.click(screen.getByRole('button', { name: 'Verify Answers' }));

        await waitFor(() => {
            expect(recoveryService.verifyRecoveryRehearsal).toHaveBeenLastCalledWith([
                'OakStreet',
                'Anna',
                'BlueSky42',
            ]);
            expect(onVerified).toHaveBeenCalledTimes(1);
        });
        expect(await screen.findByText('Great! All 3 answers verified')).toBeInTheDocument();
    });

    it('RecoveryRehearsalModal invokes onClose and onResetQuestions from partial mismatch reset action', async () => {
        const onClose = vi.fn();
        const onVerified = vi.fn();
        const onResetQuestions = vi.fn();

        vi.mocked(recoveryService.verifyRecoveryRehearsal).mockResolvedValueOnce({
            allCorrect: false,
            questionMatches: [false, true, true],
            rehearsalStage: 0,
            nextRehearsalAt: 1000,
        });

        render(
            <RecoveryRehearsalModal
                isOpen={true}
                questionIds={[1, 7, 12]}
                onClose={onClose}
                onVerified={onVerified}
                onResetQuestions={onResetQuestions}
            />
        );

        const inputs = screen.getAllByPlaceholderText('Enter your answer');
        fireEvent.change(inputs[0], { target: { value: 'WrongStreet' } });
        fireEvent.change(inputs[1], { target: { value: 'Anna' } });
        fireEvent.change(inputs[2], { target: { value: 'BlueSky42' } });
        fireEvent.click(screen.getByRole('button', { name: 'Verify Answers' }));

        const resetLink = await screen.findByText('Reset security questions now');
        fireEvent.click(resetLink);

        expect(onClose).toHaveBeenCalledTimes(1);
        expect(onResetQuestions).toHaveBeenCalledTimes(1);
        expect(onVerified).not.toHaveBeenCalled();
    });

    it('RecoveryRehearsalModal snoozes rehearsal for 24h and notifies callbacks', async () => {
        const onClose = vi.fn();
        const onVerified = vi.fn();
        const onResetQuestions = vi.fn();

        render(
            <RecoveryRehearsalModal
                isOpen={true}
                questionIds={[1, 7, 12]}
                onClose={onClose}
                onVerified={onVerified}
                onResetQuestions={onResetQuestions}
            />
        );

        fireEvent.click(screen.getByRole('button', { name: 'Remind me later' }));

        await waitFor(() => {
            expect(recoveryService.snoozeRecoveryRehearsal).toHaveBeenCalledWith(24);
            expect(onVerified).toHaveBeenCalledTimes(1);
            expect(onClose).toHaveBeenCalledTimes(1);
        });
    });

    it('RecoveryRehearsalModal retries verifyRecoveryRehearsal via session.protectAction when session is locked', async () => {
        const onClose = vi.fn();
        const onVerified = vi.fn();
        const onResetQuestions = vi.fn();

        mockProtectAction.mockImplementation(
            async (fn: (pwd?: string) => Promise<unknown>) => await fn('UnlockedMasterPw')
        );
        mockOptionalSession = { protectAction: mockProtectAction };

        vi.mocked(recoveryService.verifyRecoveryRehearsal)
            .mockRejectedValueOnce({
                code: 'error.auth.sessionLocked',
                message: 'Session locked',
            })
            .mockResolvedValueOnce({
                allCorrect: true,
                questionMatches: [true, true, true],
                rehearsalStage: 1,
                nextRehearsalAt: 2000,
            });

        render(
            <RecoveryRehearsalModal
                isOpen={true}
                questionIds={[1, 7, 12]}
                onClose={onClose}
                onVerified={onVerified}
                onResetQuestions={onResetQuestions}
            />
        );

        const inputs = screen.getAllByPlaceholderText('Enter your answer');
        fireEvent.change(inputs[0], { target: { value: 'OakStreet' } });
        fireEvent.change(inputs[1], { target: { value: 'Anna' } });
        fireEvent.change(inputs[2], { target: { value: 'BlueSky42' } });
        fireEvent.click(screen.getByRole('button', { name: 'Verify Answers' }));

        await waitFor(() => {
            expect(mockProtectAction).toHaveBeenCalledTimes(1);
            expect(recoveryService.verifyRecoveryRehearsal).toHaveBeenNthCalledWith(1, [
                'OakStreet',
                'Anna',
                'BlueSky42',
            ]);
            expect(recoveryService.verifyRecoveryRehearsal).toHaveBeenNthCalledWith(
                2,
                ['OakStreet', 'Anna', 'BlueSky42'],
                'UnlockedMasterPw'
            );
            expect(onVerified).toHaveBeenCalledTimes(1);
        });
        expect(await screen.findByText('Great! All 3 answers verified')).toBeInTheDocument();
    });
});
