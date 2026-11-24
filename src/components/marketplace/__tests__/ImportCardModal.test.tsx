import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ImportCardModal } from '../ImportCardModal';
import { marketService } from '../../../services/marketService';
import { contactService } from '../../../services/contactService';
import { profileService } from '../../../services/profileService';

vi.mock('../../../services/marketService', () => ({
  marketService: {
    importContactCard: vi.fn(),
    getMyMarketProfile: vi.fn(),
    getMarketView: vi.fn(),
    syncMarketChannels: vi.fn().mockResolvedValue(undefined),
    probeContactSyncs: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('../../../services/contactService', () => ({
  contactService: {
    getContacts: vi.fn(),
    saveContact: vi.fn().mockResolvedValue(undefined),
    deleteContact: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('../../../services/profileService', () => ({
  profileService: {
    getUserId: vi.fn(),
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

  const ownDid = 'did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH';
  const siblingRootDid = 'did:key:z6MkrCdE7fGhJkMnPqRsTuVwXyZ123456789abcdefghijkL';
  const siblingSubDid = `garden:ab12@${siblingRootDid}`;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(marketService.importContactCard).mockReset();
    vi.mocked(contactService.getContacts).mockReset();
    vi.mocked(contactService.saveContact).mockReset().mockResolvedValue(undefined);
    vi.mocked(contactService.deleteContact).mockReset().mockResolvedValue(undefined);
    vi.mocked(profileService.getUserId).mockReset().mockResolvedValue(ownDid);
    vi.mocked(marketService.getMyMarketProfile).mockReset().mockResolvedValue({
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
    const qrImg = screen.getByAltText(/HuMoCo Counter QR Code/i);
    expect(qrImg).toBeInTheDocument();
    expect(qrImg).toHaveAttribute('src', expect.stringContaining('data:image/svg+xml;utf8,%3Csvg'));
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

  it('ignores late importContactCard completion after modal closes', async () => {
    let resolveImport!: (val: { status: string; did: string; replaced: boolean }) => void;
    vi.mocked(marketService.importContactCard).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveImport = resolve;
        })
    );

    const onImportSuccess = vi.fn();
    const onClose = vi.fn();
    const { rerender } = render(
      <ImportCardModal isOpen={true} onClose={onClose} onImportSuccess={onImportSuccess} />
    );

    fireEvent.change(screen.getByPlaceholderText(/humoco:\/\/card/i), {
      target: { value: 'humoco://card?env=slow' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^Import$/i }));

    await waitFor(() => {
      expect(marketService.importContactCard).toHaveBeenCalledTimes(1);
    });

    // Close modal before async import completes
    rerender(
      <ImportCardModal isOpen={false} onClose={onClose} onImportSuccess={onImportSuccess} />
    );

    resolveImport({ status: 'accepted', did: 'did:key:z6MkuBob456', replaced: false });
    await Promise.resolve();
    await Promise.resolve();

    expect(onImportSuccess).not.toHaveBeenCalled();
  });

  it('handles non-accepted statuses, import errors, replaced card title, and hidden counter QR when unpublished', async () => {
    vi.mocked(marketService.getMyMarketProfile).mockResolvedValueOnce({
      profile: { id: 'alice-id', firstName: 'Alice' },
      visibility: {
        name: 'everyone', organization: 'everyone', community: 'everyone', city: 'everyone',
        url: 'everyone', picture: 'everyone', street: 'everyone', email: 'everyone',
        phone: 'everyone', coordinates: 'everyone', gender: 'everyone',
      },
      skills: [], ads: [], maxHops: 2, allowInquiries: true, published: false,
      qrIsPartial: false,
    });

    vi.mocked(marketService.importContactCard)
      .mockResolvedValueOnce({ status: 'ignored_stale', replaced: false })
      .mockResolvedValueOnce({ status: 'rejected_expired', replaced: false })
      .mockResolvedValueOnce({ status: 'rejected_hops', replaced: false })
      .mockResolvedValueOnce({ status: 'rejected_offender', replaced: false })
      .mockResolvedValueOnce({ status: 'malformed_signature', replaced: false })
      .mockRejectedValueOnce(new Error('Invalid envelope JSON'))
      .mockResolvedValueOnce({ status: 'accepted', did: 'did:key:z6MkuBob456', replaced: true });

    render(<ImportCardModal {...defaultProps} />);
    const textarea = screen.getByPlaceholderText(/humoco:\/\/card/i);
    const submitImport = () => {
      fireEvent.change(textarea, { target: { value: 'humoco://card?env=payload' } });
      fireEvent.click(screen.getByRole('button', { name: /^Import$/i }));
    };

    submitImport();
    expect(await screen.findByText(/Bereits aktuell|Already Up to Date/i)).toBeInTheDocument();
    expect(screen.getByText(/gleicher oder neuerer Version|same or newer version/i)).toBeInTheDocument();

    submitImport();
    expect(await screen.findByText(/Visitenkarte abgelaufen|Business card expired/i)).toBeInTheDocument();

    submitImport();
    expect(await screen.findByText(/Weiterempfehlungslimit überschritten|Recommendation Limit Exceeded/i)).toBeInTheDocument();

    submitImport();
    expect(await screen.findByText(/Sicherheitswarnung: Gesperrt|Security Warning: Blocked/i)).toBeInTheDocument();

    submitImport();
    expect(await screen.findByText('malformed_signature')).toBeInTheDocument();

    submitImport();
    expect(await screen.findByText('Invalid envelope JSON')).toBeInTheDocument();
    expect(defaultProps.onImportSuccess).not.toHaveBeenCalled();

    submitImport();
    expect(await screen.findByText(/Visitenkarte aktualisiert|Business Card Updated/i)).toBeInTheDocument();
    expect(screen.queryByAltText(/HuMoCo Counter QR Code/i)).not.toBeInTheDocument();
  });

  it('blocks importing own active profile DID or contact URI', async () => {
    render(<ImportCardModal {...defaultProps} />);

    fireEvent.change(screen.getByPlaceholderText(/humoco:\/\/card/i), {
      target: { value: `humoco://contact?did=${ownDid}&name=Self` },
    });
    fireEvent.click(screen.getByRole('button', { name: /^Import$/i }));

    expect(await screen.findByText(/aktuell aktives eigenes Profil|currently active own profile/i)).toBeInTheDocument();
    expect(contactService.saveContact).not.toHaveBeenCalled();
    expect(marketService.importContactCard).not.toHaveBeenCalled();
    expect(defaultProps.onImportSuccess).not.toHaveBeenCalled();
  });

  it('imports humoco://contact URI with multi-part name, sync URL, and sibling-prefix trust notice', async () => {
    vi.mocked(contactService.getContacts).mockResolvedValue([
      {
        did: siblingRootDid,
        profile: { id: siblingRootDid, firstName: 'Jane', lastName: 'Sibling' },
        tags: [],
        addedAt: '2026-01-01T00:00:00Z',
        trustLevel: 2,
        syncs: [],
      },
    ]);

    render(<ImportCardModal {...defaultProps} />);

    fireEvent.change(screen.getByPlaceholderText(/humoco:\/\/card/i), {
      target: {
        value: `humoco://contact?did=${encodeURIComponent(siblingSubDid)}&name=Jane%20Marie%20Doe&sync=${encodeURIComponent('https://sync.example.org/sync.php')}`,
      },
    });
    fireEvent.click(screen.getByRole('button', { name: /^Import$/i }));

    await waitFor(() => {
      expect(contactService.saveContact).toHaveBeenCalledWith(
        expect.objectContaining({
          did: siblingSubDid,
          trustLevel: 2,
          profile: expect.objectContaining({
            firstName: 'Jane Marie',
            lastName: 'Doe',
          }),
          syncs: [expect.objectContaining({ writeUrl: 'https://sync.example.org/sync.php' })],
        }),
        'mock-password'
      );
      expect(defaultProps.onImportSuccess).toHaveBeenCalledWith(siblingSubDid);
      expect(marketService.probeContactSyncs).toHaveBeenCalledWith(false);
    });

    expect(screen.getByTestId('import-sibling-prefix-trust-notice')).toBeInTheDocument();
  });

  it('deletes newly added contact when reverting to Level 0 with preserveManualCard', async () => {
    const bobContact = {
      did: 'did:key:z6MkuBob456',
      profile: { id: 'did:key:z6MkuBob456', firstName: 'Bob', lastName: 'Partner' },
      tags: [],
      addedAt: '2026-01-01T00:00:00Z',
      trustLevel: 2,
      syncs: [],
    };
    vi.mocked(marketService.importContactCard).mockResolvedValue({
      status: 'accepted',
      did: 'did:key:z6MkuBob456',
      replaced: false,
    });
    vi.mocked(contactService.getContacts)
      .mockResolvedValueOnce([]) // initial import
      .mockResolvedValueOnce([]) // clicking Level 2
      .mockResolvedValueOnce([bobContact]); // clicking Level 0 after Level 2

    render(<ImportCardModal {...defaultProps} />);
    fireEvent.change(screen.getByPlaceholderText(/humoco:\/\/card/i), {
      target: { value: 'humoco://card?env=bob' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^Import$/i }));

    const lvl2Btn = await screen.findByRole('button', { name: /Freund.*Stufe 2|Friend.*Level 2|Vertraut.*Stufe 2|Trusted.*Level 2/i });
    fireEvent.click(lvl2Btn);
    await waitFor(() => {
      expect(contactService.saveContact).toHaveBeenCalledTimes(1);
    });

    fireEvent.click(screen.getByRole('button', { name: /Nur im Marktplatz merken|Keep in marketplace only/i }));
    await waitFor(() => {
      expect(contactService.deleteContact).toHaveBeenCalledWith('did:key:z6MkuBob456', 'mock-password', {
        preserveManualCard: true,
      });
    });
  });

  it('clears trustLevel instead of deleting when pre-existing contact selects Level 0', async () => {
    const existingBob = {
      did: 'did:key:z6MkuBob456',
      profile: { id: 'did:key:z6MkuBob456', firstName: 'Bob', lastName: 'Partner' },
      tags: [],
      addedAt: '2026-01-01T00:00:00Z',
      trustLevel: 1,
      desiredHops: 2,
      syncs: [],
    };
    vi.mocked(marketService.importContactCard).mockResolvedValue({
      status: 'accepted',
      did: 'did:key:z6MkuBob456',
      replaced: false,
    });
    vi.mocked(contactService.getContacts).mockResolvedValue([existingBob]);

    render(<ImportCardModal {...defaultProps} />);
    fireEvent.change(screen.getByPlaceholderText(/humoco:\/\/card/i), {
      target: { value: 'humoco://card?env=bob' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^Import$/i }));

    const lvl0Btn = await screen.findByRole('button', { name: /Nur im Marktplatz merken|Keep in marketplace only/i });
    fireEvent.click(lvl0Btn);

    await waitFor(() => {
      expect(contactService.saveContact).toHaveBeenCalledWith(
        expect.objectContaining({
          did: 'did:key:z6MkuBob456',
          trustLevel: null,
          desiredHops: null,
        }),
        'mock-password',
        { preserveManualCard: true }
      );
      expect(contactService.deleteContact).not.toHaveBeenCalled();
    });
  });

  it('surfaces trust-level save errors cleanly', async () => {
    vi.mocked(marketService.importContactCard).mockResolvedValue({
      status: 'accepted',
      did: 'did:key:z6MkuBob456',
      replaced: false,
    });
    vi.mocked(contactService.saveContact).mockRejectedValueOnce(new Error('Keychain locked'));

    render(<ImportCardModal {...defaultProps} />);
    fireEvent.change(screen.getByPlaceholderText(/humoco:\/\/card/i), {
      target: { value: 'humoco://card?env=bob' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^Import$/i }));

    fireEvent.click(await screen.findByRole('button', { name: /Bekannt.*Stufe 1|Acquaintance.*Level 1|Known.*Level 1/i }));

    expect(await screen.findByText(/Fehler beim Speichern der Vertrauensstufe|Failed to save trust level/i)).toBeInTheDocument();
    expect(screen.getByText('Keychain locked')).toBeInTheDocument();
  });

  it('imports .hmcard file input and resets modal state on Done/Close', async () => {
    vi.mocked(marketService.importContactCard).mockResolvedValue({
      status: 'accepted',
      did: 'did:key:z6MkuBob456',
      replaced: false,
    });

    const onClose = vi.fn();
    const { baseElement } = render(
      <ImportCardModal isOpen={true} onClose={onClose} onImportSuccess={defaultProps.onImportSuccess} />
    );

    const fileInput = baseElement.querySelector('input[type="file"]') as HTMLInputElement;
    const filePayload = '{"card_jws":"file_jwt"}';
    const file = new File([filePayload], 'bob.hmcard', { type: 'application/json' });
    Object.defineProperty(file, 'text', { value: vi.fn().mockResolvedValue(filePayload) });

    fireEvent.change(fileInput, { target: { files: [file] } });

    await waitFor(() => {
      expect(marketService.importContactCard).toHaveBeenCalledWith(filePayload, 'mock-password');
    });

    const doneBtn = await screen.findByRole('button', { name: /^Fertig$|^Done$/i });
    fireEvent.click(doneBtn);

    expect(onClose).toHaveBeenCalledTimes(1);
    expect(screen.getByPlaceholderText(/humoco:\/\/card/i)).toHaveValue('');
  });
});
