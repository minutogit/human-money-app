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
        expect(screen.getByText(en['helpCenter.groupFirstSteps'])).toBeInTheDocument();
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
        try {
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

            expect(onClose).toHaveBeenCalledTimes(1);
            expect(onNavigate).toHaveBeenCalledWith('wotNetwork');
            const navEvent = dispatchSpy.mock.calls
                .map(([ev]) => ev)
                .find((ev): ev is CustomEvent<{ topic: string }> =>
                    ev instanceof CustomEvent && ev.type === 'humoco:navigate-help-center'
                );
            expect(navEvent).toBeInstanceOf(CustomEvent);
            expect(navEvent?.detail).toEqual({ topic: 'wotNetwork' });
        } finally {
            dispatchSpy.mockRestore();
        }
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

    it('renders nothing when closed and resets topic to prop when reopened with a new topic', () => {
        const { rerender } = render(
            <HelpModal
                isOpen={false}
                onClose={() => {}}
                topic="conceptParadigm"
            />
        );
        expect(screen.queryByRole('dialog')).toBeNull();

        rerender(
            <HelpModal
                isOpen={true}
                onClose={() => {}}
                topic="conceptParadigm"
            />
        );
        expect(
            screen.getByRole('heading', { level: 2, name: en['help.conceptParadigm.title'] }),
        ).toBeInTheDocument();

        fireEvent.click(screen.getByRole('button', { name: en['help.voucherBasics.title'] }));
        expect(
            screen.getByRole('heading', { level: 2, name: en['help.voucherBasics.title'] }),
        ).toBeInTheDocument();

        rerender(
            <HelpModal
                isOpen={false}
                onClose={() => {}}
                topic="conceptParadigm"
            />
        );
        expect(screen.queryByRole('dialog')).toBeNull();

        rerender(
            <HelpModal
                isOpen={true}
                onClose={() => {}}
                topic="mnemonic"
            />
        );
        expect(
            screen.getByRole('heading', { level: 2, name: en['help.mnemonic.title'] }),
        ).toBeInTheDocument();
    });
});
