import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { MineTab } from '../MineTab';
import { UiModeProvider } from '../../../context/UiModeContext';
import {
  FrontendMyMarketProfile,
  FrontendPostInboxSummary,
} from '../../../types';

describe('MineTab Component', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
  });

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
        published: true,
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
        published: true,
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
        target: { kind: 'ad', adId: 'ad-1', adTitle: 'IT-Beratung kostenlos' },
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

  it('renders business card preview with publish toggle and share trigger', () => {
    const onTogglePublished = vi.fn().mockResolvedValue(undefined);
    const onOpenProfileEditor = vi.fn();
    const onOpenShareCard = vi.fn();

    render(
      <MineTab
        myProfile={mockMyProfile}
        inbox={mockInbox}
        onOpenProfileEditor={onOpenProfileEditor}
        onOpenShareCard={onOpenShareCard}
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
    expect(screen.getAllByText(/Veröffentlicht|Published/i).length).toBeGreaterThanOrEqual(1);

    // Edit card button
    const editBtn = screen.getByRole('button', { name: /Visitenkarte bearbeiten|Edit business card/i });
    fireEvent.click(editBtn);
    expect(onOpenProfileEditor).toHaveBeenCalledTimes(1);

    // Share card button & publish toggle
    fireEvent.click(screen.getByRole('button', { name: /^Teilen$|^Share$/i }));
    expect(onOpenShareCard).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByTestId('my-card-publish-checkbox'));
    expect(onTogglePublished).toHaveBeenCalledWith(false);
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
          target: { kind: 'ad', adId: 'ad-2', adTitle: 'Sauerteigbrot' },
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

  it('hides renew buttons on active Event ads and offers reschedule button on expired Event ads', () => {
    const onEditAd = vi.fn();
    const profileWithEvents: FrontendMyMarketProfile = {
      ...mockMyProfile,
      ads: [
        {
          id: 'ev-active-1',
          kind: 'event',
          title: 'Aktiver Stammtisch',
          publishedAt: nowSecs - 3600,
          expiresAt: nowSecs + 86400 * 5,
          allowInquiries: true,
          status: 'active',
          daysLeft: 5,
          published: true,
          eventAt: nowSecs + 86400 * 5,
          eventAllDay: false,
        },
        {
          id: 'ev-expired-1',
          kind: 'event',
          title: 'Alter Flohmarkt',
          publishedAt: nowSecs - 86400 * 10,
          expiresAt: nowSecs - 86400,
          allowInquiries: true,
          status: 'expired',
          daysLeft: 0,
          published: true,
          eventAt: nowSecs - 86400 * 2,
          eventAllDay: true,
        },
      ],
    };

    render(
      <UiModeProvider initialMode="expert">
        <MineTab
          myProfile={profileWithEvents}
          inbox={mockInbox}
          onOpenProfileEditor={vi.fn()}
          onOpenShareCard={vi.fn()}
          onTogglePublished={vi.fn()}
          onCreateAd={vi.fn()}
          onEditAd={onEditAd}
          onQuickRenewAd={vi.fn()}
          onRenewAd={vi.fn()}
          onDeleteAd={vi.fn()}
        />
      </UiModeProvider>
    );

    expect(screen.queryByTestId('quick-renew-ad-ev-active-1')).not.toBeInTheDocument();
    expect(screen.queryByTestId('custom-renew-ad-ev-active-1')).not.toBeInTheDocument();

    // Expand expired section
    fireEvent.click(screen.getByText(/Abgelaufene Inserate \(1\)|Expired ads \(1\)/i));
    expect(screen.queryByTestId('quick-renew-ad-ev-expired-1')).not.toBeInTheDocument();
    const rescheduleBtn = screen.getByTestId('edit-expired-ad-ev-expired-1');
    fireEvent.click(rescheduleBtn);
    expect(onEditAd).toHaveBeenCalledWith(expect.objectContaining({ id: 'ev-expired-1' }));
  });

  it('separates drafts and published ads clearly, shows unpublished card hint, and triggers toggle callbacks', () => {
    const onToggleAdPublished = vi.fn();
    const profileMixed: FrontendMyMarketProfile = {
      ...mockMyProfile,
      published: false,
      ads: [
        {
          id: 'ad-pub-1',
          kind: 'offer',
          title: 'Bereits aktiviertes Inserat',
          publishedAt: nowSecs - 1000,
          expiresAt: nowSecs + 86400 * 30,
          allowInquiries: true,
          status: 'active',
          daysLeft: 30,
          published: true,
        },
        {
          id: 'ad-draft-1',
          kind: 'request',
          title: 'Mein Entwurf',
          publishedAt: nowSecs - 500,
          expiresAt: nowSecs + 86400 * 90,
          allowInquiries: true,
          status: 'active',
          daysLeft: 90,
          published: false,
        },
      ],
    };

    render(
      <MineTab
        myProfile={profileMixed}
        inbox={mockInbox}
        onOpenProfileEditor={vi.fn()}
        onOpenShareCard={vi.fn()}
        onTogglePublished={vi.fn()}
        onCreateAd={vi.fn()}
        onEditAd={vi.fn()}
        onToggleAdPublished={onToggleAdPublished}
        onRenewAd={vi.fn()}
        onDeleteAd={vi.fn()}
      />
    );

    expect(screen.getByTestId('ads-hidden-card-unpublished-hint')).toBeInTheDocument();
    expect(screen.getByTestId('published-ads-section')).toBeInTheDocument();
    expect(screen.getByTestId('draft-ads-section')).toBeInTheDocument();
    expect(screen.getByTestId('ad-draft-badge-ad-draft-1')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('publish-ad-ad-draft-1'));
    expect(onToggleAdPublished).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'ad-draft-1' }),
      true,
    );

    fireEvent.click(screen.getByTestId('unpublish-ad-ad-pub-1'));
    expect(onToggleAdPublished).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'ad-pub-1' }),
      false,
    );
  });

  it('confirms ad deletion in modal, disables buttons while in flight, and respects Cancel', async () => {
    let resolveDelete!: () => void;
    const onDeleteAd = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveDelete = resolve;
        })
    );

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
        onDeleteAd={onDeleteAd}
      />
    );

    const activeRow = screen.getByTestId('my-active-ad-ad-active-1');
    fireEvent.click(within(activeRow).getByTitle(/Löschen|Delete/i));

    expect(screen.getByText(/Diesen Eintrag wirklich dauerhaft löschen\?|Really delete this item permanently\?/i)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /^Abbrechen$|^Cancel$/i }));
    expect(onDeleteAd).not.toHaveBeenCalled();
    expect(screen.queryByText(/Diesen Eintrag wirklich dauerhaft löschen\?|Really delete this item permanently\?/i)).not.toBeInTheDocument();

    fireEvent.click(within(activeRow).getByTitle(/Löschen|Delete/i));
    const dialog = screen.getByRole('dialog');
    const confirmBtn = within(dialog).getByRole('button', { name: /^Löschen$|^Delete$/i });
    fireEvent.click(confirmBtn);

    expect(onDeleteAd).toHaveBeenCalledWith('ad-active-1');
    expect(within(dialog).getByRole('button', { name: /^Abbrechen$|^Cancel$/i })).toBeDisabled();

    resolveDelete();
    await waitFor(() => {
      expect(screen.queryByText(/Diesen Eintrag wirklich dauerhaft löschen\?|Really delete this item permanently\?/i)).not.toBeInTheDocument();
    });
  });

  it('renders empty ads and empty inquiries states and wires create and edit triggers', () => {
    const onCreateAd = vi.fn();
    const onEditAd = vi.fn();

    const emptyProfile: FrontendMyMarketProfile = {
      ...mockMyProfile,
      ads: [],
    };
    const emptyInbox: FrontendPostInboxSummary = {
      ...mockInbox,
      inquiries: [],
    };

    const { rerender } = render(
      <MineTab
        myProfile={emptyProfile}
        inbox={emptyInbox}
        onOpenProfileEditor={vi.fn()}
        onOpenShareCard={vi.fn()}
        onTogglePublished={vi.fn()}
        onCreateAd={onCreateAd}
        onEditAd={onEditAd}
        onRenewAd={vi.fn()}
        onDeleteAd={vi.fn()}
      />
    );

    expect(screen.getByText(/Du hast aktuell keine aktiven Inserate|You currently have no active ads/i)).toBeInTheDocument();
    expect(screen.getByText(/Noch keine Anfragen vorhanden|No inquiries yet/i)).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('create-ad-btn'));
    expect(onCreateAd).toHaveBeenCalledTimes(1);

    rerender(
      <MineTab
        myProfile={mockMyProfile}
        inbox={emptyInbox}
        onOpenProfileEditor={vi.fn()}
        onOpenShareCard={vi.fn()}
        onTogglePublished={vi.fn()}
        onCreateAd={onCreateAd}
        onEditAd={onEditAd}
        onRenewAd={vi.fn()}
        onDeleteAd={vi.fn()}
      />
    );

    const activeRow = screen.getByTestId('my-active-ad-ad-active-1');
    fireEvent.click(within(activeRow).getByRole('button', { name: /Bearbeiten|Edit/i }));
    expect(onEditAd).toHaveBeenCalledWith(expect.objectContaining({ id: 'ad-active-1' }));
  });

  it('renders expiring-soon singular day badge and supports quick and custom renewal on expired ads', () => {
    const onQuickRenewAd = vi.fn();
    const onRenewAd = vi.fn();
    const expiringSoonProfile: FrontendMyMarketProfile = {
      ...mockMyProfile,
      ads: [
        { ...mockMyProfile.ads[0], daysLeft: 1 },
        mockMyProfile.ads[1],
      ],
    };

    const { rerender } = render(
      <UiModeProvider initialMode="expert">
        <MineTab
          myProfile={expiringSoonProfile}
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

    const dayBadge = screen.getByText(/^noch 1 Tag$|^1 day left$/i).parentElement;
    expect(dayBadge).toHaveClass('bg-amber-50', 'text-amber-800');

    fireEvent.click(screen.getByText(/Abgelaufene Inserate \(1\)|Expired ads \(1\)/i));
    fireEvent.click(screen.getByTestId('quick-renew-ad-ad-expired-1'));
    expect(onQuickRenewAd).toHaveBeenCalledWith(expect.objectContaining({ id: 'ad-expired-1' }));

    fireEvent.click(screen.getByTestId('custom-renew-ad-ad-expired-1'));
    expect(onRenewAd).toHaveBeenCalledWith(expect.objectContaining({ id: 'ad-expired-1' }));

    // Fallback when onQuickRenewAd is omitted
    rerender(
      <UiModeProvider initialMode="expert">
        <MineTab
          myProfile={expiringSoonProfile}
          inbox={mockInbox}
          onOpenProfileEditor={vi.fn()}
          onOpenShareCard={vi.fn()}
          onTogglePublished={vi.fn()}
          onCreateAd={vi.fn()}
          onEditAd={vi.fn()}
          onRenewAd={onRenewAd}
          onDeleteAd={vi.fn()}
        />
      </UiModeProvider>
    );
    fireEvent.click(screen.getByTestId('quick-renew-ad-ad-expired-1'));
    expect(onRenewAd).toHaveBeenLastCalledWith(expect.objectContaining({ id: 'ad-expired-1' }));
  });

  it('renders Reschedule CTA on expired Event drafts invoking onEditAd instead of onToggleAdPublished', () => {
    const onEditAd = vi.fn();
    const onToggleAdPublished = vi.fn();
    const expiredEventDraftProfile: FrontendMyMarketProfile = {
      ...mockMyProfile,
      ads: [
        {
          id: 'ev-draft-expired',
          kind: 'event',
          title: 'Verpasster Workshop',
          publishedAt: nowSecs - 86400 * 10,
          expiresAt: nowSecs - 86400,
          allowInquiries: true,
          status: 'expired',
          daysLeft: 0,
          published: false,
          eventAt: nowSecs - 86400 * 2,
          eventAllDay: true,
        },
      ],
    };

    render(
      <MineTab
        myProfile={expiredEventDraftProfile}
        inbox={mockInbox}
        onOpenProfileEditor={vi.fn()}
        onOpenShareCard={vi.fn()}
        onTogglePublished={vi.fn()}
        onCreateAd={vi.fn()}
        onEditAd={onEditAd}
        onToggleAdPublished={onToggleAdPublished}
        onRenewAd={vi.fn()}
        onDeleteAd={vi.fn()}
      />
    );

    const ctaBtn = screen.getByTestId('publish-ad-ev-draft-expired');
    expect(ctaBtn).toHaveTextContent(/Neu ansetzen|Reschedule/i);
    fireEvent.click(ctaBtn);
    expect(onEditAd).toHaveBeenCalledWith(expect.objectContaining({ id: 'ev-draft-expired' }));
    expect(onToggleAdPublished).not.toHaveBeenCalled();
  });

  it('supports InquiryThreadCard Enter reply, error retention, decline, markRead, dismiss, and no-reply-channel warning', async () => {
    const onSendInquiryMessage = vi
      .fn()
      .mockRejectedValueOnce(new Error('Postbox unreachable'))
      .mockResolvedValueOnce(undefined);
    const onMarkInquiryRead = vi.fn();
    const onDismissInquiry = vi.fn().mockResolvedValue(undefined);

    const { rerender } = render(
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
        onMarkInquiryRead={onMarkInquiryRead}
        onDismissInquiry={onDismissInquiry}
      />
    );

    fireEvent.click(screen.getByTestId('inquiry-thread-inq-1'));
    expect(onMarkInquiryRead).toHaveBeenCalledWith('inq-1');

    const input = screen.getByTestId('inquiry-input-inq-1');
    fireEvent.change(input, { target: { value: 'Passt morgen um 10?' } });
    fireEvent.keyDown(input, { key: 'Enter', shiftKey: false });

    expect(await screen.findByText('Postbox unreachable')).toBeInTheDocument();
    expect(input).toHaveValue('Passt morgen um 10?');

    fireEvent.click(screen.getByTestId('inquiry-decline-btn-inq-1'));
    await waitFor(() => {
      expect(onSendInquiryMessage).toHaveBeenLastCalledWith('inq-1', {
        text: 'Passt morgen um 10?',
        decision: 'decline',
      });
    });

    fireEvent.click(within(screen.getByTestId('inquiry-thread-inq-1')).getByTitle(/Entfernen|Remove/i));
    await waitFor(() => {
      expect(onDismissInquiry).toHaveBeenCalledWith('inq-1');
    });

    const noSyncInbox: FrontendPostInboxSummary = {
      ...mockInbox,
      inquiries: [
        {
          ...mockInbox.inquiries[0],
          inquiryId: 'inq-nosync',
          isContact: false,
          replySyncs: [],
        },
      ],
    };

    rerender(
      <MineTab
        myProfile={mockMyProfile}
        inbox={noSyncInbox}
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

    expect(screen.getByText(/Kein Rückkanal|No reply channel/i)).toBeInTheDocument();
    expect(screen.queryByTestId('inquiry-input-inq-nosync')).not.toBeInTheDocument();
  });
});
