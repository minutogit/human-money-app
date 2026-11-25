import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { FirstStepsChecklist } from '../FirstStepsChecklist';

const mockNavigate = vi.fn();

vi.mock('../../../context/NavigationContext', () => ({
    useNavigation: () => ({
        navigate: mockNavigate,
    }),
}));

const translations: Record<string, string> = {
    'dashboard.firstStepsTitle': 'Erste Schritte',
    'dashboard.stepsProgress': '{{completed}} von {{total}} erledigt',
};

vi.mock('react-i18next', () => ({
    useTranslation: () => ({
        t: (key: string, defaultOrOptions?: string | Record<string, unknown>, options?: Record<string, unknown>) => {
            const opts = (typeof defaultOrOptions === 'object' ? defaultOrOptions : options) || {};
            let text = typeof defaultOrOptions === 'string' ? defaultOrOptions : (translations[key] || key);
            for (const [k, v] of Object.entries(opts)) {
                text = text.replace(new RegExp(`{{${k}}}`, 'g'), String(v));
            }
            return text;
        },
    }),
}));

describe('FirstStepsChecklist Component', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        localStorage.clear();
    });

    it('renders checklist when some steps are uncompleted', () => {
        render(
            <FirstStepsChecklist 
                hasBackup={false}
                hasContact={true}
                hasVoucher={false}
                hasSync={false}
                userId="did:key:user1"
            />
        );

        expect(screen.getByTestId('first-steps-checklist')).toBeInTheDocument();
        expect(screen.getByText('Erste Schritte')).toBeInTheDocument();
        expect(screen.getByText('1 von 4 erledigt')).toBeInTheDocument();

        // Check steps
        expect(screen.getByTestId('checklist-step-backup')).toBeInTheDocument();
        expect(screen.getByTestId('checklist-step-contact')).toBeInTheDocument();
        expect(screen.getByTestId('checklist-step-voucher')).toBeInTheDocument();
        expect(screen.getByTestId('checklist-step-sync')).toBeInTheDocument();
    });

    it('navigates when clicking a step', () => {
        render(
            <FirstStepsChecklist 
                hasBackup={false}
                hasContact={false}
                hasVoucher={false}
                hasSync={false}
                userId="did:key:user1"
            />
        );

        fireEvent.click(screen.getByTestId('checklist-step-backup'));
        expect(mockNavigate).toHaveBeenCalledWith({ view: 'settings', initialSection: 'backup' });

        fireEvent.click(screen.getByTestId('checklist-step-contact'));
        expect(mockNavigate).toHaveBeenCalledWith({ view: 'address_book' });

        fireEvent.click(screen.getByTestId('checklist-step-voucher'));
        expect(mockNavigate).toHaveBeenCalledWith({ view: 'create_voucher' });

        fireEvent.click(screen.getByTestId('checklist-step-sync'));
        expect(mockNavigate).toHaveBeenCalledWith({ view: 'settings', initialSection: 'sync' });
    });

    it('is dismissed when clicking the X button and persists in localStorage', () => {
        const onDismiss = vi.fn();
        const { unmount } = render(
            <FirstStepsChecklist 
                hasBackup={false}
                hasContact={false}
                hasVoucher={false}
                hasSync={false}
                userId="did:key:user1"
                onDismiss={onDismiss}
            />
        );

        expect(screen.getByTestId('first-steps-checklist')).toBeInTheDocument();
        fireEvent.click(screen.getByTestId('dismiss-checklist-button'));

        expect(onDismiss).toHaveBeenCalled();
        expect(localStorage.getItem('humoco_first_steps_dismissed_did:key:user1')).toBe('true');
        expect(screen.queryByTestId('first-steps-checklist')).not.toBeInTheDocument();

        // On fresh mount, useState initializer reads localStorage and keeps checklist hidden
        unmount();
        render(
            <FirstStepsChecklist 
                hasBackup={false}
                hasContact={false}
                hasVoucher={false}
                hasSync={false}
                userId="did:key:user1"
            />
        );
        expect(screen.queryByTestId('first-steps-checklist')).not.toBeInTheDocument();
    });

    it('is hidden automatically when all steps are completed', () => {
        render(
            <FirstStepsChecklist 
                hasBackup={true}
                hasContact={true}
                hasVoucher={true}
                hasSync={true}
                userId="did:key:user1"
            />
        );

        expect(screen.queryByTestId('first-steps-checklist')).not.toBeInTheDocument();
    });

    it('keeps backup step incomplete when hasIdentityRecovery is false and routes click to security section', () => {
        render(
            <FirstStepsChecklist
                hasBackup={true}
                hasContact={true}
                hasVoucher={true}
                hasSync={true}
                hasIdentityRecovery={false}
                userId="did:key:user1"
            />
        );

        expect(screen.getByTestId('first-steps-checklist')).toBeInTheDocument();
        expect(screen.getByText('3 von 4 erledigt')).toBeInTheDocument();

        fireEvent.click(screen.getByTestId('checklist-step-backup'));
        expect(mockNavigate).toHaveBeenCalledWith({ view: 'settings', initialSection: 'security' });
    });

    it('isolates dismissal state per userId and falls back to profileScope', () => {
        const first = render(
            <FirstStepsChecklist
                hasBackup={false}
                hasContact={false}
                hasVoucher={false}
                hasSync={false}
                userId="did:key:user1"
            />
        );
        fireEvent.click(screen.getByTestId('dismiss-checklist-button'));
        expect(screen.queryByTestId('first-steps-checklist')).not.toBeInTheDocument();
        first.unmount();

        const second = render(
            <FirstStepsChecklist
                hasBackup={false}
                hasContact={false}
                hasVoucher={false}
                hasSync={false}
                userId="did:key:user2"
            />
        );
        expect(screen.getByTestId('first-steps-checklist')).toBeInTheDocument();
        second.unmount();

        render(
            <FirstStepsChecklist
                hasBackup={false}
                hasContact={false}
                hasVoucher={false}
                hasSync={false}
                profileScope="profile-a"
            />
        );
        fireEvent.click(screen.getByTestId('dismiss-checklist-button'));
        expect(localStorage.getItem('humoco_first_steps_dismissed_profile-a')).toBe('true');
    });

    it('activates checklist steps via Enter and Space keys', () => {
        render(
            <FirstStepsChecklist
                hasBackup={false}
                hasContact={false}
                hasVoucher={false}
                hasSync={false}
                userId="did:key:user1"
            />
        );

        fireEvent.keyDown(screen.getByTestId('checklist-step-contact'), { key: 'Enter' });
        expect(mockNavigate).toHaveBeenCalledWith({ view: 'address_book' });

        fireEvent.keyDown(screen.getByTestId('checklist-step-sync'), { key: ' ' });
        expect(mockNavigate).toHaveBeenCalledWith({ view: 'settings', initialSection: 'sync' });
    });
});
