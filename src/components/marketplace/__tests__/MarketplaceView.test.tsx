import { render, screen, waitFor, fireEvent, within } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { MarketplaceView } from '../MarketplaceView';
import { invoke } from '@tauri-apps/api/core';
import {
  FrontendMarketSnapshot,
  FrontendMarketPerson,
  FrontendBoardItem,
  FrontendPostInboxSummary,
  FrontendMyMarketProfile,
} from '../../../types';

const mockNavigate = vi.fn();
const mockGoBack = vi.fn();
const mockNavigateToSettingsSection = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('../../../context/NavigationContext', () => ({
  useNavigation: () => ({
    navigate: mockNavigate,
    goBack: mockGoBack,
    navigateToSettingsSection: mockNavigateToSettingsSection,
    appState: { view: 'marketplace' },
  }),
}));

vi.mock('../../../context/SessionContext', () => ({
  useSession: () => ({
    protectAction: vi.fn((action) => action(undefined)),
    isSessionActive: true,
  }),
}));

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: vi.fn(),
}));

vi.mock('../../../utils/clipboard', () => ({
  copyToClipboard: vi.fn(() => Promise.resolve(true)),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: vi.fn(() => Promise.resolve('/fake/path/card.hmcard')),
  open: vi.fn(() => Promise.resolve('/fake/path/card.hmcard')),
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
  writeFile: vi.fn(() => Promise.resolve()),
  readFile: vi.fn(() => Promise.resolve(new TextEncoder().encode('{"card_jws":"test","hops":1}'))),
}));

describe('MarketplaceView Component', () => {
  const nowSecs = Math.floor(Date.now() / 1000);

  const mockBoardItems: FrontendBoardItem[] = [
    {
      ownerDid: 'did:key:z6MkuAlice',
      ownerName: 'Alice Honey Farm',
      trust: 'trusted',
      ad: {
        id: 'ad-alice-1',
        kind: 'offer',
        title: 'Bio Honey 500g',
        text: 'Fresh summer flower honey from regional bees',
        pricing: '5 FreeTaler',
        publishedAt: nowSecs - 3600,
        expiresAt: nowSecs + 86400 * 30,
        allowInquiries: true,
      },
      daysLeft: 30,
    },
    {
      ownerDid: 'did:key:z6MkuBob',
      ownerName: 'Bob Carpentry',
      trust: 'unknown',
      ad: {
        id: 'ad-bob-1',
        kind: 'request',
        title: 'Suche Eichenbohlen',
        text: 'Trockenes Eichenholz gesucht',
        pricing: 'Guter Tauschwert',
        publishedAt: nowSecs - 7200,
        expiresAt: nowSecs + 86400 * 15,
        allowInquiries: true,
      },
      daysLeft: 15,
    },
  ];

  const mockPeople: FrontendMarketPerson[] = [
    {
      did: 'did:key:z6MkuAlice',
      profile: {
        id: 'did:key:z6MkuAlice',
        firstName: 'Alice',
        lastName: 'Wonderland',
        organization: 'Alice Honey Farm',
        community: 'Bio Community',
        address: { city: 'Bienenstadt' },
      },
      trust: 'trusted',
      skills: [
        {
          title: 'Imkerei & Honig',
          text: 'Regionaler Bio-Honig',
          pricing: '5 FreeTaler',
          origin: 'card',
        },
      ],
      activeAds: [mockBoardItems[0].ad],
      adsPending: 0,
      allowInquiries: true,
      sources: [{ kind: 'card', hops: 1 }],
      standards: [{ name: 'FreeTaler', uuid: 'std-freetaler-uuid' }],
      cardSyncs: [],
      asOf: 1731000000,
      canForward: true,
    },
    {
      did: 'did:key:z6MkuBob',
      profile: {
        id: 'did:key:z6MkuBob',
        firstName: 'Bob',
        lastName: 'Builder',
        organization: 'Bob Carpentry',
      },
      trust: 'unknown',
      skills: [
        {
          title: 'Zimmerei & Holz',
          text: 'Möbel und Dachstühle',
          origin: 'voucher',
        },
      ],
      activeAds: [mockBoardItems[1].ad],
      adsPending: 0,
      allowInquiries: true,
      sources: [{ kind: 'card', hops: 1 }],
      standards: [],
      cardSyncs: [],
      asOf: 1731050000,
      canForward: false,
    },
  ];

  const mockMyProfile: FrontendMyMarketProfile = {
    profile: {
      id: 'did:key:z6MkuSelf',
      firstName: 'Me',
      lastName: 'Self',
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
        title: 'Gartenpflege',
        text: 'Rasen mähen und Hecke schneiden',
        pricing: '30 Minuto / Std',
      },
    ],
    ads: [
      {
        id: 'my-ad-1',
        kind: 'offer',
        title: 'Gartenarbeit am Wochenende',
        text: 'Biete Hilfe im Garten an',
        pricing: '30 Minuto',
        publishedAt: nowSecs - 86400,
        expiresAt: nowSecs + 86400 * 25,
        status: 'active',
        daysLeft: 25,
        allowInquiries: true,
      },
    ],
    maxHops: 2,
    allowInquiries: true,
    published: true,
    shareUri: 'humoco://card?env=self',
    fileWire: '{"card_jws":"self"}',
    qrSvg: '<svg></svg>',
    qrIsPartial: false,
  };

  const mockSnapshot: FrontendMarketSnapshot = {
    board: mockBoardItems,
    people: mockPeople,
    now: nowSecs,
  };

  const mockInbox: FrontendPostInboxSummary = {
    pendingSignRequests: [],
    pendingWotHandshakes: [],
    sellerInquiries: [],
    buyerInquiries: [],
    unacknowledgedPayments: [],
  };

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();

    (invoke as Mock).mockImplementation((cmd: string) => {
      switch (cmd) {
        case 'get_market_view':
          return Promise.resolve(mockSnapshot);
        case 'get_my_market_profile':
          return Promise.resolve(mockMyProfile);
        case 'get_inbox':
        case 'get_post_inbox_summary':
          return Promise.resolve(mockInbox);
        case 'get_contacts':
          return Promise.resolve([]);
        case 'get_settings':
        case 'get_app_settings':
          return Promise.resolve({ marketSelectedSyncs: [{ id: 'sync-1' }] });
        case 'set_card_published':
          return Promise.resolve({
            ...mockMyProfile,
            published: true,
          });
        case 'upsert_ad':
          return Promise.resolve('ad-new-id');
        case 'renew_ad':
          return Promise.resolve();
        case 'delete_ad':
          return Promise.resolve();
        case 'save_my_market_profile':
          return Promise.resolve(mockMyProfile);
        default:
          return Promise.resolve({});
      }
    });
  });

  it('renders loading state initially, then displays board tab by default with ads', async () => {
    render(<MarketplaceView />);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('get_market_view', {});
    });

    // Check header
    expect(screen.getByRole('heading', { level: 1, name: /Marktplatz|Marketplace/i })).toBeInTheDocument();

    // Check segmented tabs
    expect(screen.getByRole('button', { name: /Pinnwand|Board/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Menschen|People/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Meins|Mine/i })).toBeInTheDocument();

    // Board items rendered
    expect(screen.getByText('Bio Honey 500g')).toBeInTheDocument();
    expect(screen.getByText('Suche Eichenbohlen')).toBeInTheDocument();
  });

  it('filters board items by kind chips and search query', async () => {
    render(<MarketplaceView />);

    await waitFor(() => {
      expect(screen.getByText('Bio Honey 500g')).toBeInTheDocument();
    });

    // Filter by offers only
    const offersBtn = screen.getByRole('button', { name: /Nur Angebote|Offers only/i });
    fireEvent.click(offersBtn);

    expect(screen.getByText('Bio Honey 500g')).toBeInTheDocument();
    expect(screen.queryByText('Suche Eichenbohlen')).not.toBeInTheDocument();

    // Filter by requests only
    const requestsBtn = screen.getByRole('button', { name: /Nur Gesuche|Requests only/i });
    fireEvent.click(requestsBtn);

    expect(screen.queryByText('Bio Honey 500g')).not.toBeInTheDocument();
    expect(screen.getByText('Suche Eichenbohlen')).toBeInTheDocument();

    // Reset to all
    const allBtn = screen.getByRole('button', { name: /^Alle$|^All$/i });
    fireEvent.click(allBtn);

    expect(screen.getByText('Bio Honey 500g')).toBeInTheDocument();
    expect(screen.getByText('Suche Eichenbohlen')).toBeInTheDocument();

    // Filter by search input
    const searchInput = screen.getByPlaceholderText(/Inserate nach Titel, Text oder Name durchsuchen|Search ads/i);
    fireEvent.change(searchInput, { target: { value: 'Honey' } });

    expect(screen.getByText('Bio Honey 500g')).toBeInTheDocument();
    expect(screen.queryByText('Suche Eichenbohlen')).not.toBeInTheDocument();
  });

  it('switches to People tab and displays person cards with skills and active ads count', async () => {
    render(<MarketplaceView />);

    await waitFor(() => {
      expect(screen.getByText('Bio Honey 500g')).toBeInTheDocument();
    });

    const peopleTabBtn = screen.getByRole('button', { name: /Menschen|People/i });
    fireEvent.click(peopleTabBtn);

    await waitFor(() => {
      expect(screen.getByText('Alice Wonderland')).toBeInTheDocument();
      expect(screen.getByText('Bob Builder')).toBeInTheDocument();
      expect(screen.getByText('Imkerei & Honig')).toBeInTheDocument();
      expect(screen.getByText('Zimmerei & Holz')).toBeInTheDocument();
    });
  });

  it('switches to Mine tab and displays business card, my ads, and inquiries', async () => {
    render(<MarketplaceView />);

    await waitFor(() => {
      expect(screen.getByText('Bio Honey 500g')).toBeInTheDocument();
    });

    const mineTabBtn = screen.getByRole('button', { name: /Meins|Mine/i });
    fireEvent.click(mineTabBtn);

    await waitFor(() => {
      expect(screen.getByText(/Meine Visitenkarte|My Business Card/i)).toBeInTheDocument();
      expect(screen.getByText(/Meine Inserate|My Ads/i)).toBeInTheDocument();
      expect(screen.getByText('Gartenarbeit am Wochenende')).toBeInTheDocument();
      expect(screen.getAllByText(/Anfragen|Inquiries/i).length).toBeGreaterThan(0);
    });

    // In Mine tab, "Visitenkarte bearbeiten" secondary button is shown in header
    expect(screen.getAllByRole('button', { name: /Visitenkarte bearbeiten|Edit business card/i }).length).toBeGreaterThanOrEqual(1);
  });

  it('navigates to MarketPersonView when clicking on a person or board ad owner', async () => {
    render(<MarketplaceView />);

    await waitFor(() => {
      expect(screen.getByText('Alice Honey Farm')).toBeInTheDocument();
    });

    // Click owner name on board card
    const ownerBtn = screen.getByText('Alice Honey Farm');
    fireEvent.click(ownerBtn);

    // Should render MarketPersonView
    await waitFor(() => {
      expect(screen.getByText('Imkerei & Honig')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /Zurück|Back/i })).toBeInTheDocument();
    });

    // Click Back
    const backBtn = screen.getByRole('button', { name: /Zurück|Back/i });
    fireEvent.click(backBtn);

    // Returns to board
    await waitFor(() => {
      expect(screen.getByText('Bio Honey 500g')).toBeInTheDocument();
    });
  });

  it('opens AdEditorModal when clicking "Inserat aufgeben" and saves ad', async () => {
    render(<MarketplaceView />);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Inserat aufgeben|Post ad/i })).toBeInTheDocument();
    });

    const createAdBtn = screen.getByRole('button', { name: /Inserat aufgeben|Post ad/i });
    fireEvent.click(createAdBtn);

    // Modal opens
    await waitFor(() => {
      expect(screen.getByRole('heading', { level: 2, name: /Neues Inserat|New ad/i })).toBeInTheDocument();
    });

    // Fill title and text
    const titleInput = screen.getByPlaceholderText(/z\. B\. Brennholz abzugeben|e\.g\. Firewood/i);
    fireEvent.change(titleInput, { target: { value: 'Frische Äpfel abzugeben' } });

    const textInput = screen.getByTestId('ad-text-input');
    fireEvent.change(textInput, { target: { value: 'Bio-Äpfel aus dem eigenen Garten' } });

    // Save button
    const saveBtn = screen.getByRole('button', { name: /^Speichern$|^Save$/i });
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('upsert_ad', expect.objectContaining({
        ad: expect.objectContaining({
          kind: 'offer',
          title: 'Frische Äpfel abzugeben',
          text: 'Bio-Äpfel aus dem eigenen Garten',
        }),
      }));
    });
  });

  it('prompts to publish business card before creating ad if unpublished', async () => {
    const unpublishedProfile: FrontendMyMarketProfile = {
      ...mockMyProfile,
      published: false,
    };

    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_market_view') return Promise.resolve(mockSnapshot);
      if (cmd === 'get_my_market_profile') return Promise.resolve(unpublishedProfile);
      if (cmd === 'set_card_published') {
        return Promise.resolve({ ...unpublishedProfile, published: true });
      }
      return Promise.resolve({});
    });

    render(<MarketplaceView />);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Inserat aufgeben|Post ad/i })).toBeInTheDocument();
    });

    const createAdBtn = screen.getByRole('button', { name: /Inserat aufgeben|Post ad/i });
    fireEvent.click(createAdBtn);

    // Shows prompt to publish
    await waitFor(() => {
      expect(screen.getByText(/Visitenkarte veröffentlichen\?|Publish business card\?/i)).toBeInTheDocument();
    });

    // Confirm publish
    const confirmBtn = screen.getByRole('button', { name: /Veröffentlichen & Inserat aufgeben|Publish & Post Ad/i });
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('set_card_published', expect.objectContaining({
        published: true,
      }));
    });
  });

  it('triggers onBack when back button in header is clicked', async () => {
    const handleBack = vi.fn();
    render(<MarketplaceView onBack={handleBack} />);

    await waitFor(() => {
      expect(screen.getByRole('heading', { level: 1, name: /Marktplatz|Marketplace/i })).toBeInTheDocument();
    });

    const backBtn = screen.getByRole('button', { name: /Back|Zurück/i });
    expect(backBtn).toBeInTheDocument();
    fireEvent.click(backBtn);
    expect(handleBack).toHaveBeenCalledTimes(1);
  });

  it('triggers goBack from navigation when onBack prop is not provided', async () => {
    mockGoBack.mockClear();
    render(<MarketplaceView />);

    await waitFor(() => {
      expect(screen.getByRole('heading', { level: 1, name: /Marktplatz|Marketplace/i })).toBeInTheDocument();
    });

    const backBtn = screen.getByRole('button', { name: /Back|Zurück/i });
    expect(backBtn).toBeInTheDocument();
    fireEvent.click(backBtn);
    expect(mockGoBack).toHaveBeenCalledTimes(1);
  });

  it('shows 3-tier reach presets in Simple mode and hides expert storage controls', async () => {
    localStorage.setItem('humoco_ui_mode', 'simple');
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_market_view') return Promise.resolve(mockSnapshot);
      if (cmd === 'get_my_market_profile') return Promise.resolve(mockMyProfile);
      if (cmd === 'get_app_settings') {
        return Promise.resolve({
          marketShowUnknown: true,
          maxStoredCards: 2000,
        });
      }
      if (cmd === 'get_reach_preset') {
        return Promise.resolve({
          preset: 'medium',
          ownListMaxHops: 2,
          cardMaxHops: 2,
          shareMinLevel: 1,
          mediaAutoLoadMaxHops: 2,
        });
      }
      return Promise.resolve({});
    });

    render(<MarketplaceView />);

    await waitFor(() => {
      expect(screen.getByRole('heading', { level: 1, name: /Marktplatz|Marketplace/i })).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('toggle-marketplace-options-button'));

    await waitFor(() => {
      expect(screen.getByTestId('marketplace-options-panel')).toBeInTheDocument();
    });

    expect(screen.getByTestId('preset-option-near')).toBeInTheDocument();
    expect(screen.getByTestId('preset-option-medium')).toBeInTheDocument();
    expect(screen.getByTestId('preset-option-far')).toBeInTheDocument();
    expect(screen.queryByTestId('marketplace-options-inbound-section')).not.toBeInTheDocument();
    expect(screen.queryByTestId('validity-years-select')).not.toBeInTheDocument();
  });

  it('toggles marketplace options panel and allows changing market settings (Expert mode)', async () => {
    localStorage.setItem('humoco_ui_mode', 'expert');
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_market_view') return Promise.resolve(mockSnapshot);
      if (cmd === 'get_my_market_profile') return Promise.resolve(mockMyProfile);
      if (cmd === 'get_app_settings') {
        return Promise.resolve({
          marketShowUnknown: true,
          maxStoredCards: 2000,
        });
      }
      if (cmd === 'get_reach_preset') {
        return Promise.resolve({
          preset: 'medium',
          ownListMaxHops: 2,
          cardMaxHops: 2,
          shareMinLevel: 1,
          mediaAutoLoadMaxHops: 2,
        });
      }
      if (cmd === 'save_app_settings') return Promise.resolve();
      if (cmd === 'save_my_market_profile') return Promise.resolve({ ...mockMyProfile, maxHops: 3 });
      return Promise.resolve({});
    });

    render(<MarketplaceView />);

    await waitFor(() => {
      expect(screen.getByRole('heading', { level: 1, name: /Marktplatz|Marketplace/i })).toBeInTheDocument();
    });

    // Options panel is initially closed
    expect(screen.queryByTestId('marketplace-options-panel')).not.toBeInTheDocument();

    // Toggle button should be present
    const toggleBtn = screen.getByTestId('toggle-marketplace-options-button');
    expect(toggleBtn).toBeInTheDocument();
    fireEvent.click(toggleBtn);

    // Options panel opens
    await waitFor(() => {
      expect(screen.getByTestId('marketplace-options-panel')).toBeInTheDocument();
    });

    expect(screen.getByTestId('marketplace-options-outbound-section')).toBeInTheDocument();
    expect(screen.getByTestId('marketplace-options-inbound-section')).toBeInTheDocument();
    expect(screen.getByText(/settings\.marketShowUnknown|Unbekannte Ersteller anzeigen|Show unknown creators/i)).toBeInTheDocument();

    // Test updating own profile maxHops in options panel
    const outboundSection = screen.getByTestId('marketplace-options-outbound-section');
    const reachSelect = within(outboundSection).getByTestId('reach-select');
    fireEvent.change(reachSelect, { target: { value: '3' } });

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('save_my_market_profile', expect.objectContaining({
        request: expect.objectContaining({ maxHops: 3 }),
      }));
    });

    // Toggle close
    fireEvent.click(toggleBtn);
    expect(screen.queryByTestId('marketplace-options-panel')).not.toBeInTheDocument();
  });
});
