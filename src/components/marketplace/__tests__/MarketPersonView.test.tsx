import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { MarketPersonView } from '../MarketPersonView';
import { FrontendMarketPerson } from '../../../types';
import { marketService } from '../../../services/marketService';

vi.mock('../../../services/marketService', () => ({
  marketService: {
    syncMarketDrops: vi.fn().mockResolvedValue(undefined),
    getMarketView: vi.fn(),
    exportForwardCard: vi.fn(),
    previewContactTrustChange: vi.fn().mockResolvedValue({ newlyAccessibleMembers: 0, newlyExcludedMembers: 0 }),
  },
}));

vi.mock('../../../services/contactService', () => ({
  contactService: {
    getContacts: vi.fn().mockResolvedValue([]),
    saveContact: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('../../../context/NavigationContext', () => ({
  useNavigation: () => ({
    navigate: vi.fn(),
  }),
}));

vi.mock('../../../context/SessionContext', () => ({
  useSession: () => ({
    protectAction: vi.fn(async (cb: (pw?: string) => Promise<any>) => cb('mock-password')),
  }),
}));

describe('MarketPersonView Component', () => {
  const basePerson: FrontendMarketPerson = {
    did: 'did:key:z6MkuBob456',
    profile: {
      id: 'bob-id',
      firstName: 'Bob',
      lastName: 'Builder',
      organization: 'Bob Co',
      community: 'Builders',
    },
    trust: 'trusted',
    cardDrops: [],
    sources: [{ kind: 'card', hops: 1 }],
    standards: [],
    listings: [],
    asOf: 1700000000,
    canForward: true,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders waiting for mutual handshake notice when listings are empty but cardDrops exist', () => {
    const personWithDrops: FrontendMarketPerson = {
      ...basePerson,
      cardDrops: [
        {
          readUrl: 'https://bob-sync.org/sync.php',
          writeUrl: 'https://bob-sync.org/sync.php',
          dropPub: 'bob-drop-pub',
        },
      ],
    };

    render(<MarketPersonView person={personWithDrops} onBack={vi.fn()} />);

    expect(screen.getByText(/Kontakt gespeichert – Freischaltung ausstehend|Contact saved – pending mutual unlock/i)).toBeInTheDocument();
    expect(screen.getByText(/Inserate werden geladen, sobald euch eure Adressbücher gegenseitig freigeschaltet haben|Listings will load as soon as both address books/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Jetzt abrufen|Fetch now/i })).toBeInTheDocument();
  });

  it('triggers syncMarketDrops when clicking [Jetzt abrufen]', async () => {
    const personWithDrops: FrontendMarketPerson = {
      ...basePerson,
      cardDrops: [
        {
          readUrl: 'https://bob-sync.org/sync.php',
          writeUrl: 'https://bob-sync.org/sync.php',
          dropPub: 'bob-drop-pub',
        },
      ],
    };

    vi.mocked(marketService.getMarketView).mockResolvedValue([personWithDrops]);

    render(<MarketPersonView person={personWithDrops} onBack={vi.fn()} />);

    const fetchBtn = screen.getByRole('button', { name: /Jetzt abrufen|Fetch now/i });
    fireEvent.click(fetchBtn);

    await waitFor(() => {
      expect(marketService.syncMarketDrops).toHaveBeenCalled();
      expect(marketService.getMarketView).toHaveBeenCalled();
    });
  });

  it('renders default acceptsOwnVouchers when listings are empty and no cardDrops exist', () => {
    const personWithoutDrops: FrontendMarketPerson = {
      ...basePerson,
      cardDrops: [],
    };

    render(<MarketPersonView person={personWithoutDrops} onBack={vi.fn()} />);

    expect(screen.queryByText(/Freischaltung ausstehend|pending mutual unlock/i)).not.toBeInTheDocument();
    expect(screen.getByText(/Nimmt eigene Gutscheine an|Accepts own vouchers/i)).toBeInTheDocument();
  });

  it('renders listings when person has offers or requests', () => {
    const personWithListings: FrontendMarketPerson = {
      ...basePerson,
      listings: [
        {
          listingId: 'listing-1',
          listingType: 'offer',
          title: 'Frisches Holz',
          text: 'Eichenholz getrocknet',
          pricing: '50 Minuto',
          origin: 'card',
          allowInquiries: true,
        },
      ],
    };

    render(<MarketPersonView person={personWithListings} onBack={vi.fn()} />);

    expect(screen.getByText('Frisches Holz')).toBeInTheDocument();
    expect(screen.getByText('50 Minuto')).toBeInTheDocument();
  });
});
