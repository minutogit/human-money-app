import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { MineTab } from '../MineTab';
import { UiModeProvider } from '../../../context/UiModeContext';
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
    inquiries: [
      {
        inquiryId: 'inq-1',
        role: 'seller',
        counterpartyDid: 'did:key:z6MkuSender',
        counterpartyName: 'Interessent Peter',
        isContact: false,
        status: 'open',
        target: { kind: 'ad', ad_id: 'ad-1', ad_title: 'IT-Beratung kostenlos' },
        createdAt: nowSecs - 3600,
        updatedAt: nowSecs - 3600,
        unread: true,
        replySyncs: [{ writeUrl: 'https://sync.example/w', readUrl: 'https://sync.example/r', dropPub: 'pub', failCount: 0, status: 'active' }],
        messages: [
          {
            msgId: 'msg-1',
            sender: 'them',
            text: 'Hallo, ich hätte Interesse an einer Beratung.',
            createdAt: nowSecs - 3600,
          },
        ],
      },
    ],
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

    render(
      <UiModeProvider initialMode="expert">
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
      </UiModeProvider>
    );

    const customRenewBtn = screen.getByTestId('custom-renew-ad-ad-active-1');
    expect(customRenewBtn).toBeInTheDocument();
    fireEvent.click(customRenewBtn);
    expect(onRenewAd).toHaveBeenCalledWith(expect.objectContaining({ id: 'ad-active-1' }));
  });

  it('renders inquiries section with chat messages and handles reply', () => {
    const onSendInquiryMessage = vi.fn().mockResolvedValue(undefined);

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
        onSendInquiryMessage={onSendInquiryMessage}
      />
    );

    expect(screen.getByText(/Interessent Peter/i)).toBeInTheDocument();
    expect(screen.getByText('Hallo, ich hätte Interesse an einer Beratung.')).toBeInTheDocument();
    expect(screen.getByTestId('inquiry-confirm-btn-inq-1')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('inquiry-confirm-btn-inq-1'));
    expect(onSendInquiryMessage).toHaveBeenCalledWith('inq-1', {
      text: undefined,
      decision: 'confirm',
    });
  });

  it('shows pay button on confirmed buyer inquiry even without paymentUri and passes counterpartyDid', () => {
    const onPayInquiry = vi.fn();
    const inboxWithConfirmedSent: FrontendPostInboxSummary = {
      ...mockInbox,
      inquiries: [
        {
          inquiryId: 'inq-sent-1',
          role: 'buyer',
          counterpartyDid: 'did:key:z6MkuSeller',
          counterpartyName: 'Bäcker Bernd',
          isContact: true,
          status: 'confirmed',
          target: { kind: 'ad', ad_id: 'ad-2', ad_title: 'Sauerteigbrot' },
          createdAt: nowSecs - 1200,
          updatedAt: nowSecs - 600,
          unread: false,
          replySyncs: [],
          messages: [
            {
              msgId: 'm1',
              sender: 'me',
              text: 'Noch 2 Brote da?',
              createdAt: nowSecs - 1200,
            },
            {
              msgId: 'm2',
              sender: 'them',
              text: 'Gerne ab 16 Uhr',
              decision: 'confirm',
              createdAt: nowSecs - 600,
            },
          ],
        },
      ],
    };

    render(
      <MineTab
        myProfile={mockMyProfile}
        inbox={inboxWithConfirmedSent}
        onOpenProfileEditor={vi.fn()}
        onOpenShareCard={vi.fn()}
        onTogglePublished={vi.fn()}
        onCreateAd={vi.fn()}
        onEditAd={vi.fn()}
        onRenewAd={vi.fn()}
        onDeleteAd={vi.fn()}
        onPayInquiry={onPayInquiry}
      />
    );

    const payBtn = screen.getByTestId('inquiry-pay-btn-inq-sent-1');
    fireEvent.click(payBtn);
    expect(onPayInquiry).toHaveBeenCalledWith('did:key:z6MkuSeller', undefined);
  });
});
