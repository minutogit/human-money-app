import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { QuickPinCard } from '../../settings/QuickPinCard';
import { RecoveryRehearsalModal } from '../RecoveryRehearsalModal';
import { authService } from '../../../services/authService';
import { recoveryService } from '../../../services/recoveryService';

const mockElevateSession = vi.fn().mockResolvedValue(undefined);

vi.mock('../../../context/SessionContext', () => ({
    useSession: () => ({
        folderName: 'alice_folder',
        accessMode: 'full',
        elevateSession: mockElevateSession,
    }),
    useOptionalSession: () => null,
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
        vi.mocked(recoveryService.snoozeRecoveryRehearsal).mockResolvedValue(Date.now() + 86400);
    });

    it('QuickPinCard shows disabled status initially, validates PIN mismatch, enables Cashier Mode, and disables PIN', async () => {
        vi.mocked(authService.getProfilePinStatus).mockResolvedValue({
            enabled: false,
            scope: null,
            failedAttempts: 0,
            maxAttempts: 5,
            remainingAttempts: 5,
        });
        vi.mocked(authService.enableProfilePin).mockResolvedValue({
            enabled: true,
            scope: 'receiveOnly',
            failedAttempts: 0,
            maxAttempts: 5,
            remainingAttempts: 5,
        });
        vi.mocked(authService.disableProfilePin).mockResolvedValue(undefined);

        render(<QuickPinCard />);

        const badge = await screen.findByTestId('quick-pin-status-badge');
        expect(badge).toHaveTextContent('Not configured');

        fireEvent.click(screen.getByTestId('quick-pin-configure-btn'));
        fireEvent.click(screen.getByTestId('pin-scope-receive-only'));

        // Mismatch validation
        fireEvent.change(screen.getByTestId('quick-pin-input'), { target: { value: '123456' } });
        fireEvent.change(screen.getByTestId('quick-pin-confirm-input'), { target: { value: '654321' } });
        fireEvent.change(screen.getByTestId('quick-pin-master-password-input'), { target: { value: 'MasterPass123' } });
        fireEvent.click(screen.getByTestId('quick-pin-save-btn'));

        expect(await screen.findByText('The two PIN entries do not match.')).toBeInTheDocument();
        expect(authService.enableProfilePin).not.toHaveBeenCalled();

        // Matching PIN saves and updates status badge to Cashier Mode
        fireEvent.change(screen.getByTestId('quick-pin-confirm-input'), { target: { value: '123456' } });
        fireEvent.click(screen.getByTestId('quick-pin-save-btn'));

        await waitFor(() => {
            expect(authService.enableProfilePin).toHaveBeenCalledWith(
                'MasterPass123',
                '123456',
                'receiveOnly',
                'alice_folder'
            );
        });
        expect(await screen.findByTestId('quick-pin-status-badge')).toHaveTextContent('Cashier Mode');

        // Disable active PIN
        fireEvent.click(screen.getByTestId('quick-pin-disable-open-btn'));
        fireEvent.change(screen.getByTestId('quick-pin-disable-password-input'), { target: { value: 'MasterPass123' } });
        fireEvent.click(screen.getByTestId('quick-pin-disable-confirm-btn'));

        await waitFor(() => {
            expect(authService.disableProfilePin).toHaveBeenCalledWith('MasterPass123', 'alice_folder');
        });
        expect(await screen.findByTestId('quick-pin-status-badge')).toHaveTextContent('Not configured');
    });

    it('RecoveryRehearsalModal renders 3 configured questions, displays partial mismatches, verifies full match, and handles snooze', async () => {
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

        const inputs = screen.getAllByPlaceholderText('Enter your answer');
        expect(inputs).toHaveLength(3);

        fireEvent.change(inputs[0], { target: { value: 'OakStreet' } });
        fireEvent.change(inputs[1], { target: { value: 'WrongLove' } });
        fireEvent.change(inputs[2], { target: { value: 'BlueSky42' } });

        vi.mocked(recoveryService.verifyRecoveryRehearsal).mockResolvedValueOnce({
            allCorrect: false,
            questionMatches: [true, false, true],
            rehearsalStage: 0,
            nextRehearsalAt: 1000,
        });

        fireEvent.click(screen.getByRole('button', { name: 'Verify Answers' }));

        expect(await screen.findByText('Mismatch')).toBeInTheDocument();
        expect(screen.getAllByText('Match')).toHaveLength(2);
        expect(screen.getByText('Reset security questions now')).toBeInTheDocument();
        expect(onVerified).not.toHaveBeenCalled();

        // Correct answer succeeds and calls onVerified
        fireEvent.change(inputs[1], { target: { value: 'Anna' } });
        vi.mocked(recoveryService.verifyRecoveryRehearsal).mockResolvedValueOnce({
            allCorrect: true,
            questionMatches: [true, true, true],
            rehearsalStage: 1,
            nextRehearsalAt: 2000,
        });

        fireEvent.click(screen.getByRole('button', { name: 'Verify Answers' }));

        await waitFor(() => {
            expect(onVerified).toHaveBeenCalledTimes(1);
        });
        expect(await screen.findByText('Great! All 3 answers verified')).toBeInTheDocument();

        // Re-render for snooze test
        vi.clearAllMocks();
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
            expect(onClose).toHaveBeenCalled();
        });
    });
});
