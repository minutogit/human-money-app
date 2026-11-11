// src/components/__tests__/QuickContactModal.test.tsx
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { QuickContactModal } from '../profile/QuickContactModal';
import { profileService } from '../../services/profileService';
import * as clipboard from '../../utils/clipboard';

vi.mock('../../services/profileService', () => ({
    profileService: {
        getMyQuickContact: vi.fn(),
    },
}));

vi.mock('../../utils/clipboard', () => ({
    copyToClipboard: vi.fn().mockResolvedValue(true),
}));

describe('QuickContactModal Component', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('renders loading state initially and then shows QR code and details', async () => {
        vi.mocked(profileService.getMyQuickContact).mockResolvedValueOnce({
            did: 'did:key:z6MkpTHR8VNsBxYqzFoH1oMrY',
            name: 'Alice Example',
            syncUrls: ['https://sync1.example.org/sync.php'],
            uri: 'humoco://contact?did=did%3Akey%3Az6MkpTHR8VNsBxYqzFoH1oMrY&name=Alice%20Example&sync=https%3A%2F%2Fsync1.example.org%2Fsync.php',
            qrSvg: '<svg data-testid="qr-svg-mock"></svg>',
        });

        render(
            <QuickContactModal
                isOpen={true}
                onClose={vi.fn()}
            />
        );

        await waitFor(() => {
            expect(screen.getByText('Alice Example')).toBeInTheDocument();
        });

        expect(screen.getByTestId('quick-contact-qr-svg')).toBeInTheDocument();
        expect(screen.getByText(/did:key:z6MkpTHR8VNs/)).toBeInTheDocument();
        expect(screen.getByRole('button', { name: /contacts\.quickContactCopyBtn|Kontaktlink kopieren|Copy/i })).toBeInTheDocument();
    });

    it('copies contact text with URI to clipboard when clicking copy button', async () => {
        vi.mocked(profileService.getMyQuickContact).mockResolvedValueOnce({
            did: 'did:key:z6MkpTHR8VNsBxYqzFoH1oMrY',
            name: 'Bob Test',
            syncUrls: [],
            uri: 'humoco://contact?did=did%3Akey%3Az6MkpTHR8VNsBxYqzFoH1oMrY&name=Bob%20Test',
            qrSvg: '<svg></svg>',
        });

        render(
            <QuickContactModal
                isOpen={true}
                onClose={vi.fn()}
            />
        );

        await waitFor(() => {
            expect(screen.getByText('Bob Test')).toBeInTheDocument();
        });

        const copyBtn = screen.getByTestId('quick-contact-copy-button');
        fireEvent.click(copyBtn);

        await waitFor(() => {
            expect(clipboard.copyToClipboard).toHaveBeenCalledWith(
                expect.stringContaining('humoco://contact?did=did%3Akey%3Az6MkpTHR8VNsBxYqzFoH1oMrY')
            );
        });
    });

    it('calls onOpenFullShare and closes when clicking full share link', async () => {
        vi.mocked(profileService.getMyQuickContact).mockResolvedValueOnce({
            did: 'did:key:z6MkpTHR8VNsBxYqzFoH1oMrY',
            name: 'Charlie',
            syncUrls: [],
            uri: 'humoco://contact?did=did%3Akey%3Az6MkpTHR8VNsBxYqzFoH1oMrY',
            qrSvg: '<svg></svg>',
        });

        const onClose = vi.fn();
        const onOpenFullShare = vi.fn();

        render(
            <QuickContactModal
                isOpen={true}
                onClose={onClose}
                onOpenFullShare={onOpenFullShare}
            />
        );

        await waitFor(() => {
            expect(screen.getByText('Charlie')).toBeInTheDocument();
        });

        const fullShareLink = screen.getByTestId('open-full-share-button');
        fireEvent.click(fullShareLink);

        expect(onClose).toHaveBeenCalled();
        expect(onOpenFullShare).toHaveBeenCalled();
    });
});
