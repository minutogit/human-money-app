import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { MarketplaceView } from '../MarketplaceView';
import { invoke } from '@tauri-apps/api/core';
import { FrontendMarketPerson } from '../../../types';

const mockNavigate = vi.fn();
const mockGoBack = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('../../../context/NavigationContext', () => ({
  useNavigation: () => ({
    navigate: mockNavigate,
    goBack: mockGoBack,
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
  const mockMarketPeople: FrontendMarketPerson[] = [
    {
      did: 'did:key:z6MkuAlice',
      profile: {
        id: 'did:key:z6MkuAlice',
        firstName: 'Alice',
        lastName: 'Wonderland',
        organization: 'Alice Honey Farm',
        community: 'Bio Community',
        email: 'alice@honey.org',
        phone: '+4912345678',
        url: 'https://alice-honey.org',
        serviceOffer: 'Bio Honey & Beeswax candles',
        needs: 'Wooden boxes and glass jars',
      },
      listings: [
        {
          listingType: 'offer',
          title: 'Bio Honey 500g',
          text: 'Fresh summer flower honey from regional bees',
          pricing: '5 FreeTaler',
          origin: 'voucher',
        },
        {
          listingType: 'request',
          title: 'Need 100 Glass Jars',
          text: 'Empty clean glass jars with lids',
          pricing: null,
          origin: 'voucher',
        },
      ],
      sources: [{ kind: 'voucher' }, { kind: 'card', hops: 1 }],
      asOf: 1731000000,
      standards: [{ name: 'FreeTaler', uuid: 'std-freetaler-uuid' }],
      trust: 'trusted',
      canForward: true,
      cardDrops: [],
    },
    {
      did: 'did:key:z6MkuBob',
      profile: {
        id: 'did:key:z6MkuBob',
        firstName: 'Bob',
        lastName: 'Builder',
        organization: 'Bob Carpentry',
      },
      listings: [], // Creator without explicit listings
      sources: [{ kind: 'voucher' }],
      asOf: 1731050000,
      standards: [{ name: 'Minuto', uuid: 'std-minuto-uuid' }],
      trust: 'unknown',
      canForward: false,
      cardDrops: [],
    },
    {
      did: 'did:key:z6MkuCharlie',
      profile: {
        id: 'did:key:z6MkuCharlie',
        firstName: 'Charlie',
        lastName: 'Chaplin',
        organization: 'Cinema',
      },
      listings: [
        {
          listingType: 'offer',
          title: 'Movie Tickets',
          text: 'Classic comedy screening',
          pricing: '10 Minuto',
          origin: 'card',
        },
      ],
      sources: [{ kind: 'card', hops: 3 }],
      asOf: 1731060000,
      standards: [],
      trust: 'unknown',
      canForward: false, // Max hops reached!
      cardDrops: [],
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_market_view') {
        return Promise.resolve(mockMarketPeople);
      }
      if (cmd === 'get_app_settings') {
        return Promise.resolve({
          marketShowUnknown: true,
          sessionTimeoutSeconds: 300,
          privacyDefault: 'ask',
          maxStoreHops: 2,
          marketSendCardOnPayment: false,
        });
      }
      if (cmd === 'get_contacts') {
        return Promise.resolve([]);
      }
      if (cmd === 'get_own_contact_card') {
        return Promise.resolve({
          card: {
            did: 'did:key:z6MkuSelf',
            v: 1,
            validUntil: Math.floor(Date.now() / 1000) + 5184000,
            maxHops: 2,
            profile: {
              id: 'did:key:z6MkuSelf',
              firstName: 'Owner',
              lastName: 'User',
            },
            listings: [
              {
                listingType: 'offer',
                title: 'Existing Own Offer',
                text: 'Own offer text',
                pricing: '20 Minuto',
              },
            ],
            drops: [],
          },
          cardJws: 'signed.card.jws',
          qrSvg: '<svg id="qr-test"></svg>',
          shareUri: 'humoco://card?env=eyJ0ZXN0IjoxfQ',
          envelopeJson: '{"card_jws":"signed.card.jws","hops":1}',
        });
      }
      if (cmd === 'get_user_profile') {
        return Promise.resolve({
          id: 'did:key:z6MkuSelf',
          firstName: 'Owner',
          lastName: 'User',
        });
      }
      if (cmd === 'save_own_contact_card') {
        return Promise.resolve({
          card: {
            did: 'did:key:z6MkuSelf',
            v: 2,
            validUntil: Math.floor(Date.now() / 1000) + 5184000,
            maxHops: 2,
            profile: { id: 'did:key:z6MkuSelf', firstName: 'Owner', lastName: 'User' },
            listings: [],
            drops: [],
          },
          cardJws: 'signed.card.jws.v2',
          qrSvg: '<svg id="qr-test-v2"></svg>',
          shareUri: 'humoco://card?env=eyJ2IjoxfQ',
          envelopeJson: '{"card_jws":"signed.card.jws.v2","hops":1}',
        });
      }
      if (cmd === 'export_forward_card') {
        return Promise.resolve({
          did: 'did:key:z6MkuAlice',
          envelopeJson: '{"hops":2}',
          shareUri: 'humoco://card?env=forwarded',
          qrSvg: '<svg id="forward-qr"></svg>',
        });
      }
      if (cmd === 'import_contact_card') {
        return Promise.resolve({
          status: 'accepted',
          did: 'did:key:z6MkuAlice',
          replaced: false,
        });
      }
      return Promise.resolve(null);
    });
  });

  it('renders offers tab by default with listing details', async () => {
    render(<MarketplaceView />);

    await waitFor(() => {
      expect(screen.getByText('Bio Honey 500g')).toBeInTheDocument();
    });

    expect(screen.getByText('5 FreeTaler')).toBeInTheDocument();
    expect(screen.getByText('Alice Wonderland')).toBeInTheDocument();
    expect(screen.getByText(/Fresh summer flower honey/)).toBeInTheDocument();
  });

  it('renders requests tab when clicking Requests tab', async () => {
    render(<MarketplaceView />);

    await waitFor(() => {
      expect(screen.getByText('Bio Honey 500g')).toBeInTheDocument();
    });

    // Click Requests tab
    const requestsTab = screen.getByRole('button', { name: /Requests|Suche/i });
    fireEvent.click(requestsTab);

    await waitFor(() => {
      expect(screen.getByText('Need 100 Glass Jars')).toBeInTheDocument();
    });
    expect(screen.getByText(/Empty clean glass jars with lids/)).toBeInTheDocument();
  });

  it('renders people tab and shows "accepts own vouchers" for creators without listings', async () => {
    render(<MarketplaceView />);

    await waitFor(() => {
      expect(screen.getByText('Bio Honey 500g')).toBeInTheDocument();
    });

    // Click People tab
    const peopleTab = screen.getByRole('button', { name: /People|Personen/i });
    fireEvent.click(peopleTab);

    await waitFor(() => {
      expect(screen.getByText('Alice Wonderland')).toBeInTheDocument();
      expect(screen.getByText('Bob Builder')).toBeInTheDocument();
    });

    // Bob has no listings, so he should show "accepts own vouchers"
    expect(screen.getByText(/Accepts own vouchers|Nimmt eigene Gutscheine an/i)).toBeInTheDocument();

    // Standards should be visible
    expect(screen.getAllByText('FreeTaler').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Minuto').length).toBeGreaterThan(0);
  });

  it('filters listings and people by search query', async () => {
    render(<MarketplaceView />);

    await waitFor(() => {
      expect(screen.getByText('Bio Honey 500g')).toBeInTheDocument();
    });

    const searchInput = screen.getByPlaceholderText(/Search offers|Angebote/i);
    fireEvent.change(searchInput, { target: { value: 'Honey' } });

    // Alice matches "Honey"
    expect(screen.getByText('Bio Honey 500g')).toBeInTheDocument();

    // Switch to People tab
    const peopleTab = screen.getByRole('button', { name: /People|Personen/i });
    fireEvent.click(peopleTab);

    expect(screen.getByText('Alice Wonderland')).toBeInTheDocument();
    expect(screen.queryByText('Bob Builder')).not.toBeInTheDocument();
  });

  it('filters by voucher standard and trust tier', async () => {
    render(<MarketplaceView />);

    await waitFor(() => {
      expect(screen.getByText('Bio Honey 500g')).toBeInTheDocument();
    });

    // Go to People tab
    const peopleTab = screen.getByRole('button', { name: /People|Personen/i });
    fireEvent.click(peopleTab);

    await waitFor(() => {
      expect(screen.getByText('Bob Builder')).toBeInTheDocument();
    });

    // Filter by Minuto standard
    const standardSelect = screen.getByRole('combobox', { name: /All Standards|Alle Standards/i });
    fireEvent.change(standardSelect, { target: { value: 'std-minuto-uuid' } });

    expect(screen.queryByText('Alice Wonderland')).not.toBeInTheDocument();
    expect(screen.getByText('Bob Builder')).toBeInTheDocument();
  });

  it('navigates to send_vouchers when clicking Pay (Bezahlen)', async () => {
    render(<MarketplaceView />);

    await waitFor(() => {
      expect(screen.getByText('Bio Honey 500g')).toBeInTheDocument();
    });

    // Go to People tab
    const peopleTab = screen.getByRole('button', { name: /People|Personen/i });
    fireEvent.click(peopleTab);

    await waitFor(() => {
      expect(screen.getByText('Bob Builder')).toBeInTheDocument();
    });

    // Find all Pay buttons and click the one for Bob
    const payButtons = screen.getAllByRole('button', { name: /Pay|Bezahlen/i });
    fireEvent.click(payButtons[1]); // Bob's pay button

    expect(mockNavigate).toHaveBeenCalledWith({
      view: 'send_vouchers',
      initialRecipientId: 'did:key:z6MkuBob',
    });
  });

  it('opens MarketPersonView when clicking on a person card and allows paying from detail view', async () => {
    render(<MarketplaceView />);

    await waitFor(() => {
      expect(screen.getByText('Bio Honey 500g')).toBeInTheDocument();
    });

    // Click on Alice's offer listing card
    fireEvent.click(screen.getByText('Bio Honey 500g'));

    // Should now show Alice's detailed view
    await waitFor(() => {
      expect(screen.getByText('Alice Honey Farm')).toBeInTheDocument();
      expect(screen.getByText('alice@honey.org')).toBeInTheDocument();
      expect(screen.getByText('+4912345678')).toBeInTheDocument();
    });

    // Click Pay button in detailed person view
    const payButton = screen.getByRole('button', { name: /Pay|Bezahlen/i });
    fireEvent.click(payButton);

    expect(mockNavigate).toHaveBeenCalledWith({
      view: 'send_vouchers',
      initialRecipientId: 'did:key:z6MkuAlice',
    });
  });

  it('renders empty state when market view returns empty list', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_market_view') {
        return Promise.resolve([]);
      }
      if (cmd === 'get_app_settings') {
        return Promise.resolve({ marketShowUnknown: true });
      }
      return Promise.resolve(null);
    });

    render(<MarketplaceView />);

    await waitFor(() => {
      expect(screen.getByText(/No marketplace entries found|Keine Marktplatz-Einträge gefunden/i)).toBeInTheDocument();
    });
  });

  it('opens MyOfferModal, enforces character limits (80/280/40), and saves card', async () => {
    render(<MarketplaceView />);

    await waitFor(() => {
      expect(screen.getByText('Bio Honey 500g')).toBeInTheDocument();
    });

    // Click "My Offer" button in header
    const myOfferBtn = screen.getByRole('button', { name: /My Offer|Mein Angebot/i });
    fireEvent.click(myOfferBtn);

    // Modal should open and load existing card
    await waitFor(() => {
      expect(screen.getByDisplayValue('Existing Own Offer')).toBeInTheDocument();
    });

    // Add an offer listing
    const addOfferBtn = screen.getByRole('button', { name: /Add Offer|Angebot hinzufügen/i });
    fireEvent.click(addOfferBtn);

    // Find the title inputs
    const titleInputs = screen.getAllByPlaceholderText(/Honey from own apiary|Honig aus eigener Imkerei/i);
    const newTitleInput = titleInputs[titleInputs.length - 1];

    // Exceed 80 chars
    fireEvent.change(newTitleInput, { target: { value: 'A'.repeat(85) } });
    expect(screen.getByText(/Maximum 80 characters allowed|Maximal 80 Zeichen erlaubt/i)).toBeInTheDocument();

    // Fix title
    fireEvent.change(newTitleInput, { target: { value: 'Fresh Apples 1kg' } });

    // Exceed pricing 40 chars
    const pricingInputs = screen.getAllByPlaceholderText(/30 Minuto \/ hr|30 Minuto \/ Std/i);
    const newPricingInput = pricingInputs[pricingInputs.length - 1];
    fireEvent.change(newPricingInput, { target: { value: 'P'.repeat(45) } });
    expect(screen.getByText(/Maximum 40 characters allowed|Maximal 40 Zeichen erlaubt/i)).toBeInTheDocument();

    // Fix pricing
    fireEvent.change(newPricingInput, { target: { value: '3 Minuto' } });

    // Exceed description 280 chars
    const textareas = screen.getAllByPlaceholderText(/Describe your offer|Beschreibe dein Angebot/i);
    const newTextarea = textareas[textareas.length - 1];
    fireEvent.change(newTextarea, { target: { value: 'T'.repeat(290) } });
    expect(screen.getByText(/Maximum 280 characters allowed|Maximal 280 Zeichen erlaubt/i)).toBeInTheDocument();

    // Fix description
    fireEvent.change(newTextarea, { target: { value: 'Organic crisp apples from garden' } });

    // Save button
    const saveBtn = screen.getByRole('button', { name: /Save|Speichern/i });
    expect(saveBtn).not.toBeDisabled();
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('save_own_contact_card', expect.objectContaining({
        request: expect.objectContaining({
          maxHops: 2,
          listings: expect.arrayContaining([
            expect.objectContaining({ title: 'Fresh Apples 1kg', pricing: '3 Minuto' })
          ])
        })
      }));
    });
  });

  it('handles "Weiterempfehlen" (Recommend) button: enabled when canForward: true, disabled when canForward: false', async () => {
    render(<MarketplaceView />);

    await waitFor(() => {
      expect(screen.getByText('Bio Honey 500g')).toBeInTheDocument();
    });

    // Click on Alice (canForward: true)
    fireEvent.click(screen.getByText('Bio Honey 500g'));

    await waitFor(() => {
      expect(screen.getByText('Alice Honey Farm')).toBeInTheDocument();
    });

    // Recommend button should be enabled for Alice
    const recommendBtn = screen.getByRole('button', { name: /Recommend|Weiterempfehlen/i });
    expect(recommendBtn).not.toBeDisabled();

    // Click Recommend -> calls export_forward_card
    fireEvent.click(recommendBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('export_forward_card', expect.objectContaining({
        did: 'did:key:z6MkuAlice',
      }));
    });

    // Share modal opens
    expect(screen.getByText(/Recommend Contact Card|Visitenkarte weiterempfehlen/i)).toBeInTheDocument();

    // Close modal & go back
    const closeBtn = screen.getByRole('button', { name: /Close|Schließen/i });
    fireEvent.click(closeBtn);

    const backBtns = screen.getAllByRole('button', { name: /Back|Zurück/i });
    fireEvent.click(backBtns[0]);

    // Switch to People tab and click Charlie (canForward: false with card source)
    const peopleTab = screen.getByRole('button', { name: /People|Personen/i });
    fireEvent.click(peopleTab);

    await waitFor(() => {
      expect(screen.getByText('Charlie Chaplin')).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText('Charlie Chaplin'));

    await waitFor(() => {
      expect(screen.getByText('Cinema')).toBeInTheDocument();
    });

    // Recommend button should be disabled for Charlie
    const disabledRecommendBtn = screen.getByRole('button', { name: /Recommend|Weiterempfehlen/i });
    expect(disabledRecommendBtn).toBeDisabled();
    expect(disabledRecommendBtn).toHaveAttribute('title', expect.stringMatching(/Maximum recommendation hops reached|Maximale Weiterempfehlungsstufen erreicht/i));
  });

  it('imports card via ImportCardModal and triggers reload', async () => {
    render(<MarketplaceView />);

    await waitFor(() => {
      expect(screen.getByText('Bio Honey 500g')).toBeInTheDocument();
    });

    // Click "Import" button in header
    const importBtn = screen.getByRole('button', { name: /Import|Importieren/i });
    fireEvent.click(importBtn);

    // Modal opens
    await waitFor(() => {
      expect(screen.getByText(/Import Contact Card|Visitenkarte importieren/i)).toBeInTheDocument();
    });

    // Paste payload
    const textarea = screen.getByPlaceholderText(/humoco:\/\/card\?env=.../i);
    fireEvent.change(textarea, { target: { value: 'humoco://card?env=eyJjYXJkIjoxfQ' } });

    // Click import inside modal footer
    const allImportBtns = screen.getAllByRole('button', { name: /Import|Importieren/i });
    const doImportBtn = allImportBtns[allImportBtns.length - 1];
    fireEvent.click(doImportBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('import_contact_card', expect.objectContaining({
        payload: 'humoco://card?env=eyJjYXJkIjoxfQ',
      }));
    });

    // Navigates to Alice's detail view
    await waitFor(() => {
      expect(screen.getByText('Alice Honey Farm')).toBeInTheDocument();
    });
  });
});
