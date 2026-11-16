import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { GuardianSetupModal } from '../GuardianSetupModal';
import { contactService } from '../../services/contactService';
import { recoveryService } from '../../services/recoveryService';
import { Contact } from '../../types';

vi.mock('../../services/contactService', () => ({
    contactService: {
        getContacts: vi.fn(),
    },
}));

vi.mock('../../services/recoveryService', () => ({
    recoveryService: {
        setupSocialRecoveryGuardians: vi.fn(),
    },
}));

describe('GuardianSetupModal Component', () => {
    const mockOnClose = vi.fn();
    const mockOnSuccess = vi.fn();

    const mockEligibleContacts: Contact[] = [
        {
            did: 'did:key:z6MkuContact1',
            profile: { id: '1', firstName: 'Alice', lastName: 'Smith' },
            tags: [],
            trustLevel: 2,
            addedAt: '2026-01-01',
        },
        {
            did: 'did:key:z6MkuContact2',
            profile: { id: '2', firstName: 'Bob', lastName: 'Jones' },
            tags: [],
            trustLevel: 1,
            addedAt: '2026-01-01',
        },
        {
            did: 'did:key:z6MkuContact3',
            profile: { id: '3', firstName: 'Carol', lastName: 'Williams' },
            tags: [],
            trustLevel: 3,
            addedAt: '2026-01-01',
        },
    ];

    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(contactService.getContacts).mockResolvedValue(mockEligibleContacts);
    });

    it('renders modal with contact selection and questions when open', async () => {
        render(
            <GuardianSetupModal
                isOpen={true}
                onClose={mockOnClose}
                onSuccess={mockOnSuccess}
            />
        );

        await waitFor(() => {
            expect(screen.getByText('Alice Smith')).toBeInTheDocument();
            expect(screen.getByText('Bob Jones')).toBeInTheDocument();
            expect(screen.getByText('Carol Williams')).toBeInTheDocument();
        });
    });

    it('successfully calls setupSocialRecoveryGuardians when valid inputs provided', async () => {
        vi.mocked(recoveryService.setupSocialRecoveryGuardians).mockResolvedValue({
            rotationEpoch: 1,
            thresholdM: 2,
            guardiansCount: 3,
            postSyncResult: null,
            backupSyncResult: null,
        });

        render(
            <GuardianSetupModal
                isOpen={true}
                onClose={mockOnClose}
                onSuccess={mockOnSuccess}
            />
        );

        await screen.findByText('Alice Smith');

        // Initial selection automatically selected 3 eligible contacts
        // Enter answers (total length >= 12, each >= 2)
        fireEvent.change(screen.getByTestId('guardian-answer-0'), { target: { value: 'ValidAnswerOne' } });
        fireEvent.change(screen.getByTestId('guardian-answer-1'), { target: { value: 'ValidAnswerTwo' } });
        fireEvent.change(screen.getByTestId('guardian-answer-2'), { target: { value: 'ValidAnswerThree' } });

        // Enter password
        fireEvent.change(screen.getByTestId('guardian-password-input'), { target: { value: 'CorrectPassword123' } });

        const submitBtn = screen.getByTestId('guardian-setup-submit-btn');
        expect(submitBtn).not.toBeDisabled();
        fireEvent.click(submitBtn);

        await waitFor(() => {
            expect(recoveryService.setupSocialRecoveryGuardians).toHaveBeenCalledWith({
                password: 'CorrectPassword123',
                guardianDids: ['did:key:z6MkuContact1', 'did:key:z6MkuContact2', 'did:key:z6MkuContact3'],
                thresholdM: 2,
                questionIds: [1, 2, 3],
                answers: ['ValidAnswerOne', 'ValidAnswerTwo', 'ValidAnswerThree'],
            });
        });
    });

    it('renders all 12 recovery questions and allows selecting IDs 7, 10, 12', async () => {
        vi.mocked(recoveryService.setupSocialRecoveryGuardians).mockResolvedValue({
            rotationEpoch: 1,
            thresholdM: 2,
            guardiansCount: 3,
            postSyncResult: null,
            backupSyncResult: null,
        });

        render(
            <GuardianSetupModal
                isOpen={true}
                onClose={mockOnClose}
                onSuccess={mockOnSuccess}
            />
        );

        await screen.findByText('Alice Smith');

        const selects = screen.getAllByRole('combobox') as HTMLSelectElement[];
        const questionSelects = selects.filter((s) => s.querySelectorAll('option').length === 12);
        expect(questionSelects).toHaveLength(3);

        fireEvent.change(questionSelects[0], { target: { value: '7' } });
        fireEvent.change(questionSelects[1], { target: { value: '10' } });
        fireEvent.change(questionSelects[2], { target: { value: '12' } });

        fireEvent.change(screen.getByTestId('guardian-answer-0'), { target: { value: 'FirstLoveName' } });
        fireEvent.change(screen.getByTestId('guardian-answer-1'), { target: { value: 'Nokia3310Phone' } });
        fireEvent.change(screen.getByTestId('guardian-answer-2'), { target: { value: 'CorrectHorseBattery' } });
        fireEvent.change(screen.getByTestId('guardian-password-input'), { target: { value: 'CorrectPassword123' } });

        fireEvent.click(screen.getByTestId('guardian-setup-submit-btn'));

        await waitFor(() => {
            expect(recoveryService.setupSocialRecoveryGuardians).toHaveBeenCalledWith({
                password: 'CorrectPassword123',
                guardianDids: ['did:key:z6MkuContact1', 'did:key:z6MkuContact2', 'did:key:z6MkuContact3'],
                thresholdM: 2,
                questionIds: [7, 10, 12],
                answers: ['FirstLoveName', 'Nokia3310Phone', 'CorrectHorseBattery'],
            });
        });
    });
});
