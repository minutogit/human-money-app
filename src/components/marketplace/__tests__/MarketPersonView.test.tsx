import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { MarketPersonView } from '../MarketPersonView';
import { Contact, FrontendMarketPerson } from '../../../types';
import { marketService } from '../../../services/marketService';
import { contactService } from '../../../services/contactService';
import { postService } from '../../../services/postService';
import { copyToClipboard } from '../../../utils/clipboard';
import { openUrl } from '@tauri-apps/plugin-opener';

const mockNavigate = vi.fn();

vi.mock('../../../services/marketService', () => ({
  marketService: {
    syncMarketChannels: vi.fn().mockResolvedValue(undefined),
    getMarketView: vi.fn(),
    getAdImage: vi.fn().mockResolvedValue(null),
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

vi.mock('../../../services/postService', () => ({
  postService: {
    sendMarketInquiryViaPost: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('../../../utils/clipboard', () => ({
  copyToClipboard: vi.fn().mockResolvedValue(true),
}));

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../../context/NavigationContext', () => ({
  useNavigation: () => ({
    navigate: mockNavigate,
  }),
}));

vi.mock('../../../context/SessionContext', () => ({
  useSession: () => ({
    protectAction: vi.fn(async <T,>(cb: (pw?: string) => Promise<T>) => cb('mock-password')),
  }),
  useOptionalSession: () => null,
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
      people: [{ ...personPending, adsPending: 0 }],
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
      expect(screen.queryByText(/3 Inserate werden über den Sync-Dienst nachgeladen|3 ads will be loaded via sync service/i)).not.toBeInTheDocument();
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
          publishedAt: nowSecs - 3600,
          expiresAt: nowSecs + 86400 * 30,
          allowInquiries: true,
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

  it('validates external website URLs before calling openUrl (blocks javascript:/file: and normalizes uppercase/bare domains)', async () => {
    const { rerender } = render(
      <MarketPersonView
        person={{
          ...basePerson,
          profile: { ...basePerson.profile, url: 'javascript:alert(1)' },
        }}
        onBack={vi.fn()}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: 'javascript:alert(1)' }));
    await Promise.resolve();
    expect(openUrl).not.toHaveBeenCalled();

    rerender(
      <MarketPersonView
        person={{
          ...basePerson,
          profile: { ...basePerson.profile, url: 'file:///etc/passwd' },
        }}
        onBack={vi.fn()}
      />
    );
    fireEvent.click(screen.getByRole('button', { name: 'file:///etc/passwd' }));
    await Promise.resolve();
    expect(openUrl).not.toHaveBeenCalled();

    rerender(
      <MarketPersonView
        person={{
          ...basePerson,
          profile: { ...basePerson.profile, url: '  HTTPS://example.org/shop  ' },
        }}
        onBack={vi.fn()}
      />
    );
    fireEvent.click(screen.getByRole('button', { name: 'HTTPS://example.org/shop' }));
    await waitFor(() => {
      expect(openUrl).toHaveBeenCalledWith('https://example.org/shop');
    });

    rerender(
      <MarketPersonView
        person={{
          ...basePerson,
          profile: { ...basePerson.profile, url: 'imkerei.example.org' },
        }}
        onBack={vi.fn()}
      />
    );
    fireEvent.click(screen.getByRole('button', { name: 'imkerei.example.org' }));
    await waitFor(() => {
      expect(openUrl).toHaveBeenCalledWith('https://imkerei.example.org/');
    });
  });

  it('renders profile.address.fullAddress fallback when structured street/city fields are absent', () => {
    render(
      <MarketPersonView
        person={{
          ...basePerson,
          profile: {
            ...basePerson.profile,
            address: {
              fullAddress: 'Test Street 123, 12345 Test City, Germany',
            },
          },
        }}
        onBack={vi.fn()}
      />
    );

    expect(screen.getByText('Test Street 123, 12345 Test City, Germany')).toBeInTheDocument();
  });

  it('navigates to payment recipientDid and copies truncated DID to clipboard', async () => {
    const subDid = 'shop:ab12@did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH';
    const onBack = vi.fn();

    render(
      <MarketPersonView
        person={{
          ...basePerson,
          did: 'did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH',
          recipientDid: subDid,
        }}
        onBack={onBack}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: /Zurück|Back/i }));
    expect(onBack).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole('button', { name: /^Bezahlen$|^Pay$/i }));
    expect(mockNavigate).toHaveBeenCalledWith({
      view: 'send_vouchers',
      initialRecipientId: subDid,
    });

    fireEvent.click(screen.getByTitle(subDid));
    await waitFor(() => {
      expect(copyToClipboard).toHaveBeenCalledWith(subDid);
    });
  });

  it('exports forward card via ShareCardModal and handles disabled and error states', async () => {
    vi.mocked(marketService.exportForwardCard)
      .mockRejectedValueOnce(new Error('Hop limit reached'))
      .mockResolvedValueOnce({
        envelopeJson: '{"card_jws":"fwd"}',
        shareUri: 'humoco://card?env=fwd',
        qrSvg: '<svg xmlns="http://www.w3.org/2000/svg"><rect/></svg>',
        did: basePerson.did,
      });

    const { rerender } = render(
      <MarketPersonView
        person={{ ...basePerson, canForward: false, sources: [{ kind: 'card', hops: 2 }] }}
        onBack={vi.fn()}
      />
    );

    const disabledForwardBtn = screen.getByRole('button', { name: /Weiterempfehlen|Recommend|Forward/i });
    expect(disabledForwardBtn).toBeDisabled();
    expect(disabledForwardBtn).toHaveAttribute('title', expect.stringMatching(/Maximale Weiterempfehlungsstufen|Maximum recommendation/i));

    rerender(<MarketPersonView person={{ ...basePerson, canForward: true }} onBack={vi.fn()} />);

    const forwardBtn = screen.getByRole('button', { name: /Weiterempfehlen|Recommend|Forward/i });
    expect(forwardBtn).toBeEnabled();
    fireEvent.click(forwardBtn);

    expect(await screen.findByText('Hop limit reached')).toBeInTheDocument();
    expect(screen.queryByAltText(/HuMoCo Contact Card QR Code/i)).not.toBeInTheDocument();

    fireEvent.click(forwardBtn);
    await waitFor(() => {
      expect(marketService.exportForwardCard).toHaveBeenLastCalledWith(basePerson.did, 'mock-password');
      expect(screen.getByAltText(/HuMoCo Contact Card QR Code/i)).toBeInTheDocument();
    });
  });

  it('opens inquiry modal, warns when sync is missing, submits via postService, and surfaces errors', async () => {
    vi.mocked(postService.sendMarketInquiryViaPost)
      .mockRejectedValueOnce(new Error('Sync delivery failed'))
      .mockResolvedValueOnce({} as never);

    render(<MarketPersonView person={{ ...basePerson, cardSyncs: [] }} onBack={vi.fn()} />);

    fireEvent.click(screen.getByRole('button', { name: /Anfrage senden|Send Inquiry/i }));

    expect(screen.getByText(/keinen Sync-Dienst auf ihrer Visitenkarte|does not currently have a sync service/i)).toBeInTheDocument();

    const textarea = screen.getByPlaceholderText(/Kurze Notiz|Short note/i);
    fireEvent.change(textarea, { target: { value: '  Hallo Bob, passt Samstag?  ' } });

    const submitButtons = screen.getAllByRole('button', { name: /Anfrage senden|Send Inquiry/i });
    fireEvent.click(submitButtons[submitButtons.length - 1]);

    expect(await screen.findByText('Sync delivery failed')).toBeInTheDocument();
    expect(postService.sendMarketInquiryViaPost).toHaveBeenCalledWith(
      basePerson.did,
      { kind: 'person', skillTitle: undefined },
      'Hallo Bob, passt Samstag?',
      [],
      'mock-password'
    );

    fireEvent.click(submitButtons[submitButtons.length - 1]);
    expect(await screen.findByText(/Anfrage erfolgreich gesendet!|Inquiry successfully sent!/i)).toBeInTheDocument();
  });

  it('hides inquiry triggers when allowInquiries is false', () => {
    render(
      <MarketPersonView
        person={{
          ...basePerson,
          allowInquiries: false,
          skills: [{ title: 'Imkerei', origin: 'card' }],
        }}
        onBack={vi.fn()}
      />
    );

    expect(screen.queryByRole('button', { name: /Anfrage senden|Send Inquiry/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Anfragen$|^Inquire$/i })).not.toBeInTheDocument();
  });

  it('adapts contact action button label across unknown, unrated pending, and rated contacts', async () => {
    const baseContact: Contact = {
      did: basePerson.did,
      profile: { id: basePerson.did, firstName: 'Bob', lastName: 'Builder' },
      tags: [],
      addedAt: '2026-01-01T00:00:00Z',
      syncs: [],
    };

    vi.mocked(contactService.getContacts).mockResolvedValue([]);
    const { unmount } = render(<MarketPersonView person={basePerson} onBack={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Ins Adressbuch|Add to Address Book/i })).toBeInTheDocument();
    });
    unmount();

    vi.mocked(contactService.getContacts).mockResolvedValue([{ ...baseContact, trustLevel: null }]);
    const { unmount: unmountUnrated } = render(<MarketPersonView person={basePerson} onBack={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Vertrauen einstufen|Rate Trust/i })).toBeInTheDocument();
    });
    unmountUnrated();

    vi.mocked(contactService.getContacts).mockResolvedValue([{ ...baseContact, trustLevel: 2 }]);
    render(<MarketPersonView person={basePerson} onBack={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Kontakt bearbeiten|Edit Contact/i })).toBeInTheDocument();
    });
  });

  it('renders unknown trust badge, organization fallback name, accepted standards, and source badges', () => {
    render(
      <MarketPersonView
        person={{
          ...basePerson,
          profile: { id: basePerson.did, organization: 'Biohof Berg' },
          trust: 'unknown',
          standards: [{ uuid: 'std-1', name: 'Minuto Region' }],
          sources: [{ kind: 'voucher' }, { kind: 'card', hops: 2 }, { kind: 'sync', readUrl: 'https://sync.example.org/sync.php' }],
        }}
        onBack={vi.fn()}
      />
    );

    expect(screen.getByRole('heading', { level: 2, name: 'Biohof Berg' })).toBeInTheDocument();
    expect(screen.getByText(/Unbekannter Kontakt|Unknown contact/i)).toBeInTheDocument();
    expect(screen.getByText('Minuto Region')).toBeInTheDocument();
    expect(screen.getByText(/Aus Gutschein-Profil|From voucher profile/i)).toBeInTheDocument();
    expect(screen.getByText(/Visitenkarte \(Stufe 2\)|Business card \(Degree 2\)/i)).toBeInTheDocument();
    expect(screen.getByText(/Sync-Dienst|Sync Service/i)).toBeInTheDocument();
  });
});
