import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { QrScannerModal } from '../QrScannerModal';
import { readFromClipboard } from '../../../utils/clipboard';
import enJson from '../../../locales/en.json';

const en = enJson as Record<string, string>;

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
            expect(screen.getByText(en['common.qrClipboardEmpty'])).toBeInTheDocument();
        });
        expect(onScan).not.toHaveBeenCalled();
        expect(onClose).not.toHaveBeenCalled();
    });

    it('renders nothing when closed, surfaces clipboard errors, and scans or rejects QR images while closing bitmaps', async () => {
        const { rerender, unmount } = render(
            <QrScannerModal isOpen={false} onClose={onClose} onScan={onScan} />
        );
        expect(screen.queryByRole('dialog')).toBeNull();

        vi.mocked(readFromClipboard).mockResolvedValueOnce({ text: '', error: true });
        rerender(<QrScannerModal isOpen={true} onClose={onClose} onScan={onScan} />);

        fireEvent.click(screen.getByTestId('qr-scanner-paste-btn'));
        await waitFor(() => {
            expect(screen.getByText(en['common.qrClipboardEmpty'])).toBeInTheDocument();
        });

        vi.mocked(readFromClipboard).mockRejectedValueOnce(new Error('clipboard crashed'));
        fireEvent.click(screen.getByTestId('qr-scanner-paste-btn'));
        await waitFor(() => {
            expect(screen.getByText('clipboard crashed')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByRole('button', { name: en['common.close'] }));
        expect(onClose).toHaveBeenCalledTimes(1);
        onClose.mockClear();
        unmount();

        const winWithDetector = window as unknown as {
            BarcodeDetector?: new (opts?: { formats?: string[] }) => {
                detect: (img: unknown) => Promise<Array<{ rawValue?: string }>>;
            };
        };
        const globalWithBitmap = globalThis as unknown as {
            createImageBitmap?: (file: Blob) => Promise<{ close: () => void }>;
        };
        const prevDetector = winWithDetector.BarcodeDetector;
        const prevBitmap = globalWithBitmap.createImageBitmap;
        const bitmapClose = vi.fn();
        const detectSpy = vi.fn();

        winWithDetector.BarcodeDetector = class MockBarcodeDetector {
            detect(img: unknown) {
                return detectSpy(img);
            }
        };
        globalWithBitmap.createImageBitmap = vi.fn().mockResolvedValue({ close: bitmapClose });

        try {
            render(
                <QrScannerModal isOpen={true} onClose={onClose} onScan={onScan} />
            );
            expect(screen.getByTestId('qr-scanner-image-btn')).toBeInTheDocument();

            const fileInput = document.body.querySelector('input[type="file"]') as HTMLInputElement | null;
            expect(fileInput).not.toBeNull();

            detectSpy.mockResolvedValueOnce([]);
            const emptyQrFile = new File(['png'], 'empty.png', { type: 'image/png' });
            fireEvent.change(fileInput as HTMLInputElement, { target: { files: [emptyQrFile] } });

            await waitFor(() => {
                expect(screen.getByText(en['common.qrImageNoCodeFound'])).toBeInTheDocument();
            });
            expect(bitmapClose).toHaveBeenCalledTimes(1);
            expect(onScan).not.toHaveBeenCalled();

            detectSpy.mockResolvedValueOnce([{ rawValue: '  humoco://pay?to=did:key:zAlice  ' }]);
            const validQrFile = new File(['png'], 'valid.png', { type: 'image/png' });
            fireEvent.change(fileInput as HTMLInputElement, { target: { files: [validQrFile] } });

            await waitFor(() => {
                expect(onScan).toHaveBeenCalledWith('humoco://pay?to=did:key:zAlice');
                expect(onClose).toHaveBeenCalledTimes(1);
            });
            expect(bitmapClose).toHaveBeenCalledTimes(2);
        } finally {
            if (prevDetector === undefined) {
                delete winWithDetector.BarcodeDetector;
            } else {
                winWithDetector.BarcodeDetector = prevDetector;
            }
            if (prevBitmap === undefined) {
                delete globalWithBitmap.createImageBitmap;
            } else {
                globalWithBitmap.createImageBitmap = prevBitmap;
            }
        }
    });
});
