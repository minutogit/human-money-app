import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { MarketPersonView } from '../MarketPersonView';
import { FrontendMarketPerson } from '../../../types';
import { marketService } from '../../../services/marketService';

vi.mock('../../../services/marketService', () => ({
  marketService: {
    syncMarketChannels: vi.fn().mockResolvedValue(undefined),
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
    skills: [],
    activeAds: [],
    adsPending: 0,
    allowInquiries: true,
    sources: [{ kind: 'card', hops: 1 }],
    standards: [],
    cardSyncs: [],
    asOf: 1700000000,
    canForward: true,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders pending sync notice when adsPending > 0 and triggers sync when clicked', async () => {
    const personPending: FrontendMarketPerson = {
      ...basePerson,
      adsPending: 3,
      cardSyncs: [
        {
          readUrl: 'https://bob-sync.org/sync.php',
          writeUrl: 'https://bob-sync.org/sync.php',
          dropPub: 'bob-drop-pub',
        },
      ],
    };

    vi.mocked(marketService.getMarketView).mockResolvedValue({
      board: [],
      people: [personPending],
      now: 123456,
    });

    render(<MarketPersonView person={personPending} onBack={vi.fn()} />);

    expect(screen.getByText(/3 Inserate werden über den Sync-Dienst nachgeladen|3 ads will be loaded via sync service/i)).toBeInTheDocument();
    const fetchBtn = screen.getByRole('button', { name: /Jetzt abrufen|Fetch now/i });
    expect(fetchBtn).toBeInTheDocument();

    fireEvent.click(fetchBtn);
    await waitFor(() => {
      expect(marketService.syncMarketChannels).toHaveBeenCalled();
      expect(marketService.getMarketView).toHaveBeenCalled();
    });
  });

  it('renders skills when person has skills', () => {
    const personWithSkills: FrontendMarketPerson = {
      ...basePerson,
      skills: [
        {
          title: 'Holzarbeiten',
          text: 'Tische, Bänke und Regale aus Vollholz',
          pricing: '45 Minuto / Std',
          origin: 'card',
        },
      ],
    };

    render(<MarketPersonView person={personWithSkills} onBack={vi.fn()} />);

    expect(screen.getByText('Holzarbeiten')).toBeInTheDocument();
    expect(screen.getByText('Tische, Bänke und Regale aus Vollholz')).toBeInTheDocument();
    expect(screen.getByText('45 Minuto / Std')).toBeInTheDocument();
  });

  it('renders active ads with AdCard when person has active ads', () => {
    const nowSecs = Math.floor(Date.now() / 1000);
    const personWithAds: FrontendMarketPerson = {
      ...basePerson,
      activeAds: [
        {
          id: 'ad-1',
          kind: 'offer',
          title: 'Frisches Eichenbrennholz',
          text: '2 Ster trockenes Holz',
          pricing: '120 Minuto',
          published_at: nowSecs - 3600,
          expires_at: nowSecs + 86400 * 30,
          allow_inquiries: true,
        },
      ],
    };

    render(<MarketPersonView person={personWithAds} onBack={vi.fn()} />);

    expect(screen.getByText('Frisches Eichenbrennholz')).toBeInTheDocument();
    expect(screen.getByText('2 Ster trockenes Holz')).toBeInTheDocument();
    expect(screen.getByText('120 Minuto')).toBeInTheDocument();
  });

  it('renders subtle note when person.profile.needs is present (Maintainer Decision 7)', () => {
    const personWithNeeds: FrontendMarketPerson = {
      ...basePerson,
      profile: {
        ...basePerson.profile,
        needs: 'Hilfe bei der Obsternte im Herbst',
      },
    };

    render(<MarketPersonView person={personWithNeeds} onBack={vi.fn()} />);

    expect(screen.getByText(/(Laut Gutschein gesucht:|Sought according to voucher:)/i)).toBeInTheDocument();
    expect(screen.getByText('Hilfe bei der Obsternte im Herbst')).toBeInTheDocument();
  });

  it('renders remote badge when skill is remote', () => {
    const personWithRemoteSkill: FrontendMarketPerson = {
      ...basePerson,
      skills: [
        {
          title: 'Online Nachhilfe',
          text: 'Mathematik und Physik via Video',
          origin: 'card',
          isRemote: true,
        },
      ],
    };

    render(<MarketPersonView person={personWithRemoteSkill} onBack={vi.fn()} />);

    expect(screen.getByText('Online Nachhilfe')).toBeInTheDocument();
    expect(screen.getByTestId('skill-remote-badge')).toBeInTheDocument();
    expect(screen.getByText(/Ortsunabhängig|Location-independent/i)).toBeInTheDocument();
  });
});
