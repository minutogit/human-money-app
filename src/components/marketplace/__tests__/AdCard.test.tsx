import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { AdCard, clearAdImageCacheForTests } from '../AdCard';
import { Ad } from '../../../types';
import { marketService } from '../../../services/marketService';

describe('AdCard Component', () => {
  beforeEach(() => {
    clearAdImageCacheForTests();
    vi.restoreAllMocks();
  });
  const nowSecs = Math.floor(Date.now() / 1000);

  const mockAdOffer: Ad = {
    id: 'ad-1',
    kind: 'offer',
    title: 'Apfelsaft frisch gepresst',
    text: '5 Liter Bag-in-Box Bio-Apfelsaft aus Streuobstanbau',
    pricing: '10 Minuto',
    published_at: nowSecs - 3600,
    expires_at: nowSecs + 86400 * 30,
    allow_inquiries: true,
  };

  const mockAdRequest: Ad = {
    id: 'ad-2',
    kind: 'request',
    title: 'Suche Anhänger für Samstag',
    text: 'Brauche einen PKW-Anhänger für Grünschnitt',
    published_at: nowSecs - 7200,
    expires_at: nowSecs + 86400 * 5,
    allow_inquiries: true,
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
    expect(screen.getByText(/noch 30 Tage|30 days left/i)).toBeInTheDocument();

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
    expect(screen.getByText(/noch 5 Tage|5 days left/i)).toBeInTheDocument();
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
        ad={{ ...mockAdOffer, is_remote: true }}
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
        ad={{ ...mockAdOffer, image_token: 'img-tok-123' }}
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
        ad={{ ...mockAdOffer, image_token: 'img-tok-404' }}
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
        ad={{ ...mockAdOffer, image_token: 'img-tok-broken' }}
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
});
