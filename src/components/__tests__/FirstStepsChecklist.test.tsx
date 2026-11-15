import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { FirstStepsChecklist } from '../FirstStepsChecklist';

const mockNavigate = vi.fn();

vi.mock('../../context/NavigationContext', () => ({
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
        const { rerender } = render(
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

        // On remount / rerender, remains hidden
        rerender(
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
});
