import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { RenewAdModal } from '../RenewAdModal';
import { FrontendMyAd } from '../../../types';

describe('RenewAdModal Component', () => {
  const nowSecs = Math.floor(Date.now() / 1000);

  const mockActiveAd: FrontendMyAd = {
    id: 'ad-renew-1',
    kind: 'offer',
    title: 'Apfelsaft frisch gepresst',
    text: '5 Liter Bag-in-Box',
    pricing: '10 Minuto',
    publishedAt: nowSecs - 86400 * 20,
    expiresAt: nowSecs + 86400 * 10,
    allowInquiries: true,
    status: 'active',
    daysLeft: 10,
    published: true,
  };

  const mockExpiredAd: FrontendMyAd = {
    id: 'ad-renew-2',
    kind: 'request',
    title: 'Suche Anhänger',
    text: 'PKW-Anhänger gesucht',
    publishedAt: nowSecs - 86400 * 40,
    expiresAt: nowSecs - 86400 * 10,
    allowInquiries: true,
    status: 'expired',
    daysLeft: 0,
    published: true,
  };

  it('renders renew modal for active ad with duration options and confirms renewal', async () => {
    const onRenew = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    render(
      <RenewAdModal
        isOpen={true}
        onClose={onClose}
        onRenew={onRenew}
        ad={mockActiveAd}
      />
    );

    expect(screen.getByRole('heading', { level: 2, name: /Inserat verlängern|Renew ad/i })).toBeInTheDocument();
    expect(screen.getByText('Apfelsaft frisch gepresst')).toBeInTheDocument();

    // Select 180 days
    const tab180 = screen.getByRole('tab', { name: /180 Tage|180 days/i });
    fireEvent.click(tab180);

    // Confirm
    const confirmBtn = screen.getByRole('button', { name: /Jetzt verlängern|Renew now/i });
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(onRenew).toHaveBeenCalledWith('ad-renew-1', 180);
      expect(onClose).toHaveBeenCalledTimes(1);
    });
  });

  it('renders republish modal for expired ad', () => {
    render(
      <RenewAdModal
        isOpen={true}
        onClose={vi.fn()}
        onRenew={vi.fn()}
        ad={mockExpiredAd}
      />
    );

    expect(screen.getByRole('heading', { level: 2, name: /Inserat reaktivieren|Reactivate ad/i })).toBeInTheDocument();
    expect(screen.getByText('Suche Anhänger')).toBeInTheDocument();
  });

  it('resets durationDays to 90 on ad switch and ignores late onRenew resolution after close', async () => {
    let resolveRenew!: () => void;
    const onRenew = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveRenew = resolve;
        })
    );
    const onClose = vi.fn();

    const { rerender } = render(
      <RenewAdModal
        isOpen={true}
        onClose={onClose}
        onRenew={onRenew}
        ad={mockActiveAd}
      />
    );

    fireEvent.click(screen.getByRole('tab', { name: /180 Tage|180 days/i }));
    expect(screen.getByRole('tab', { name: /180 Tage|180 days/i })).toHaveAttribute('aria-selected', 'true');

    // Switch ad -> resets durationDays to 90
    rerender(
      <RenewAdModal
        isOpen={true}
        onClose={onClose}
        onRenew={onRenew}
        ad={mockExpiredAd}
      />
    );
    expect(screen.getByRole('tab', { name: /90 Tage|90 days/i })).toHaveAttribute('aria-selected', 'true');

    fireEvent.click(screen.getByRole('button', { name: /Jetzt verlängern|Renew now/i }));
    expect(onRenew).toHaveBeenCalledWith('ad-renew-2', 90);

    // Close modal before async renew resolves
    rerender(
      <RenewAdModal
        isOpen={false}
        onClose={onClose}
        onRenew={onRenew}
        ad={mockExpiredAd}
      />
    );
    resolveRenew();
    await Promise.resolve();
    await Promise.resolve();

    expect(onClose).not.toHaveBeenCalled();
  });

  it('renders nothing when ad is null', () => {
    const { container } = render(
      <RenewAdModal isOpen={true} onClose={vi.fn()} onRenew={vi.fn()} ad={null} />
    );

    expect(container.firstChild).toBeNull();
  });

  it('displays error banner without closing and re-enables buttons when onRenew rejects', async () => {
    const onRenew = vi.fn().mockRejectedValue(new Error('Storage full'));
    const onClose = vi.fn();

    render(
      <RenewAdModal isOpen={true} onClose={onClose} onRenew={onRenew} ad={mockActiveAd} />
    );

    fireEvent.click(screen.getByRole('button', { name: /Jetzt verlängern|Renew now/i }));

    expect(await screen.findByText('Storage full')).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: /Jetzt verlängern|Renew now/i })).toBeEnabled();
    expect(screen.getByRole('button', { name: /Abbrechen|Cancel/i })).toBeEnabled();
  });

  it('prevents duplicate submissions while in flight and ignores late rejection after close', async () => {
    let rejectRenew!: (err: Error) => void;
    const onRenew = vi.fn(
      () =>
        new Promise<void>((_resolve, reject) => {
          rejectRenew = reject;
        })
    );
    const onClose = vi.fn();

    const { rerender } = render(
      <RenewAdModal isOpen={true} onClose={onClose} onRenew={onRenew} ad={mockActiveAd} />
    );

    const confirmBtn = screen.getByRole('button', { name: /Jetzt verlängern|Renew now/i });
    const cancelBtn = screen.getByRole('button', { name: /Abbrechen|Cancel/i });
    fireEvent.click(confirmBtn);
    expect(confirmBtn).toBeDisabled();
    expect(cancelBtn).toBeDisabled();

    fireEvent.click(confirmBtn);
    expect(onRenew).toHaveBeenCalledTimes(1);

    rerender(<RenewAdModal isOpen={false} onClose={onClose} onRenew={onRenew} ad={mockActiveAd} />);
    rejectRenew(new Error('Late error'));
    await Promise.resolve();
    await Promise.resolve();

    rerender(<RenewAdModal isOpen={true} onClose={onClose} onRenew={onRenew} ad={mockActiveAd} />);
    expect(screen.queryByText('Late error')).not.toBeInTheDocument();
  });

  it('updates live expiration preview when switching between 30, 90, and 180 day tabs', () => {
    render(
      <RenewAdModal isOpen={true} onClose={vi.fn()} onRenew={vi.fn()} ad={mockActiveAd} />
    );

    expect(screen.getByText(/noch 90 Tage|90 days left/i)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('tab', { name: /30 Tage|30 days/i }));
    expect(screen.getByText(/noch 30 Tage|30 days left/i)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('tab', { name: /180 Tage|180 days/i }));
    expect(screen.getByText(/noch 180 Tage|180 days left/i)).toBeInTheDocument();
  });
});
