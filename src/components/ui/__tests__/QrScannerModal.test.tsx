import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { QrScannerModal } from '../QrScannerModal';
import { readFromClipboard } from '../../../utils/clipboard';

vi.mock('../../../utils/clipboard', () => ({
    readFromClipboard: vi.fn(),
}));

vi.mock('../../../utils/log', () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
    },
}));

describe('QrScannerModal', () => {
    const onClose = vi.fn();
    const onScan = vi.fn();

    beforeEach(() => {
        vi.clearAllMocks();
        delete (window as unknown as { BarcodeDetector?: unknown }).BarcodeDetector;
    });

    it('shows unsupported fallback and allows 1-click clipboard paste when BarcodeDetector is absent', async () => {
        vi.mocked(readFromClipboard).mockResolvedValue({
            text: '  humoco://kit?v=1&ent=abcd  ',
            error: false,
        });

        render(<QrScannerModal isOpen={true} onClose={onClose} onScan={onScan} />);

        expect(screen.getByTestId('qr-scanner-camera-error')).toBeInTheDocument();
        expect(screen.queryByTestId('qr-scanner-image-btn')).not.toBeInTheDocument();

        fireEvent.click(screen.getByTestId('qr-scanner-paste-btn'));

        await waitFor(() => {
            expect(onScan).toHaveBeenCalledWith('humoco://kit?v=1&ent=abcd');
            expect(onClose).toHaveBeenCalled();
        });
    });

    it('shows inline error if clipboard fallback is empty', async () => {
        vi.mocked(readFromClipboard).mockResolvedValue({ text: '   ', error: false });

        render(<QrScannerModal isOpen={true} onClose={onClose} onScan={onScan} />);

        fireEvent.click(screen.getByTestId('qr-scanner-paste-btn'));

        await waitFor(() => {
            expect(screen.getByText(/Clipboard does not contain a code|Zwischenablage enthält keinen Code/i)).toBeInTheDocument();
        });
        expect(onScan).not.toHaveBeenCalled();
    });
});
