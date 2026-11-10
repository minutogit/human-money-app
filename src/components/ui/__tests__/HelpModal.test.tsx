import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { HelpModal } from '../HelpModal';
import enJson from '../../../locales/en.json';

const en = enJson as Record<string, string>;

describe('HelpModal Component', () => {
    it('renders topic details with category badge, short summary, and formatted long text', () => {
        render(
            <HelpModal
                isOpen={true}
                onClose={() => {}}
                topic="voucherBasics"
            />
        );

        const dialog = screen.getByRole('dialog');
        expect(dialog).toBeInTheDocument();

        expect(screen.getByText(en['help.voucherBasics.title'])).toBeInTheDocument();
        expect(screen.getByText(en['help.voucherBasics.short'])).toBeInTheDocument();
        expect(screen.getByText(en['helpCenter.categoryBasics'])).toBeInTheDocument();
    });

    it('allows switching topic via related topic chip', () => {
        render(
            <HelpModal
                isOpen={true}
                onClose={() => {}}
                topic="conceptParadigm"
            />
        );

        // conceptParadigm has related ['voucherBasics', 'benutzerId', 'conceptCommunity']
        const voucherBasicsTitle = en['help.voucherBasics.title'];
        const chip = screen.getByRole('button', { name: voucherBasicsTitle });
        expect(chip).toBeInTheDocument();

        // Click related topic chip
        fireEvent.click(chip);

        // Now voucherBasics is active
        expect(screen.getByText(voucherBasicsTitle)).toBeInTheDocument();
        expect(screen.getByText(en['help.voucherBasics.short'])).toBeInTheDocument();
    });

    it('triggers navigation and closes modal when clicking "Im Hilfe-Center öffnen"', () => {
        const onClose = vi.fn();
        const onNavigate = vi.fn();
        const dispatchSpy = vi.spyOn(window, 'dispatchEvent');

        render(
            <HelpModal
                isOpen={true}
                onClose={onClose}
                topic="wotNetwork"
                onNavigateToHelpCenter={onNavigate}
            />
        );

        const openBtn = screen.getByRole('button', {
            name: new RegExp(en['helpCenter.openInHelpCenter'], 'i'),
        });
        fireEvent.click(openBtn);

        expect(onClose).toHaveBeenCalled();
        expect(onNavigate).toHaveBeenCalledWith('wotNetwork');
        expect(dispatchSpy).toHaveBeenCalled();
        dispatchSpy.mockRestore();
    });

    it('closes on Escape key press', () => {
        const onClose = vi.fn();
        render(
            <HelpModal
                isOpen={true}
                onClose={onClose}
                topic="mnemonic"
            />
        );

        fireEvent.keyDown(window, { key: 'Escape' });
        expect(onClose).toHaveBeenCalled();
    });
});
