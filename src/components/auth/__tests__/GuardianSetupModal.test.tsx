import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { GuardianSetupModal } from '../GuardianSetupModal';
import { contactService } from '../../../services/contactService';
import { recoveryService } from '../../../services/recoveryService';
import { Contact } from '../../../types';

vi.mock('../../../services/contactService', () => ({
    contactService: {
        getContacts: vi.fn(),
    },
}));

vi.mock('../../../services/recoveryService', () => ({
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
            expect(mockOnSuccess).toHaveBeenCalledTimes(1);
            expect(mockOnClose).toHaveBeenCalledTimes(1);
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

    it('deduplicates sibling prefix contacts sharing the same root key', async () => {
        const withSibling: Contact[] = [
            {
                did: 'laden:abc@did:key:z6MkuContact1',
                profile: { id: 'laden:abc@did:key:z6MkuContact1', firstName: 'Alice', lastName: 'Laden' },
                tags: [],
                trustLevel: 2,
                addedAt: '2026-01-01',
            },
            {
                did: 'privat:def@did:key:z6MkuContact1',
                profile: { id: 'privat:def@did:key:z6MkuContact1', firstName: 'Alice', lastName: 'Privat' },
                tags: [],
                trustLevel: 2,
                addedAt: '2026-01-01',
            },
            {
                did: 'did:key:z6MkuContact2',
                profile: { id: 'did:key:z6MkuContact2', firstName: 'Bob', lastName: 'Jones' },
                tags: [],
                trustLevel: 2,
                addedAt: '2026-01-01',
            },
            {
                did: 'did:key:z6MkuContact3',
                profile: { id: 'did:key:z6MkuContact3', firstName: 'Carol', lastName: 'Williams' },
                tags: [],
                trustLevel: 3,
                addedAt: '2026-01-01',
            },
        ];
        vi.mocked(contactService.getContacts).mockResolvedValue(withSibling);

        render(
            <GuardianSetupModal
                isOpen={true}
                onClose={mockOnClose}
                onSuccess={mockOnSuccess}
            />
        );

        await screen.findByText('Bob Jones');
        expect(screen.getByTestId('select-contact-laden:abc@did:key:z6MkuContact1')).toBeInTheDocument();
        expect(screen.queryByTestId('select-contact-privat:def@did:key:z6MkuContact1')).not.toBeInTheDocument();
        expect(screen.getByTestId('select-contact-did:key:z6MkuContact2')).toBeInTheDocument();
        expect(screen.getByTestId('select-contact-did:key:z6MkuContact3')).toBeInTheDocument();
        expect(screen.getByText('3 / 5')).toBeInTheDocument();
    });

    it('filters out trustLevel 0 contacts, shows min-contacts warning, and disables submit when fewer than 3 trusted contacts exist', async () => {
        const twoTrustedPlusUntrusted: Contact[] = [
            ...mockEligibleContacts.slice(0, 2),
            {
                did: 'did:key:z6MkuUntrusted',
                profile: { id: '4', firstName: 'Untrusted', lastName: 'Person' },
                tags: [],
                trustLevel: 0,
                addedAt: '2026-01-01',
            },
        ];
        vi.mocked(contactService.getContacts).mockResolvedValue(twoTrustedPlusUntrusted);

        render(
            <GuardianSetupModal
                isOpen={true}
                onClose={mockOnClose}
                onSuccess={mockOnSuccess}
            />
        );

        await screen.findByText('Alice Smith');

        expect(screen.queryByText('Untrusted Person')).toBeNull();
        expect(screen.getByTestId('guardian-min-contacts-hint')).toHaveTextContent(
            "At least 3 contacts with trust level 'Acquainted' or higher are required (currently available: 2)."
        );

        fireEvent.click(screen.getByTestId('select-contact-did:key:z6MkuContact1'));
        fireEvent.click(screen.getByTestId('select-contact-did:key:z6MkuContact2'));
        fireEvent.change(screen.getByTestId('guardian-answer-0'), { target: { value: 'ValidAnswerOne' } });
        fireEvent.change(screen.getByTestId('guardian-answer-1'), { target: { value: 'ValidAnswerTwo' } });
        fireEvent.change(screen.getByTestId('guardian-answer-2'), { target: { value: 'ValidAnswerThree' } });
        fireEvent.change(screen.getByTestId('guardian-password-input'), {
            target: { value: 'CorrectPassword123' },
        });

        expect(screen.getByTestId('guardian-setup-submit-btn')).toBeDisabled();
        expect(recoveryService.setupSocialRecoveryGuardians).not.toHaveBeenCalled();
    });

    it('enforces answer length rules and password requirement before enabling submit', async () => {
        render(
            <GuardianSetupModal
                isOpen={true}
                onClose={mockOnClose}
                onSuccess={mockOnSuccess}
            />
        );

        await screen.findByText('Alice Smith');

        fireEvent.change(screen.getByTestId('guardian-answer-0'), { target: { value: 'ab' } });
        fireEvent.change(screen.getByTestId('guardian-answer-1'), { target: { value: 'cd' } });
        fireEvent.change(screen.getByTestId('guardian-answer-2'), { target: { value: 'ef' } });
        fireEvent.change(screen.getByTestId('guardian-password-input'), {
            target: { value: 'CorrectPassword123' },
        });

        expect(
            screen.getByText(
                'All 3 answers must have at least 2 characters each and at least 12 characters combined.'
            )
        ).toBeInTheDocument();
        expect(screen.getByTestId('guardian-setup-submit-btn')).toBeDisabled();

        // Single-char individual answer still fails even when combined length >= 12
        fireEvent.change(screen.getByTestId('guardian-answer-0'), { target: { value: 'a' } });
        fireEvent.change(screen.getByTestId('guardian-answer-1'), { target: { value: 'LongAnswerOne' } });
        fireEvent.change(screen.getByTestId('guardian-answer-2'), { target: { value: 'LongAnswerTwo' } });
        expect(
            screen.getByText(
                'All 3 answers must have at least 2 characters each and at least 12 characters combined.'
            )
        ).toBeInTheDocument();
        expect(screen.getByTestId('guardian-setup-submit-btn')).toBeDisabled();

        // Fix answer 0, clear password -> still disabled until password provided
        fireEvent.change(screen.getByTestId('guardian-answer-0'), { target: { value: 'ab' } });
        fireEvent.change(screen.getByTestId('guardian-password-input'), { target: { value: '' } });
        expect(
            screen.queryByText(
                'All 3 answers must have at least 2 characters each and at least 12 characters combined.'
            )
        ).toBeNull();
        expect(screen.getByTestId('guardian-setup-submit-btn')).toBeDisabled();

        // Positive baseline: restore password -> submit enabled
        fireEvent.change(screen.getByTestId('guardian-password-input'), {
            target: { value: 'CorrectPassword123' },
        });
        expect(screen.getByTestId('guardian-setup-submit-btn')).not.toBeDisabled();
    });

    it('keeps modal open with delivery warning when guardian sync reaches zero servers and closes on acknowledgement', async () => {
        vi.mocked(recoveryService.setupSocialRecoveryGuardians).mockResolvedValue({
            rotationEpoch: 1,
            thresholdM: 2,
            guardiansCount: 3,
            postSyncResult: { uploadedTo: 0, errors: [] },
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

        fireEvent.change(screen.getByTestId('guardian-answer-0'), { target: { value: 'ValidAnswerOne' } });
        fireEvent.change(screen.getByTestId('guardian-answer-1'), { target: { value: 'ValidAnswerTwo' } });
        fireEvent.change(screen.getByTestId('guardian-answer-2'), { target: { value: 'ValidAnswerThree' } });
        fireEvent.change(screen.getByTestId('guardian-password-input'), {
            target: { value: 'CorrectPassword123' },
        });
        fireEvent.click(screen.getByTestId('guardian-setup-submit-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('guardian-setup-unreachable-warning')).toHaveTextContent(
                'none of their sync services were reachable'
            );
        });

        expect(mockOnSuccess).toHaveBeenCalledTimes(1);
        expect(mockOnClose).not.toHaveBeenCalled();

        fireEvent.click(screen.getByRole('button', { name: /Understood|Verstanden/i }));
        expect(mockOnClose).toHaveBeenCalledTimes(1);
    });

    it('displays backend error and stays open without calling onSuccess or onClose when setupSocialRecoveryGuardians rejects', async () => {
        vi.mocked(recoveryService.setupSocialRecoveryGuardians).mockRejectedValue(
            new Error('Invalid wallet password')
        );

        render(
            <GuardianSetupModal
                isOpen={true}
                onClose={mockOnClose}
                onSuccess={mockOnSuccess}
            />
        );

        await screen.findByText('Alice Smith');

        fireEvent.change(screen.getByTestId('guardian-answer-0'), { target: { value: 'ValidAnswerOne' } });
        fireEvent.change(screen.getByTestId('guardian-answer-1'), { target: { value: 'ValidAnswerTwo' } });
        fireEvent.change(screen.getByTestId('guardian-answer-2'), { target: { value: 'ValidAnswerThree' } });
        fireEvent.change(screen.getByTestId('guardian-password-input'), {
            target: { value: 'WrongPassword' },
        });
        fireEvent.click(screen.getByTestId('guardian-setup-submit-btn'));

        await waitFor(() => {
            expect(screen.getByText('Invalid wallet password')).toBeInTheDocument();
        });

        expect(screen.getByTestId('guardian-setup-submit-btn')).toBeInTheDocument();
        expect(mockOnSuccess).not.toHaveBeenCalled();
        expect(mockOnClose).not.toHaveBeenCalled();
    });
});
