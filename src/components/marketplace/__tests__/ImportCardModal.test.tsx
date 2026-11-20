import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ImportCardModal } from '../ImportCardModal';
import { marketService } from '../../../services/marketService';
import { contactService } from '../../../services/contactService';

vi.mock('../../../services/marketService', () => ({
  marketService: {
    importContactCard: vi.fn(),
    getMyMarketProfile: vi.fn(),
    getMarketView: vi.fn(),
    syncMarketChannels: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('../../../services/contactService', () => ({
  contactService: {
    getContacts: vi.fn(),
    saveContact: vi.fn().mockResolvedValue(undefined),
  },
}));

const mockProtectAction = vi.fn(async <T,>(cb: (pw?: string) => Promise<T>) => cb('mock-password'));

vi.mock('../../../context/SessionContext', () => ({
  useSession: () => ({
    protectAction: mockProtectAction,
  }),
}));

describe('ImportCardModal Component', () => {
  const defaultProps = {
    isOpen: true,
    onClose: vi.fn(),
    onImportSuccess: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(marketService.getMyMarketProfile).mockResolvedValue({
      profile: { id: 'alice-id', firstName: 'Alice', lastName: 'Self' },
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
      skills: [],
      ads: [],
      maxHops: 2,
      allowInquiries: true,
      published: true,
      shareUri: 'humoco://card?env=test',
      fileWire: '{"envelope":"test"}',
      qrSvg: '<svg xmlns="http://www.w3.org/2000/svg"><rect/></svg>',
      qrIsPartial: false,
    });
    vi.mocked(contactService.getContacts).mockResolvedValue([]);
    vi.mocked(marketService.getMarketView).mockResolvedValue({
      board: [],
      people: [
        {
          did: 'did:key:z6MkuBob456',
          profile: { id: 'bob-id', firstName: 'Bob', lastName: 'Partner' },
          trust: 'trusted',
          skills: [],
          activeAds: [],
          adsPending: 0,
          sources: [{ kind: 'card', hops: 1 }],
          standards: [],
          cardSyncs: [{ readUrl: 'https://bob.org/sync.php', writeUrl: 'https://bob.org/sync.php', dropPub: 'pub' }],
          asOf: 1000,
          canForward: true,
          allowInquiries: true,
        },
      ],
      now: 1000,
    });
  });

  it('renders file upload button and textarea for card input', () => {
    render(<ImportCardModal {...defaultProps} />);

    expect(screen.getByRole('button', { name: /Select \.hmcard file|hmcard.*Datei auswählen/i })).toBeInTheDocument();
    expect(screen.getByPlaceholderText(/humoco:\/\/card/i)).toBeInTheDocument();
  });

  it('on successful import shows 1-click trust level selection and counter-scan QR code', async () => {
    vi.mocked(marketService.importContactCard).mockResolvedValue({
      status: 'accepted',
      did: 'did:key:z6MkuBob456',
      replaced: false,
    });

    render(<ImportCardModal {...defaultProps} />);

    const textarea = screen.getByPlaceholderText(/humoco:\/\/card/i);
    fireEvent.change(textarea, { target: { value: 'humoco://card?env=valid_card' } });

    const importBtn = screen.getByRole('button', { name: /^Import$/i });
    fireEvent.click(importBtn);

    await waitFor(() => {
      expect(marketService.importContactCard).toHaveBeenCalledWith('humoco://card?env=valid_card', 'mock-password');
      expect(defaultProps.onImportSuccess).toHaveBeenCalledWith('did:key:z6MkuBob456');
    });

    // Check 1-click trust level buttons
    await waitFor(() => {
      expect(screen.getByText(/In mein Adressbuch aufnehmen als|Add to my address book as/i)).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /Freund.*Stufe 2|Friend.*Level 2|Vertraut.*Stufe 2|Trusted.*Level 2/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /Bekannt.*Stufe 1|Acquaintance.*Level 1|Known.*Level 1/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /Nur im Marktplatz merken|Keep in marketplace only/i })).toBeInTheDocument();
    });

    // Check counter-scan QR image
    expect(screen.getByAltText(/HuMoCo Counter QR Code/i)).toBeInTheDocument();
  });

  it('clicking Level 2 saves contact with trustLevel: 2 and triggers syncMarketChannels', async () => {
    vi.mocked(marketService.importContactCard).mockResolvedValue({
      status: 'accepted',
      did: 'did:key:z6MkuBob456',
      replaced: false,
    });

    render(<ImportCardModal {...defaultProps} />);

    const textarea = screen.getByPlaceholderText(/humoco:\/\/card/i);
    fireEvent.change(textarea, { target: { value: 'humoco://card?env=valid_card' } });

    const importBtn = screen.getByRole('button', { name: /^Import$/i });
    fireEvent.click(importBtn);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Freund.*Stufe 2|Friend.*Level 2|Vertraut.*Stufe 2|Trusted.*Level 2/i })).toBeInTheDocument();
    });

    const trust2Btn = screen.getByRole('button', { name: /Freund.*Stufe 2|Friend.*Level 2|Vertraut.*Stufe 2|Trusted.*Level 2/i });
    fireEvent.click(trust2Btn);

    await waitFor(() => {
      expect(contactService.saveContact).toHaveBeenCalledWith(
        expect.objectContaining({
          did: 'did:key:z6MkuBob456',
          trustLevel: 2,
        }),
        'mock-password'
      );
      expect(marketService.syncMarketChannels).toHaveBeenCalled();
    });
  });
});
