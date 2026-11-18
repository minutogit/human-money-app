import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { UserAvatar } from '../../ui/UserAvatar';
import { NotificationBell } from '../NotificationBell';
import { EventToastStack } from '../EventToastStack';
import { marketService } from '../../../services/marketService';
import { notifyAppNoticesUpdated, showTransientSyncToast } from '../../../utils/syncEvents';

vi.mock('../../../services/marketService');
vi.mock('../../../services/postService', () => ({
    postService: {
        syncPostChannels: vi.fn().mockResolvedValue({
            uploadedTo: 0,
            polledSyncs: 1,
            importedPayments: 0,
            importedReceipts: 0,
            importedSignatures: 0,
            newSignRequests: 0,
            newWotHandshakes: 0,
            newInquiries: 0,
            newInquiryMessages: 0,
            prunedBoxes: 0,
            errors: [],
        }),
    },
}));

const mockNavigate = vi.fn();
vi.mock('../../../context/NavigationContext', () => ({
    useNavigation: () => ({
        navigate: mockNavigate,
    }),
}));

vi.mock('../../../context/SessionContext', () => ({
    useSession: () => ({
        isSessionActive: true,
    }),
}));

vi.mock('react-i18next', () => ({
    useTranslation: () => ({
        t: (key: string, defaultOrOptions?: string | Record<string, unknown>, options?: Record<string, unknown>) => {
            let text =
                typeof defaultOrOptions === 'string'
                    ? defaultOrOptions
                    : defaultOrOptions && typeof defaultOrOptions.defaultValue === 'string'
                    ? defaultOrOptions.defaultValue
                    : key;
            const vars = (options && typeof options === 'object' ? options : typeof defaultOrOptions === 'object' ? defaultOrOptions : {}) as Record<string, unknown>;
            for (const [k, v] of Object.entries(vars)) {
                text = text.replace(`{{${k}}}`, String(v));
            }
            return text;
        },
    }),
}));

describe('UserAvatar, NotificationBell and EventToastStack', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('UserAvatar renders pictureUrl image, falls back to icon on error, and omits did:key title', () => {
        const { container, rerender } = render(
            <UserAvatar pictureUrl="data:image/webp;base64,AAA" name="did:key:z6MkSecretDid" size={40} />
        );
        expect(container.innerHTML).not.toContain('did:key:');
        const img = container.querySelector('img');
        expect(img).not.toBeNull();

        fireEvent.error(img!);
        expect(container.querySelector('img')).toBeNull();
        expect(container.querySelector('svg')).not.toBeNull();

        rerender(<UserAvatar pictureUrl={null} name="Alice" size={40} />);
        expect(container.querySelector('svg')).not.toBeNull();
        expect(container.firstElementChild?.getAttribute('title')).toBe('Alice');
    });

    it('NotificationBell displays unread badge, opens popover, and marks notices read on click', async () => {
        const nowSecs = Math.floor(Date.now() / 1000);
        vi.mocked(marketService.getAppNotices).mockResolvedValue({
            lastSyncAt: nowSecs - 120,
            notices: [
                {
                    id: 'n1',
                    kind: 'payment',
                    createdAt: nowSecs - 60,
                    read: false,
                    importedPayments: 2,
                    importedReceipts: 0,
                    importedSignatures: 0,
                    newSignRequests: 0,
                    newWotHandshakes: 0,
                    newInquiries: 0,
                    newInquiryMessages: 0,
                    importedCards: 0,
                    newAds: 0,
                },
            ],
        });
        vi.mocked(marketService.markAppNoticesRead).mockResolvedValue({
            lastSyncAt: nowSecs - 120,
            notices: [],
        });

        render(<NotificationBell />);

        const badge = await screen.findByTestId('notification-bell-badge');
        expect(badge).toHaveTextContent('1');

        fireEvent.click(screen.getByTestId('notification-bell-button'));
        expect(await screen.findByTestId('notification-bell-popover')).toBeInTheDocument();
        expect(screen.getByText('Zahlung über Sync empfangen')).toBeInTheDocument();

        fireEvent.click(screen.getByText('Zahlung über Sync empfangen'));
        await waitFor(() => {
            expect(marketService.markAppNoticesRead).toHaveBeenCalledWith('n1');
            expect(mockNavigate).toHaveBeenCalledWith({ view: 'wallet' });
        });
    });

    it('EventToastStack ignores already-stored notices at startup and only toasts newly arrived ones', async () => {
        const nowSecs = Math.floor(Date.now() / 1000);
        vi.mocked(marketService.getAppNotices).mockResolvedValueOnce({
            lastSyncAt: nowSecs - 300,
            notices: [
                {
                    id: 'old-notice',
                    kind: 'market',
                    createdAt: nowSecs - 300,
                    read: false,
                    importedPayments: 0,
                    importedReceipts: 0,
                    importedSignatures: 0,
                    newSignRequests: 0,
                    newWotHandshakes: 0,
                    newInquiries: 0,
                    newInquiryMessages: 0,
                    importedCards: 1,
                    newAds: 2,
                },
            ],
        });

        render(<EventToastStack />);

        // Initial startup notice must NOT pop up as a toast
        await waitFor(() => {
            expect(marketService.getAppNotices).toHaveBeenCalledTimes(1);
        });
        expect(screen.queryByTestId('event-toast-stack')).not.toBeInTheDocument();

        // Now simulate a new sync notice arriving
        vi.mocked(marketService.getAppNotices).mockResolvedValueOnce({
            lastSyncAt: nowSecs,
            notices: [
                {
                    id: 'fresh-notice',
                    kind: 'inquiry',
                    createdAt: nowSecs,
                    read: false,
                    importedPayments: 0,
                    importedReceipts: 0,
                    importedSignatures: 0,
                    newSignRequests: 0,
                    newWotHandshakes: 0,
                    newInquiries: 1,
                    newInquiryMessages: 0,
                    importedCards: 0,
                    newAds: 0,
                    counterpartyName: 'Berta',
                },
            ],
        });

        await act(async () => {
            notifyAppNoticesUpdated();
        });

        expect(await screen.findByText('Neue Marktplatz-Anfrage')).toBeInTheDocument();
        expect(screen.getByText('Von Berta')).toBeInTheDocument();

        // Transient manual sync toast also displays
        await act(async () => {
            showTransientSyncToast({ message: 'Alles aktuell – keine neuen Einträge.' });
        });
        expect(screen.getByText('Alles aktuell – keine neuen Einträge.')).toBeInTheDocument();
    });
});
