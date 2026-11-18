import { render, screen, fireEvent, act } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { QrCodeDisplay } from '../QrCodeDisplay';
import { CopyButton } from '../CopyButton';
import { PasswordPairFields } from '../PasswordPairFields';
import { SearchFilterBar } from '../SearchFilterBar';
import { ModalShell } from '../ModalShell';
import enJson from '../../../locales/en.json';
import * as clipboardUtils from '../../../utils/clipboard';

vi.mock('../../../utils/clipboard', () => ({
    copyToClipboard: vi.fn(),
}));

const en = enJson as Record<string, string>;

describe('QrCodeDisplay', () => {
    it('renders inline SVG markup inside an accessible styled container with caption', () => {
        render(
            <QrCodeDisplay
                svg='<svg data-testid="mock-qr-svg" viewBox="0 0 10 10"><rect width="10" height="10" /></svg>'
                size={220}
                bgColor="#f8fafc"
                caption="Scan to connect"
                ariaLabel="Contact QR"
                data-testid="qr-wrapper"
                qrTestId="qr-box"
            />
        );

        expect(screen.getByTestId('qr-wrapper')).toBeInTheDocument();
        expect(screen.getByTestId('mock-qr-svg')).toBeInTheDocument();
        expect(screen.getByRole('img', { name: 'Contact QR' })).toBeInTheDocument();
        expect(screen.getByText('Scan to connect')).toBeInTheDocument();

        const box = screen.getByTestId('qr-box');
        expect(box).toHaveStyle({ width: '220px', height: '220px', backgroundColor: '#f8fafc' });
        expect(box.className).toContain('p-3');
    });

    it('supports img render mode and includeMargin=false', () => {
        render(
            <QrCodeDisplay
                svg='<svg xmlns="http://www.w3.org/2000/svg"><rect width="5" height="5" /></svg>'
                renderAs="img"
                includeMargin={false}
                ariaLabel="Card QR Image"
                qrTestId="qr-box"
            />
        );

        const img = screen.getByRole('img', { name: 'Card QR Image' }) as HTMLImageElement;
        expect(img.tagName).toBe('IMG');
        expect(img.src).toContain('data:image/svg+xml;utf8,');
        expect(screen.getByTestId('qr-box').className).toContain('p-0');
    });

    it('renders fallback safely when svg is null or unsafe', () => {
        const { rerender } = render(
            <QrCodeDisplay
                svg={null}
                value="humoco://pay?to=did:key:alice"
                fallback={<span data-testid="custom-qr-fallback">Too large</span>}
            />
        );

        expect(screen.getByTestId('custom-qr-fallback')).toBeInTheDocument();

        rerender(
            <QrCodeDisplay
                svg='<svg><script>alert(1)</script></svg>'
                value="humoco://pay?to=did:key:alice"
            />
        );

        expect(screen.getByRole('img', { name: 'QR Code: humoco://pay?to=did:key:alice' })).toBeInTheDocument();
        expect(document.querySelector('script')).toBeNull();
    });
});

describe('CopyButton', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(clipboardUtils.copyToClipboard).mockResolvedValue(true);
    });

    it('copies value, shows temporary copied feedback, and resets after delay', async () => {
        vi.useFakeTimers();
        try {
            const onCopied = vi.fn();
            render(
                <CopyButton
                    value="did:key:zAlice123"
                    resetDelayMs={1500}
                    onCopied={onCopied}
                    data-testid="copy-btn"
                />
            );

            const button = screen.getByTestId('copy-btn');
            expect(button).toHaveTextContent(en['common.copy']);

            await act(async () => {
                fireEvent.click(button);
            });

            expect(clipboardUtils.copyToClipboard).toHaveBeenCalledWith('did:key:zAlice123');
            expect(onCopied).toHaveBeenCalledWith('did:key:zAlice123');
            expect(button).toHaveTextContent(en['common.copied']);

            act(() => {
                vi.advanceTimersByTime(1500);
            });
            expect(button).toHaveTextContent(en['common.copy']);
        } finally {
            vi.useRealTimers();
        }
    });

    it('supports iconOnly mode and invokes onCopyFailed when clipboard fails', async () => {
        vi.mocked(clipboardUtils.copyToClipboard).mockResolvedValueOnce(false);
        const onCopyFailed = vi.fn();

        render(
            <CopyButton
                value="secret-id"
                iconOnly
                ariaLabel="Copy device ID"
                onCopyFailed={onCopyFailed}
            />
        );

        const button = screen.getByRole('button', { name: 'Copy device ID' });
        expect(button.textContent).toBe('');

        await act(async () => {
            fireEvent.click(button);
        });

        expect(onCopyFailed).toHaveBeenCalledTimes(1);
    });

    it('disables itself when value is empty', () => {
        render(<CopyButton value="" />);
        expect(screen.getByRole('button', { name: en['common.copy'] })).toBeDisabled();
    });
});

describe('PasswordPairFields', () => {
    it('renders controlled inputs, visibility toggles, minLength hint, and mismatch feedback', () => {
        const handlePasswordChange = vi.fn();
        const handleConfirmChange = vi.fn();

        const { rerender } = render(
            <PasswordPairFields
                password="short"
                confirmPassword="other"
                onPasswordChange={handlePasswordChange}
                onConfirmPasswordChange={handleConfirmChange}
                minLength={8}
                passwordTestId="pw-input"
                confirmTestId="pw-confirm-input"
            />
        );

        const pwInput = screen.getByTestId('pw-input') as HTMLInputElement;
        const confirmInput = screen.getByTestId('pw-confirm-input') as HTMLInputElement;

        expect(pwInput.type).toBe('password');
        expect(confirmInput.type).toBe('password');
        expect(screen.getByTestId('password-minlength-hint')).toHaveTextContent(en['auth.passwordMinChars']);
        expect(screen.getByTestId('password-mismatch-error')).toHaveTextContent(en['auth.passwordsDontMatch']);
        expect(confirmInput).toHaveAttribute('aria-invalid', 'true');

        // Toggle password visibility via built-in Input eye toggles
        const toggles = screen.getAllByTestId('input-visibility-toggle');
        expect(toggles).toHaveLength(2);
        fireEvent.click(toggles[0]);
        expect(pwInput.type).toBe('text');

        fireEvent.change(pwInput, { target: { value: 'longenough123' } });
        expect(handlePasswordChange).toHaveBeenCalledWith('longenough123');

        fireEvent.change(confirmInput, { target: { value: 'longenough123' } });
        expect(handleConfirmChange).toHaveBeenCalledWith('longenough123');

        rerender(
            <PasswordPairFields
                password="longenough123"
                confirmPassword="longenough123"
                onPasswordChange={handlePasswordChange}
                onConfirmPasswordChange={handleConfirmChange}
                minLength={8}
                minLengthHint={<span data-testid="custom-min-hint">At least 8 chars</span>}
                passwordTestId="pw-input"
                confirmTestId="pw-confirm-input"
            />
        );

        expect(screen.queryByTestId('password-mismatch-error')).not.toBeInTheDocument();
        expect(screen.getByTestId('custom-min-hint')).toBeInTheDocument();
    });
});

describe('SearchFilterBar', () => {
    it('updates search text, clears via X button, and renders optional rightSlot', () => {
        const handleChange = vi.fn();
        const { rerender } = render(
            <SearchFilterBar
                value=""
                onChange={handleChange}
                placeholder="Search contacts..."
                rightSlot={<button type="button">Filter</button>}
                inputTestId="search-input"
                clearTestId="search-clear"
            />
        );

        const input = screen.getByTestId('search-input');
        expect(screen.queryByTestId('search-clear')).not.toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Filter' })).toBeInTheDocument();

        fireEvent.change(input, { target: { value: 'Alice' } });
        expect(handleChange).toHaveBeenCalledWith('Alice');

        rerender(
            <SearchFilterBar
                value="Alice"
                onChange={handleChange}
                placeholder="Search contacts..."
                rightSlot={<button type="button">Filter</button>}
                inputTestId="search-input"
                clearTestId="search-clear"
            />
        );

        const clearBtn = screen.getByTestId('search-clear');
        fireEvent.click(clearBtn);
        expect(handleChange).toHaveBeenCalledWith('');
    });
});

describe('ModalShell hideCloseButton', () => {
    it('omits top-right close button when hideCloseButton is true while preserving Escape handling', () => {
        const onClose = vi.fn();
        render(
            <ModalShell
                isOpen={true}
                onClose={onClose}
                hideCloseButton={true}
                title="Protected Action"
            >
                <p>Body</p>
            </ModalShell>
        );

        expect(screen.queryByRole('button', { name: /close dialog/i })).not.toBeInTheDocument();
        fireEvent.keyDown(window, { key: 'Escape' });
        expect(onClose).toHaveBeenCalledTimes(1);
    });
});
