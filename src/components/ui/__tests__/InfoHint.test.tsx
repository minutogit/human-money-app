import { describe, it, expect } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { InfoHint } from '../InfoHint';
import enJson from '../../../locales/en.json';

const en = enJson as Record<string, string>;

describe('InfoHint Component', () => {
    it('renders inline variant with short text and learn more button', () => {
        render(<InfoHint topic="voucherBasics" variant="inline" />);

        const shortText = en['help.voucherBasics.short'];
        expect(screen.getByText(shortText)).toBeInTheDocument();

        const learnMoreBtn = screen.getByRole('button', { name: new RegExp(en['helpCenter.learnMore'], 'i') });
        expect(learnMoreBtn).toBeInTheDocument();
    });

    it('renders banner variant correctly', () => {
        render(<InfoHint topic="benutzerId" variant="banner" />);

        const shortText = en['help.benutzerId.short'];
        expect(screen.getByText(shortText)).toBeInTheDocument();

        const learnMoreBtn = screen.getByRole('button', { name: new RegExp(en['helpCenter.learnMore'], 'i') });
        expect(learnMoreBtn).toBeInTheDocument();
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
});
