// src/components/__tests__/QuickContactModal.test.tsx
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import { QuickContactModal } from '../QuickContactModal';
import { UiModeProvider } from '../../../context/UiModeContext';
import { profileService } from '../../../services/profileService';
import { marketService } from '../../../services/marketService';
import * as clipboard from '../../../utils/clipboard';

vi.mock('../../../services/profileService', () => ({
    profileService: {
        getMyQuickContact: vi.fn(),
    },
}));

vi.mock('../../../services/marketService', () => ({
    marketService: {
        getMyMarketProfile: vi.fn().mockResolvedValue(null),
        setCardPublished: vi.fn(),
    },
}));

vi.mock('../../../context/SessionContext', () => ({
    useOptionalSession: () => ({
        protectAction: vi.fn(async (action) => action('pw')),
    }),
}));

vi.mock('../../../utils/clipboard', () => ({
    copyToClipboard: vi.fn().mockResolvedValue(true),
}));

describe('QuickContactModal Component', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        localStorage.clear();
        vi.mocked(writeFile).mockResolvedValue(undefined);
        vi.mocked(marketService.getMyMarketProfile).mockResolvedValue(
            null as unknown as Awaited<ReturnType<typeof marketService.getMyMarketProfile>>
        );
    });

    it('renders loading state initially and displays shortened ID in Simple mode and full DID in Expert mode', async () => {
        let resolveQuickContact!: (val: Awaited<ReturnType<typeof profileService.getMyQuickContact>>) => void;
        vi.mocked(profileService.getMyQuickContact).mockImplementationOnce(
            () =>
                new Promise((resolve) => {
                    resolveQuickContact = resolve;
                })
        );

        const { unmount } = render(
            <QuickContactModal
                isOpen={true}
                onClose={vi.fn()}
            />
        );

        expect(screen.getByText('Loading...')).toBeInTheDocument();

        resolveQuickContact({
            did: 'did:key:z6MkpTHR8VNsBxYqzFoH1oMrY',
            name: 'Alice Example',
            syncUrls: ['https://sync1.example.org/sync.php'],
            uri: 'humoco://contact?did=did%3Akey%3Az6MkpTHR8VNsBxYqzFoH1oMrY&name=Alice%20Example&sync=https%3A%2F%2Fsync1.example.org%2Fsync.php',
            qrSvg: '<svg data-testid="qr-svg-mock"></svg>',
        });

        await waitFor(() => {
            expect(screen.getByText('Alice Example')).toBeInTheDocument();
        });

        expect(screen.queryByText('Loading...')).not.toBeInTheDocument();
        expect(screen.getByText('Sync available')).toBeInTheDocument();
        expect(screen.getByTestId('quick-contact-qr-svg')).toBeInTheDocument();
        expect(screen.getByText(/H1oMrY/)).toBeInTheDocument();
        expect(screen.queryByText(/did:key:z6MkpTHR8VNs/)).not.toBeInTheDocument();
        expect(screen.getByTestId('quick-contact-copy-button')).toBeInTheDocument();
        unmount();

        vi.mocked(profileService.getMyQuickContact).mockResolvedValueOnce({
            did: 'did:key:z6MkpTHR8VNsBxYqzFoH1oMrY',
            name: 'Alice Example',
            syncUrls: ['https://sync1.example.org/sync.php'],
            uri: 'humoco://contact?did=did%3Akey%3Az6MkpTHR8VNsBxYqzFoH1oMrY&name=Alice%20Example&sync=https%3A%2F%2Fsync1.example.org%2Fsync.php',
            qrSvg: '<svg data-testid="qr-svg-mock"></svg>',
        });

        render(
            <UiModeProvider initialMode="expert">
                <QuickContactModal
                    isOpen={true}
                    onClose={vi.fn()}
                />
            </UiModeProvider>
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
            expect(marketService.setCardPublished).toHaveBeenCalledWith(true, 'pw');
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
            expect(writeFile).toHaveBeenCalledTimes(1);
        });
        const [writtenPath, writtenBytes] = vi.mocked(writeFile).mock.calls[0];
        expect(writtenPath).toBe('/tmp/charlie.hmcard');
        expect(Array.from(writtenBytes as Uint8Array)).toEqual(
            Array.from(new TextEncoder().encode('{"signed":"wire"}'))
        );
    });

    it('translates structured BackendError codes when loading quick contact fails', async () => {
        vi.mocked(profileService.getMyQuickContact).mockRejectedValueOnce({
            code: 'error.auth.sessionTimeout',
            message: 'raw session timeout backend text',
        });

        render(
            <QuickContactModal
                isOpen={true}
                onClose={vi.fn()}
            />
        );

        await waitFor(() => {
            expect(
                screen.getByText('Session timed out. Please provide your password.')
            ).toBeInTheDocument();
        });
        expect(screen.queryByText('raw session timeout backend text')).not.toBeInTheDocument();
    });

    it('does not load quick contact or render modal when isOpen is false', () => {
        render(
            <QuickContactModal
                isOpen={false}
                onClose={vi.fn()}
            />
        );

        expect(profileService.getMyQuickContact).not.toHaveBeenCalled();
        expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    });

    it('renders translated publishError and keeps card unpublished when setCardPublished fails', async () => {
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
        vi.mocked(marketService.setCardPublished).mockRejectedValueOnce({
            code: 'error.auth.sessionTimeout',
            message: 'raw publish timeout',
        });

        render(
            <QuickContactModal
                isOpen={true}
                onClose={vi.fn()}
            />
        );

        fireEvent.click(await screen.findByTestId('share-tab-card'));
        const publishBtn = await screen.findByTestId('publish-card-in-share-btn');
        fireEvent.click(publishBtn);

        await waitFor(() => {
            expect(
                screen.getByText('Session timed out. Please provide your password.')
            ).toBeInTheDocument();
            expect(publishBtn).not.toBeDisabled();
        });
        expect(screen.queryByText('raw publish timeout')).not.toBeInTheDocument();
        expect(screen.queryByTestId('share-card-save-file-button')).not.toBeInTheDocument();
    });

    it('omits Sync available badge when syncUrls is empty', async () => {
        vi.mocked(profileService.getMyQuickContact).mockResolvedValueOnce({
            did: 'did:key:z6MkpTHR8VNsBxYqzFoH1oMrY',
            name: 'Offline Alice',
            syncUrls: [],
            uri: 'humoco://contact?did=did%3Akey%3Az6MkpTHR8VNsBxYqzFoH1oMrY',
            qrSvg: '<svg></svg>',
        });

        render(
            <QuickContactModal
                isOpen={true}
                onClose={vi.fn()}
            />
        );

        await waitFor(() => {
            expect(screen.getByText('Offline Alice')).toBeInTheDocument();
        });
        expect(screen.queryByText('Sync available')).not.toBeInTheDocument();
    });
});
