import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { AdCard, clearAdImageCache, resolveAuthorHops } from '../AdCard';
import { Ad } from '../../../types';
import { marketService } from '../../../services/marketService';

describe('AdCard Component', () => {
  beforeEach(() => {
    clearAdImageCache();
    vi.restoreAllMocks();
  });
  const nowSecs = Math.floor(Date.now() / 1000);

  const mockAdOffer: Ad = {
    id: 'ad-1',
    kind: 'offer',
    title: 'Apfelsaft frisch gepresst',
    text: '5 Liter Bag-in-Box Bio-Apfelsaft aus Streuobstanbau',
    pricing: '10 Minuto',
    publishedAt: nowSecs - 3600,
    expiresAt: nowSecs + 86400 * 30,
    allowInquiries: true,
  };

  const mockAdRequest: Ad = {
    id: 'ad-2',
    kind: 'request',
    title: 'Suche Anhänger für Samstag',
    text: 'Brauche einen PKW-Anhänger für Grünschnitt',
    publishedAt: nowSecs - 7200,
    expiresAt: nowSecs + 86400 * 5,
    allowInquiries: true,
  };

  it('renders offer badge, title, text, pricing, owner, and days left', () => {
    const onOwnerClick = vi.fn();
    render(
      <AdCard
        ad={mockAdOffer}
        ownerDid="did:key:z6MkuAlice"
        ownerName="Alice Imkerei"
        trust="trusted"
        onOwnerClick={onOwnerClick}
      />
    );

    expect(screen.getByText(/Angebot|Offer/i)).toBeInTheDocument();
    expect(screen.getByText('Apfelsaft frisch gepresst')).toBeInTheDocument();
    expect(screen.getByText('5 Liter Bag-in-Box Bio-Apfelsaft aus Streuobstanbau')).toBeInTheDocument();
    expect(screen.getByText('10 Minuto')).toBeInTheDocument();
    expect(screen.getByText('Alice Imkerei')).toBeInTheDocument();
    const daysBadge = screen.getByText(/noch 30 Tage|30 days left/i).parentElement;
    expect(daysBadge).toBeInTheDocument();
    expect(daysBadge).not.toHaveClass('bg-amber-50');

    // Click owner
    fireEvent.click(screen.getByText('Alice Imkerei'));
    expect(onOwnerClick).toHaveBeenCalledWith('did:key:z6MkuAlice');
  });

  it('renders request badge with warning color when <= 14 days left', () => {
    render(
      <AdCard
        ad={mockAdRequest}
        ownerDid="did:key:z6MkuBob"
        ownerName="Bob Builder"
      />
    );

    expect(screen.getByText(/Gesuch|Request/i)).toBeInTheDocument();
    expect(screen.getByText('Suche Anhänger für Samstag')).toBeInTheDocument();
    const warningBadge = screen.getByText(/noch 5 Tage|5 days left/i).parentElement;
    expect(warningBadge).toBeInTheDocument();
    expect(warningBadge).toHaveClass('bg-amber-50', 'text-amber-800');
  });

  it('handles inquire and pay buttons when enabled', () => {
    const onInquire = vi.fn();
    const onPay = vi.fn();

    render(
      <AdCard
        ad={mockAdOffer}
        ownerDid="did:key:z6MkuAlice"
        ownerName="Alice Imkerei"
        allowInquiries={true}
        onInquire={onInquire}
        onPay={onPay}
      />
    );

    const inquireBtn = screen.getByRole('button', { name: /Anfragen|Inquire/i });
    fireEvent.click(inquireBtn);
    expect(onInquire).toHaveBeenCalledWith(mockAdOffer, 'did:key:z6MkuAlice', 'Alice Imkerei');

    const payBtn = screen.getByRole('button', { name: /Gutschein senden|Send voucher/i });
    fireEvent.click(payBtn);
    expect(onPay).toHaveBeenCalledWith('did:key:z6MkuAlice');
  });

  it('renders remote badge when ad is remote', () => {
    render(
      <AdCard
        ad={{ ...mockAdOffer, isRemote: true }}
        ownerDid="did:key:z6MkuAlice"
        ownerName="Alice Imkerei"
      />
    );

    expect(screen.getByTestId('ad-remote-badge')).toBeInTheDocument();
    expect(screen.getByText(/Ortsunabhängig|Location-independent/i)).toBeInTheDocument();
  });

  it('renders ad-card-image when getAdImage returns a data URI', async () => {
    const mockDataUri = 'data:image/webp;base64,UklGRkAAAABXRUJQVlA4IDQAAADwAQCdASoAA';
    vi.spyOn(marketService, 'getAdImage').mockResolvedValue(mockDataUri);

    render(
      <AdCard
        ad={{ ...mockAdOffer, imageToken: 'img-tok-123' }}
        ownerDid="did:key:z6MkuAlice"
        ownerName="Alice Imkerei"
      />
    );

    const imageElement = await screen.findByTestId('ad-card-image');
    expect(imageElement).toBeInTheDocument();
    expect(imageElement).toHaveAttribute('src', mockDataUri);
    expect(marketService.getAdImage).toHaveBeenCalledWith('did:key:z6MkuAlice', 'img-tok-123');
  });

  it('renders ad-card-image-unavailable ("Bild nicht mehr verfügbar") when getAdImage returns null', async () => {
    vi.spyOn(marketService, 'getAdImage').mockResolvedValue(null);

    render(
      <AdCard
        ad={{ ...mockAdOffer, imageToken: 'img-tok-404' }}
        ownerDid="did:key:z6MkuAlice"
        ownerName="Alice Imkerei"
      />
    );

    const unavailableElement = await screen.findByTestId('ad-card-image-unavailable');
    expect(unavailableElement).toBeInTheDocument();
    expect(screen.getByText(/Bild nicht mehr verfügbar|Image no longer available/i)).toBeInTheDocument();
  });

  it('switches to ad-card-image-unavailable when <img onError> fires', async () => {
    const mockDataUri = 'data:image/webp;base64,broken';
    vi.spyOn(marketService, 'getAdImage').mockResolvedValue(mockDataUri);

    render(
      <AdCard
        ad={{ ...mockAdOffer, imageToken: 'img-tok-broken' }}
        ownerDid="did:key:z6MkuAlice"
        ownerName="Alice Imkerei"
      />
    );

    const imageElement = await screen.findByTestId('ad-card-image');
    fireEvent.error(imageElement);

    const unavailableElement = await screen.findByTestId('ad-card-image-unavailable');
    expect(unavailableElement).toBeInTheDocument();
    expect(screen.queryByTestId('ad-card-image')).not.toBeInTheDocument();
  });

  it('handles own ads correctly: displays myAd badge, hides inquiry/pay, renders edit button, passes isOwn to onOwnerClick', () => {
    const onEditOwnAd = vi.fn();
    const onInquire = vi.fn();
    const onPay = vi.fn();
    const onOwnerClick = vi.fn();

    render(
      <AdCard
        ad={mockAdOffer}
        ownerDid="did:key:z6MkuAlice"
        ownerName="Alice"
        isOwn={true}
        onEditOwnAd={onEditOwnAd}
        onInquire={onInquire}
        onPay={onPay}
        onOwnerClick={onOwnerClick}
      />
    );

    expect(screen.getByTestId('ad-own-badge')).toBeInTheDocument();
    expect(screen.getByText(/Mein Inserat|My Ad/i)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Anfragen|Inquire/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Bezahlen|Pay/i })).not.toBeInTheDocument();

    const editBtn = screen.getByTestId('ad-edit-own-btn');
    expect(editBtn).toBeInTheDocument();
    fireEvent.click(editBtn);
    expect(onEditOwnAd).toHaveBeenCalledWith(mockAdOffer);

    // Clicking owner button passes isOwn = true
    const ownerBtn = screen.getByRole('button', { name: /Mein Angebot|My Offer|Alice/i });
    fireEvent.click(ownerBtn);
    expect(onOwnerClick).toHaveBeenCalledWith('did:key:z6MkuAlice', true);
  });

  it('defers image fetch when authorHops exceeds mediaAutoLoadMaxHops and loads on click', async () => {
    const mockDataUri = 'data:image/webp;base64,deferredImgData';
    const getAdImageSpy = vi.spyOn(marketService, 'getAdImage').mockResolvedValue(mockDataUri);

    render(
      <AdCard
        ad={{ ...mockAdOffer, imageToken: 'img-tok-hop3' }}
        ownerDid="did:key:z6MkuFarUser"
        ownerName="Far Trader"
        authorHops={3}
        mediaAutoLoadMaxHops={2}
      />
    );

    expect(screen.getByTestId('ad-card-image-placeholder')).toBeInTheDocument();
    expect(screen.getByTestId('ad-card-load-image-btn')).toBeInTheDocument();
    expect(getAdImageSpy).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId('ad-card-load-image-btn'));

    const loadedImg = await screen.findByTestId('ad-card-image');
    expect(loadedImg).toHaveAttribute('src', mockDataUri);
    expect(getAdImageSpy).toHaveBeenCalledWith('did:key:z6MkuFarUser', 'img-tok-hop3');
  });

  it('defers image fetch when wifiOnly is true and connection is cellular', async () => {
    const mockDataUri = 'data:image/webp;base64,cellularImgData';
    const getAdImageSpy = vi.spyOn(marketService, 'getAdImage').mockResolvedValue(mockDataUri);
    const originalDescriptor = Object.getOwnPropertyDescriptor(navigator, 'connection');
    Object.defineProperty(navigator, 'connection', {
      configurable: true,
      value: { type: 'cellular', saveData: false },
    });

    try {
      render(
        <AdCard
          ad={{ ...mockAdOffer, imageToken: 'img-tok-cell' }}
          ownerDid="did:key:z6MkuCellUser"
          ownerName="Cell User"
          authorHops={1}
          mediaAutoLoadMaxHops={2}
          wifiOnly={true}
        />
      );

      expect(screen.getByTestId('ad-card-image-placeholder')).toBeInTheDocument();
      expect(getAdImageSpy).not.toHaveBeenCalled();

      fireEvent.click(screen.getByTestId('ad-card-load-image-btn'));
      const loadedImg = await screen.findByTestId('ad-card-image');
      expect(loadedImg).toHaveAttribute('src', mockDataUri);
    } finally {
      if (originalDescriptor) {
        Object.defineProperty(navigator, 'connection', originalDescriptor);
      } else {
        delete (navigator as unknown as Record<string, unknown>).connection;
      }
    }
  });

  it('bounds adImageCache at 128 entries by evicting the oldest entry on overflow', async () => {
    const getAdImageSpy = vi
      .spyOn(marketService, 'getAdImage')
      .mockImplementation(async (_owner, token) => `data:image/webp;base64,${token}`);

    for (let i = 0; i < 129; i += 1) {
      const { unmount } = render(
        <AdCard
          ad={{ ...mockAdOffer, id: `ad-${i}`, imageToken: `tok-${i}` }}
          ownerDid="did:key:z6MkuAlice"
          ownerName="Alice"
        />
      );
      await screen.findByTestId('ad-card-image');
      unmount();
    }

    expect(getAdImageSpy).toHaveBeenCalledTimes(129);

    // Entry 1 should still be cached (no refetch); entry 0 was evicted and must refetch.
    const { unmount: u1 } = render(
      <AdCard
        ad={{ ...mockAdOffer, id: 'ad-1', imageToken: 'tok-1' }}
        ownerDid="did:key:z6MkuAlice"
        ownerName="Alice"
      />
    );
    await screen.findByTestId('ad-card-image');
    u1();
    expect(getAdImageSpy).toHaveBeenCalledTimes(129);

    render(
      <AdCard
        ad={{ ...mockAdOffer, id: 'ad-0', imageToken: 'tok-0' }}
        ownerDid="did:key:z6MkuAlice"
        ownerName="Alice"
      />
    );
    await screen.findByTestId('ad-card-image');
    expect(getAdImageSpy).toHaveBeenCalledTimes(130);
  });

  it('renders Event badge and formatted event date instead of daysLeft', () => {
    const eventAd: Ad = {
      id: 'ad-event-1',
      kind: 'event',
      title: 'Tauschmarkt am Rathaus',
      text: 'Mit Kaffee und Kuchen',
      publishedAt: nowSecs - 100,
      expiresAt: nowSecs + 86400 * 10,
      allowInquiries: true,
      eventAt: Date.UTC(2026, 11, 15, 12, 0, 0) / 1000,
      eventAllDay: true,
    };

    render(
      <AdCard
        ad={eventAd}
        ownerDid="did:key:z6MkuAlice"
        ownerName="Alice"
      />
    );

    expect(screen.getByTestId('ad-kind-badge')).toHaveTextContent(/Termin|Event/i);
    expect(screen.getByTestId('ad-event-date-badge')).toBeInTheDocument();
    expect(screen.queryByText(/noch \d+ Tage|days left/i)).not.toBeInTheDocument();
  });

  it('does not auto-fetch a new deferred image when imageToken changes after a previous manual load', async () => {
    const getAdImageSpy = vi
      .spyOn(marketService, 'getAdImage')
      .mockImplementation(async (_owner, token) => `data:image/webp;base64,${token}`);

    const { rerender } = render(
      <AdCard
        ad={{ ...mockAdOffer, imageToken: 'tok-a' }}
        ownerDid="did:key:z6MkuFarUser"
        ownerName="Far Trader"
        authorHops={3}
        mediaAutoLoadMaxHops={2}
      />
    );

    expect(screen.getByTestId('ad-card-image-placeholder')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('ad-card-load-image-btn'));

    const loadedA = await screen.findByTestId('ad-card-image');
    expect(loadedA).toHaveAttribute('src', 'data:image/webp;base64,tok-a');
    expect(getAdImageSpy).toHaveBeenCalledTimes(1);
    expect(getAdImageSpy).toHaveBeenCalledWith('did:key:z6MkuFarUser', 'tok-a');

    rerender(
      <AdCard
        ad={{ ...mockAdOffer, imageToken: 'tok-b' }}
        ownerDid="did:key:z6MkuFarUser"
        ownerName="Far Trader"
        authorHops={3}
        mediaAutoLoadMaxHops={2}
      />
    );

    expect(getAdImageSpy).toHaveBeenCalledTimes(1);
    expect(screen.queryByTestId('ad-card-image')).not.toBeInTheDocument();
    expect(screen.getByTestId('ad-card-image-placeholder')).toBeInTheDocument();
  });

  it('computes minimum hop distance cleanly in resolveAuthorHops', () => {
    expect(resolveAuthorHops(undefined)).toBe(99);
    expect(resolveAuthorHops([])).toBe(99);
    expect(resolveAuthorHops([{ kind: 'card', hops: 3 }, { kind: 'voucher' }])).toBe(1);
    expect(resolveAuthorHops([{ kind: 'card', hops: 3 }, { kind: 'card', hops: 2 }])).toBe(2);
  });

  it('hides inquire button when ad.allowInquiries is false even when allowInquiries prop is true', () => {
    const onInquire = vi.fn();
    const onPay = vi.fn();

    render(
      <AdCard
        ad={{ ...mockAdOffer, allowInquiries: false }}
        ownerDid="did:key:z6MkuAlice"
        ownerName="Alice Imkerei"
        allowInquiries={true}
        onInquire={onInquire}
        onPay={onPay}
      />
    );

    expect(screen.queryByRole('button', { name: /Anfragen|Inquire/i })).not.toBeInTheDocument();
    const payBtn = screen.getByRole('button', { name: /Gutschein senden|Send voucher/i });
    fireEvent.click(payBtn);
    expect(onPay).toHaveBeenCalledWith('did:key:z6MkuAlice');
  });

  it('toggles long description text between clamped and expanded states', () => {
    const longText = 'A'.repeat(110);
    render(
      <AdCard
        ad={{ ...mockAdOffer, text: longText }}
        ownerDid="did:key:z6MkuAlice"
        ownerName="Alice"
      />
    );

    const paragraph = screen.getByText(longText);
    expect(paragraph).toHaveClass('line-clamp-2');

    fireEvent.click(screen.getByRole('button', { name: /Mehr anzeigen|Show more/i }));
    expect(paragraph).not.toHaveClass('line-clamp-2');

    fireEvent.click(screen.getByRole('button', { name: /Weniger anzeigen|Show less/i }));
    expect(paragraph).toHaveClass('line-clamp-2');
  });

  it('renders singular one-day label, expired zero-day label, and fallback owner display names', () => {
    const { rerender } = render(
      <AdCard
        ad={{ ...mockAdOffer, expiresAt: nowSecs - 10 }}
        ownerDid="did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH"
        ownerName="   "
        daysLeft={1}
      />
    );

    expect(screen.getByText(/^noch 1 Tag$|^1 day left$/i)).toBeInTheDocument();
    expect(screen.getByText('did:key:z6MkpTHR8VNsBxYAAWHu...dvktH')).toBeInTheDocument();

    rerender(
      <AdCard
        ad={{ ...mockAdOffer, expiresAt: nowSecs - 10 }}
        ownerDid="did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH"
        ownerName="did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH"
      />
    );

    expect(screen.getByText(/noch 0 Tage|0 days left/i)).toBeInTheDocument();
    expect(screen.getByText('did:key:z6MkpTHR8VNsBxYAAWHu...dvktH')).toBeInTheDocument();
  });

  it('handles image fetch rejection and auto-loads own ads despite wifiOnly on cellular connection', async () => {
    const getAdImageSpy = vi
      .spyOn(marketService, 'getAdImage')
      .mockRejectedValueOnce(new Error('disk error'))
      .mockResolvedValueOnce('data:image/webp;base64,ownImgData');

    const { rerender } = render(
      <AdCard
        ad={{ ...mockAdOffer, imageToken: 'img-tok-reject' }}
        ownerDid="did:key:z6MkuAlice"
        ownerName="Alice"
      />
    );

    expect(await screen.findByTestId('ad-card-image-unavailable')).toBeInTheDocument();

    const originalDescriptor = Object.getOwnPropertyDescriptor(navigator, 'connection');
    Object.defineProperty(navigator, 'connection', {
      configurable: true,
      value: { type: 'cellular', saveData: false },
    });

    try {
      rerender(
        <AdCard
          ad={{ ...mockAdOffer, imageToken: 'img-tok-own' }}
          ownerDid="did:key:z6MkuAlice"
          ownerName="Alice"
          isOwn={true}
          wifiOnly={true}
        />
      );

      const ownImg = await screen.findByTestId('ad-card-image');
      expect(ownImg).toHaveAttribute('src', 'data:image/webp;base64,ownImgData');
      expect(getAdImageSpy).toHaveBeenCalledTimes(2);
    } finally {
      if (originalDescriptor) {
        Object.defineProperty(navigator, 'connection', originalDescriptor);
      } else {
        delete (navigator as unknown as Record<string, unknown>).connection;
      }
    }
  });
});
