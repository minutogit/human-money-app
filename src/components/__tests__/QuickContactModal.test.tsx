// src/components/__tests__/QuickContactModal.test.tsx
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import { QuickContactModal } from '../profile/QuickContactModal';
import { profileService } from '../../services/profileService';
import { marketService } from '../../services/marketService';
import * as clipboard from '../../utils/clipboard';

vi.mock('../../services/profileService', () => ({
    profileService: {
        getMyQuickContact: vi.fn(),
    },
}));

vi.mock('../../services/marketService', () => ({
    marketService: {
        getMyMarketProfile: vi.fn().mockResolvedValue(null),
        setCardPublished: vi.fn(),
    },
}));

vi.mock('../../context/SessionContext', () => ({
    useOptionalSession: () => ({
        protectAction: vi.fn(async (action) => action('pw')),
    }),
}));

vi.mock('../../utils/clipboard', () => ({
    copyToClipboard: vi.fn().mockResolvedValue(true),
}));

describe('QuickContactModal Component', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        localStorage.clear();
        vi.mocked(marketService.getMyMarketProfile).mockResolvedValue(
            null as unknown as Awaited<ReturnType<typeof marketService.getMyMarketProfile>>
        );
    });

    it('renders loading state initially and displays shortened ID in Simple mode and full DID in Expert mode', async () => {
        vi.mocked(profileService.getMyQuickContact).mockResolvedValue({
            did: 'did:key:z6MkpTHR8VNsBxYqzFoH1oMrY',
            name: 'Alice Example',
            syncUrls: ['https://sync1.example.org/sync.php'],
            uri: 'humoco://contact?did=did%3Akey%3Az6MkpTHR8VNsBxYqzFoH1oMrY&name=Alice%20Example&sync=https%3A%2F%2Fsync1.example.org%2Fsync.php',
            qrSvg: '<svg data-testid="qr-svg-mock"></svg>',
        });

        const { unmount } = render(
            <QuickContactModal
                isOpen={true}
                onClose={vi.fn()}
            />
        );

        await waitFor(() => {
            expect(screen.getByText('Alice Example')).toBeInTheDocument();
        });

        expect(screen.getByTestId('quick-contact-qr-svg')).toBeInTheDocument();
        expect(screen.getByText(/H1oMrY/)).toBeInTheDocument();
        expect(screen.queryByText(/did:key:z6MkpTHR8VNs/)).not.toBeInTheDocument();
        expect(screen.getByTestId('quick-contact-copy-button')).toBeInTheDocument();
        unmount();

        localStorage.setItem('humoco_ui_mode', 'expert');
        render(
            <QuickContactModal
                isOpen={true}
                onClose={vi.fn()}
            />
        );

        await waitFor(() => {
            expect(screen.getByText(/did:key:z6MkpTHR8VNsBxYqzFoH1oMrY/)).toBeInTheDocument();
        });
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

    it('switches to business card tab and lets unpublished users publish deliberately', async () => {
        vi.mocked(profileService.getMyQuickContact).mockResolvedValueOnce({
            did: 'did:key:z6MkpTHR8VNsBxYqzFoH1oMrY',
            name: 'Charlie',
            syncUrls: [],
            uri: 'humoco://contact?did=did%3Akey%3Az6MkpTHR8VNsBxYqzFoH1oMrY',
            qrSvg: '<svg></svg>',
        });
        vi.mocked(marketService.getMyMarketProfile).mockResolvedValueOnce({
            did: 'did:key:z6MkpTHR8VNsBxYqzFoH1oMrY',
            published: false,
            shareUri: undefined,
            fileWire: undefined,
            qrSvg: undefined,
        } as unknown as Awaited<ReturnType<typeof marketService.getMyMarketProfile>>);
        vi.mocked(marketService.setCardPublished).mockResolvedValueOnce({
            did: 'did:key:z6MkpTHR8VNsBxYqzFoH1oMrY',
            published: true,
            shareUri: 'humoco://card?v=1',
            fileWire: '{"signed":"wire"}',
            qrSvg: '<svg data-testid="published-card-qr"></svg>',
        } as unknown as Awaited<ReturnType<typeof marketService.setCardPublished>>);

        render(
            <QuickContactModal
                isOpen={true}
                onClose={vi.fn()}
            />
        );

        await waitFor(() => {
            expect(screen.getByTestId('share-tab-card')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('share-tab-card'));

        const publishBtn = await screen.findByTestId('publish-card-in-share-btn');
        expect(screen.queryByTestId('share-card-save-file-button')).not.toBeInTheDocument();

        fireEvent.click(publishBtn);

        await waitFor(() => {
            expect(marketService.setCardPublished).toHaveBeenCalledWith(true, false, 'pw');
            expect(screen.getByTestId('share-card-save-file-button')).toBeInTheDocument();
            expect(screen.getByTestId('share-card-qr-img')).toBeInTheDocument();
        });
    });

    it('allows copying shareUri and saving .hmcard file when card is already published', async () => {
        vi.mocked(profileService.getMyQuickContact).mockResolvedValueOnce({
            did: 'did:key:z6MkpTHR8VNsBxYqzFoH1oMrY',
            name: 'Charlie',
            syncUrls: [],
            uri: 'humoco://contact?did=did%3Akey%3Az6MkpTHR8VNsBxYqzFoH1oMrY',
            qrSvg: '<svg></svg>',
        });
        vi.mocked(marketService.getMyMarketProfile).mockResolvedValueOnce({
            did: 'did:key:z6MkpTHR8VNsBxYqzFoH1oMrY',
            published: true,
            shareUri: 'humoco://card?abc',
            fileWire: '{"signed":"wire"}',
            qrSvg: '<svg></svg>',
        } as unknown as Awaited<ReturnType<typeof marketService.getMyMarketProfile>>);
        vi.mocked(save).mockResolvedValueOnce('/tmp/charlie.hmcard');

        render(
            <QuickContactModal
                isOpen={true}
                onClose={vi.fn()}
            />
        );

        await waitFor(() => {
            expect(screen.getByTestId('share-tab-card')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('share-tab-card'));

        const copyCardBtn = await screen.findByTestId('share-card-copy-link-button');
        fireEvent.click(copyCardBtn);

        await waitFor(() => {
            expect(clipboard.copyToClipboard).toHaveBeenCalledWith('humoco://card?abc');
        });

        fireEvent.click(screen.getByTestId('share-card-save-file-button'));

        await waitFor(() => {
            expect(save).toHaveBeenCalled();
            expect(writeFile).toHaveBeenCalledWith('/tmp/charlie.hmcard', expect.anything());
        });
    });
});
