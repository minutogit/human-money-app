import { describe, it, expect } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { InfoHint } from '../InfoHint';
import enJson from '../../../locales/en.json';

const en = enJson as Record<string, string>;

describe('InfoHint Component', () => {
    it('renders inline variant with short text and learn more button', () => {
        const { container } = render(<InfoHint topic="voucherBasics" variant="inline" />);

        const wrapper = container.firstElementChild;
        expect(wrapper).toBeInstanceOf(HTMLElement);
        expect(wrapper?.className).toContain('inline-flex');
        expect(wrapper?.className).not.toContain('rounded-2xl');

        const shortText = en['help.voucherBasics.short'];
        expect(screen.getByText(shortText)).toBeInTheDocument();

        const learnMoreBtn = screen.getByRole('button', { name: new RegExp(en['helpCenter.learnMore'], 'i') });
        expect(learnMoreBtn).toBeInTheDocument();
    });

    it('renders banner variant correctly and opens topic modal on learn more', () => {
        const { container } = render(<InfoHint topic="benutzerId" variant="banner" />);

        const wrapper = container.firstElementChild;
        expect(wrapper).toBeInstanceOf(HTMLElement);
        expect(wrapper?.className).toContain('rounded-2xl');
        expect(wrapper?.className).toContain('border-theme-primary/10');

        const shortText = en['help.benutzerId.short'];
        expect(screen.getByText(shortText)).toBeInTheDocument();

        const learnMoreBtn = screen.getByRole('button', { name: new RegExp(en['helpCenter.learnMore'], 'i') });
        expect(learnMoreBtn).toBeInTheDocument();
        fireEvent.click(learnMoreBtn);
        expect(screen.getByRole('dialog')).toBeInTheDocument();
        expect(screen.getByText(en['help.benutzerId.title'])).toBeInTheDocument();
    });

    it('clicking learn more button opens HelpModal', () => {
        render(<InfoHint topic="wotNetwork" />);

        expect(screen.queryByRole('dialog')).toBeNull();

        const learnMoreBtn = screen.getByRole('button', { name: new RegExp(en['helpCenter.learnMore'], 'i') });
        fireEvent.click(learnMoreBtn);

        const dialog = screen.getByRole('dialog');
        expect(dialog).toBeInTheDocument();
        expect(screen.getByText(en['help.wotNetwork.title'])).toBeInTheDocument();

        // Close modal
        const closeBtn = screen.getByRole('button', { name: /close|schließen/i });
        fireEvent.click(closeBtn);
        expect(screen.queryByRole('dialog')).toBeNull();
    });

    it('returns null for an unknown help topic', () => {
        const { container } = render(
            <InfoHint topic={'unknownTopic' as unknown as Parameters<typeof InfoHint>[0]['topic']} />
        );
        expect(container.firstChild).toBeNull();
    });
});
