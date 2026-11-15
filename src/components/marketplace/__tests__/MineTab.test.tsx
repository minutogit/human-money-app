import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { MineTab } from '../MineTab';
import {
  FrontendMyMarketProfile,
  FrontendPostInboxSummary,
} from '../../../types';

describe('MineTab Component', () => {
  const nowSecs = Math.floor(Date.now() / 1000);

  const mockMyProfile: FrontendMyMarketProfile = {
    profile: {
      id: 'did:key:z6MkuSelf',
      firstName: 'Max',
      lastName: 'Mustermann',
      organization: 'Musterbetrieb',
      address: { city: 'Musterstadt' },
    },
    visibility: {
      name: 'everyone',
      organization: 'everyone',
      community: 'everyone',
      city: 'everyone',
      url: 'everyone',
      picture: 'everyone',
      street: 'everyone',
      email: 'everyone',
      phone: 'everyone',
      coordinates: 'everyone',
      gender: 'everyone',
    },
    skills: [
      {
        title: 'Software-Entwicklung',
        text: 'Full-Stack Web & Mobile Apps',
        pricing: '80 Minuto / Std',
      },
    ],
    ads: [
      {
        id: 'ad-active-1',
        kind: 'offer',
        title: 'IT-Beratung kostenlos',
        text: 'Erste Stunde kostenlos für gemeinnützige Projekte',
        pricing: 'Kostenlos',
        publishedAt: nowSecs - 86400 * 5,
        expiresAt: nowSecs + 86400 * 25,
        allowInquiries: true,
        status: 'active',
        daysLeft: 25,
      },
      {
        id: 'ad-expired-1',
        kind: 'request',
        title: 'Altes Inserat',
        publishedAt: nowSecs - 86400 * 60,
        expiresAt: nowSecs - 86400 * 10,
        allowInquiries: true,
        status: 'expired',
        daysLeft: 0,
      },
    ],
    maxHops: 2,
    allowInquiries: true,
    published: true,
    qrIsPartial: false,
  };

  const mockInbox: FrontendPostInboxSummary = {
    pendingSignRequests: [],
    pendingWotHandshakes: [],
    sellerInquiries: [
      {
        inquiryId: 'inq-1',
        counterpartyDid: 'did:key:z6MkuSender',
        counterpartyName: 'Interessent Peter',
        target: { kind: 'ad', ad_id: 'ad-1', ad_title: 'IT-Beratung kostenlos' },
        message: 'Hallo, ich hätte Interesse an einer Beratung.',
        createdAt: nowSecs - 3600,
        replySyncs: [],
      },
    ],
    buyerInquiries: [],
    unacknowledgedPayments: [],
  };

  it('renders business card preview with publish toggle', () => {
    const onTogglePublished = vi.fn();
    const onOpenProfileEditor = vi.fn();

    render(
      <MineTab
        myProfile={mockMyProfile}
        inbox={mockInbox}
        onOpenProfileEditor={onOpenProfileEditor}
        onOpenShareCard={vi.fn()}
        onTogglePublished={onTogglePublished}
        onCreateAd={vi.fn()}
        onEditAd={vi.fn()}
        onRenewAd={vi.fn()}
        onDeleteAd={vi.fn()}
      />
    );

    expect(screen.getByText('Max Mustermann')).toBeInTheDocument();
    expect(screen.getByText(/Musterbetrieb/i)).toBeInTheDocument();
    expect(screen.getByText('Software-Entwicklung')).toBeInTheDocument();

    // Published badge
    expect(screen.getByText(/Veröffentlicht|Published/i)).toBeInTheDocument();

    // Edit card button
    const editBtn = screen.getByRole('button', { name: /Visitenkarte bearbeiten|Edit business card/i });
    fireEvent.click(editBtn);
    expect(onOpenProfileEditor).toHaveBeenCalled();
  });

  it('renders active ads and collapsible expired ads section', () => {
    const onEditAd = vi.fn();
    const onRenewAd = vi.fn();
    const onDeleteAd = vi.fn();

    render(
      <MineTab
        myProfile={mockMyProfile}
        inbox={mockInbox}
        onOpenProfileEditor={vi.fn()}
        onOpenShareCard={vi.fn()}
        onTogglePublished={vi.fn()}
        onCreateAd={vi.fn()}
        onEditAd={onEditAd}
        onRenewAd={onRenewAd}
        onDeleteAd={onDeleteAd}
      />
    );

    // Active ad rendered
    expect(screen.getByText('IT-Beratung kostenlos')).toBeInTheDocument();

    // Expired ads accordion
    const expiredSectionBtn = screen.getByText(/Abgelaufene Inserate \(1\)|Expired ads \(1\)/i);
    expect(expiredSectionBtn).toBeInTheDocument();

    // Click to expand expired ads
    fireEvent.click(expiredSectionBtn);
    expect(screen.getByText('Altes Inserat')).toBeInTheDocument();
  });

  it('supports 1-click +90 day renewal in Simple mode and hides custom modal trigger until Expert mode', () => {
    localStorage.clear();
    const onQuickRenewAd = vi.fn();
    const onRenewAd = vi.fn();

    const { unmount } = render(
      <MineTab
        myProfile={mockMyProfile}
        inbox={mockInbox}
        onOpenProfileEditor={vi.fn()}
        onOpenShareCard={vi.fn()}
        onTogglePublished={vi.fn()}
        onCreateAd={vi.fn()}
        onEditAd={vi.fn()}
        onQuickRenewAd={onQuickRenewAd}
        onRenewAd={onRenewAd}
        onDeleteAd={vi.fn()}
      />
    );

    const quickRenewBtn = screen.getByTestId('quick-renew-ad-ad-active-1');
    expect(quickRenewBtn).toBeInTheDocument();
    expect(screen.queryByTestId('custom-renew-ad-ad-active-1')).not.toBeInTheDocument();

    fireEvent.click(quickRenewBtn);
    expect(onQuickRenewAd).toHaveBeenCalledWith(expect.objectContaining({ id: 'ad-active-1' }));
    unmount();

    localStorage.setItem('humoco_ui_mode', 'expert');
    render(
      <MineTab
        myProfile={mockMyProfile}
        inbox={mockInbox}
        onOpenProfileEditor={vi.fn()}
        onOpenShareCard={vi.fn()}
        onTogglePublished={vi.fn()}
        onCreateAd={vi.fn()}
        onEditAd={vi.fn()}
        onQuickRenewAd={onQuickRenewAd}
        onRenewAd={onRenewAd}
        onDeleteAd={vi.fn()}
      />
    );

    const customRenewBtn = screen.getByTestId('custom-renew-ad-ad-active-1');
    expect(customRenewBtn).toBeInTheDocument();
    fireEvent.click(customRenewBtn);
    expect(onRenewAd).toHaveBeenCalledWith(expect.objectContaining({ id: 'ad-active-1' }));
  });

  it('renders inquiries section with received inquiries and handles reply', () => {
    const onReplyInquiry = vi.fn();

    render(
      <MineTab
        myProfile={mockMyProfile}
        inbox={mockInbox}
        onOpenProfileEditor={vi.fn()}
        onOpenShareCard={vi.fn()}
        onTogglePublished={vi.fn()}
        onCreateAd={vi.fn()}
        onEditAd={vi.fn()}
        onRenewAd={vi.fn()}
        onDeleteAd={vi.fn()}
        onReplyInquiry={onReplyInquiry}
      />
    );

    expect(screen.getByText(/Interessent Peter/i)).toBeInTheDocument();
    expect(screen.getByText('Hallo, ich hätte Interesse an einer Beratung.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Antworten|Reply/i })).toBeInTheDocument();
  });
});
