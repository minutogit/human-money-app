import { render, screen, fireEvent } from '@testing-library/react';
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
  };

  it('renders renew modal for active ad with duration options and confirms renewal', () => {
    const onRenew = vi.fn();
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

    expect(onRenew).toHaveBeenCalledWith('ad-renew-1', 180);
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
});
