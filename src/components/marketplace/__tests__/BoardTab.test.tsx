import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { BoardTab } from '../BoardTab';
import { clearAdImageCache } from '../AdCard';
import { FrontendBoardItem, FrontendMarketPerson } from '../../../types';
import { marketService } from '../../../services/marketService';

describe('BoardTab Component', () => {
  beforeEach(() => {
    clearAdImageCache();
    vi.restoreAllMocks();
  });

  const nowSecs = Math.floor(Date.now() / 1000);

  const mockItems: FrontendBoardItem[] = [
    {
      ownerDid: 'did:key:z6MkuAlice',
      ownerName: 'Alice Honey',
      trust: 'trusted',
      ad: {
        id: 'ad-1',
        kind: 'offer',
        title: 'Bio-Honig 500g',
        text: 'Regionaler Blütenhonig',
        pricing: '5 FreeTaler',
        publishedAt: nowSecs - 3600,
        expiresAt: nowSecs + 86400 * 20,
        allowInquiries: true,
      },
      daysLeft: 20,
    },
    {
      ownerDid: 'did:key:z6MkuBob',
      ownerName: 'Bob Holz',
      trust: 'unknown',
      ad: {
        id: 'ad-2',
        kind: 'request',
        title: 'Suche Balken',
        text: 'Alte Eichenbalken gesucht',
        pricing: 'Tausch möglich',
        publishedAt: nowSecs - 7200,
        expiresAt: nowSecs + 86400 * 10,
        allowInquiries: true,
      },
      daysLeft: 10,
    },
  ];

  it('renders search input and filter chips, and filters correctly', () => {
    render(
      <BoardTab
        items={mockItems}
        hasSyncService={true}
        onCreateAd={vi.fn()}
        onImportCard={vi.fn()}
        onNavigateToSyncSettings={vi.fn()}
      />
    );

    expect(screen.getByText('Bio-Honig 500g')).toBeInTheDocument();
    expect(screen.getByText('Suche Balken')).toBeInTheDocument();

    // Filter by offers
    const offersBtn = screen.getByRole('button', { name: /Nur Angebote|Offers only/i });
    fireEvent.click(offersBtn);

    expect(screen.getByText('Bio-Honig 500g')).toBeInTheDocument();
    expect(screen.queryByText('Suche Balken')).not.toBeInTheDocument();

    // Filter by search (text)
    const searchInput = screen.getByPlaceholderText(/Inserate nach Titel, Text oder Name durchsuchen|Search ads/i);
    fireEvent.change(searchInput, { target: { value: 'Blütenhonig' } });
    expect(screen.getByText('Bio-Honig 500g')).toBeInTheDocument();
    expect(screen.queryByText('Suche Balken')).not.toBeInTheDocument();

    // Reset to all and search by ownerName
    fireEvent.click(screen.getByTestId('filter-chip-all'));
    fireEvent.change(searchInput, { target: { value: 'Bob Holz' } });
    expect(screen.getByText('Suche Balken')).toBeInTheDocument();
    expect(screen.queryByText('Bio-Honig 500g')).not.toBeInTheDocument();

    // Requests chip filters out offers
    fireEvent.change(searchInput, { target: { value: '' } });
    fireEvent.click(screen.getByTestId('filter-chip-requests'));
    expect(screen.queryByText('Bio-Honig 500g')).not.toBeInTheDocument();
    expect(screen.getByText('Suche Balken')).toBeInTheDocument();
  });

  it('renders empty state without sync service and handles setup actions', () => {
    const onNavigateToSync = vi.fn();
    const onImport = vi.fn();

    render(
      <BoardTab
        items={[]}
        hasSyncService={false}
        onCreateAd={vi.fn()}
        onImportCard={onImport}
        onNavigateToSyncSettings={onNavigateToSync}
      />
    );

    expect(screen.getByText(/Noch keine Inserate vorhanden|No ads available yet/i)).toBeInTheDocument();

    const setupSyncBtn = screen.getByRole('button', { name: /Sync-Dienst einrichten|Setup sync service/i });
    fireEvent.click(setupSyncBtn);
    expect(onNavigateToSync).toHaveBeenCalled();

    const importBtn = screen.getByRole('button', { name: /Visitenkarte öffnen|Open business card/i });
    fireEvent.click(importBtn);
    expect(onImport).toHaveBeenCalled();
  });

  it('renders empty state with sync service and handles create ad', () => {
    const onCreateAd = vi.fn();

    render(
      <BoardTab
        items={[]}
        hasSyncService={true}
        onCreateAd={onCreateAd}
        onImportCard={vi.fn()}
        onNavigateToSyncSettings={vi.fn()}
      />
    );

    expect(screen.getByText(/Die Pinnwand ist leer|The board is empty/i)).toBeInTheDocument();

    const createBtn = screen.getByRole('button', { name: /Inserat aufgeben|Post ad/i });
    fireEvent.click(createBtn);
    expect(onCreateAd).toHaveBeenCalled();
  });

  it('filters ads by distance and always includes remote ads', () => {
    const mockPeople: FrontendMarketPerson[] = [
      {
        did: 'did:key:z6MkuAlice',
        profile: { id: 'did:key:z6MkuAlice', coordinates: '52.52, 13.40' }, // Berlin
        skills: [],
        activeAds: [],
        adsPending: 0,
        allowInquiries: true,
        sources: [],
        asOf: nowSecs,
        standards: [],
        trust: 'trusted',
        canForward: false,
        cardSyncs: [],
      },
      {
        did: 'did:key:z6MkuBob',
        profile: { id: 'did:key:z6MkuBob', coordinates: '48.14, 11.58' }, // Munich (~500 km)
        skills: [],
        activeAds: [],
        adsPending: 0,
        allowInquiries: true,
        sources: [],
        asOf: nowSecs,
        standards: [],
        trust: 'trusted',
        canForward: false,
        cardSyncs: [],
      },
    ];

    const remoteAdItem: FrontendBoardItem = {
      ownerDid: 'did:key:z6MkuBob',
      ownerName: 'Bob Online',
      trust: 'trusted',
      ad: {
        id: 'ad-3',
        kind: 'offer',
        title: 'Online Beratung',
        publishedAt: nowSecs,
        expiresAt: nowSecs + 86400 * 10,
        allowInquiries: true,
        isRemote: true,
      },
      daysLeft: 10,
    };

    const { rerender } = render(
      <BoardTab
        items={[...mockItems, remoteAdItem]}
        people={mockPeople}
        myCoordinates="52.52, 13.40"
        hasSyncService={true}
        onCreateAd={vi.fn()}
        onImportCard={vi.fn()}
        onNavigateToSyncSettings={vi.fn()}
      />
    );

    // Initially all 3 ads are shown
    expect(screen.getByText('Bio-Honig 500g')).toBeInTheDocument();
    expect(screen.getByText('Suche Balken')).toBeInTheDocument();
    expect(screen.getByText('Online Beratung')).toBeInTheDocument();

    // Select 25 km radius
    const radiusSelect = screen.getByTestId('board-radius-select');
    fireEvent.change(radiusSelect, { target: { value: '25' } });

    // Alice is 0 km (shown), remote ad is shown, Bob is ~500 km (filtered out)
    expect(screen.getByText('Bio-Honig 500g')).toBeInTheDocument();
    expect(screen.getByText('Online Beratung')).toBeInTheDocument();
    expect(screen.queryByText('Suche Balken')).not.toBeInTheDocument();

    // If myCoordinates is missing and radius is selected, show warning
    rerender(
      <BoardTab
        items={[...mockItems, remoteAdItem]}
        people={mockPeople}
        myCoordinates={null}
        hasSyncService={true}
        onCreateAd={vi.fn()}
        onImportCard={vi.fn()}
        onNavigateToSyncSettings={vi.fn()}
      />
    );

    expect(screen.getByTestId('missing-coordinates-hint')).toBeInTheDocument();
  });

  it('always retains own ads in distance filter regardless of distance or missing person entry', () => {
    const ownAdItem: FrontendBoardItem = {
      ownerDid: 'did:key:z6MkuMe',
      ownerName: 'Ich',
      trust: 'trusted',
      isOwn: true,
      ad: {
        id: 'ad-own',
        kind: 'offer',
        title: 'Mein eigenes Angebot',
        publishedAt: nowSecs,
        expiresAt: nowSecs + 86400 * 30,
        allowInquiries: true,
        isRemote: false,
      },
      daysLeft: 30,
    };

    render(
      <BoardTab
        items={[ownAdItem]}
        people={[]}
        myCoordinates="52.52, 13.40"
        maxDistanceKm={10}
        hasSyncService={true}
        onCreateAd={vi.fn()}
        onImportCard={vi.fn()}
        onNavigateToSyncSettings={vi.fn()}
      />
    );

    expect(screen.getByText('Mein eigenes Angebot')).toBeInTheDocument();
    expect(screen.getByTestId('ad-own-badge')).toBeInTheDocument();
  });

  it('disables board radius select and shows missing-coordinates hint when myCoordinates is missing', () => {
    render(
      <BoardTab
        items={mockItems}
        hasSyncService={true}
        myCoordinates={null}
        onCreateAd={vi.fn()}
        onImportCard={vi.fn()}
        onNavigateToSyncSettings={vi.fn()}
      />
    );

    expect(screen.getByTestId('board-radius-select')).toBeDisabled();
    expect(screen.getByTestId('missing-coordinates-hint')).toBeInTheDocument();
  });

  it('filters by Events chip and sorts events chronologically ascending by eventAt', () => {
    const laterEvent: FrontendBoardItem = {
      ownerDid: 'did:key:z6MkuAlice',
      ownerName: 'Alice',
      trust: 'trusted',
      ad: {
        id: 'ev-later',
        kind: 'event',
        title: 'Späterer Workshop',
        publishedAt: nowSecs,
        expiresAt: nowSecs + 86400 * 21,
        allowInquiries: true,
        eventAt: nowSecs + 86400 * 20,
        eventAllDay: true,
      },
      daysLeft: 21,
    };
    const soonerEvent: FrontendBoardItem = {
      ownerDid: 'did:key:z6MkuBob',
      ownerName: 'Bob',
      trust: 'trusted',
      ad: {
        id: 'ev-sooner',
        kind: 'event',
        title: 'Morgiger Stammtisch',
        publishedAt: nowSecs - 1000,
        expiresAt: nowSecs + 86400 * 2,
        allowInquiries: true,
        eventAt: nowSecs + 86400 * 1,
        eventAllDay: false,
      },
      daysLeft: 2,
    };

    render(
      <BoardTab
        items={[...mockItems, laterEvent, soonerEvent]}
        hasSyncService={true}
        onCreateAd={vi.fn()}
        onImportCard={vi.fn()}
        onNavigateToSyncSettings={vi.fn()}
      />
    );

    fireEvent.click(screen.getByTestId('filter-chip-events'));

    expect(screen.queryByText('Bio-Honig 500g')).not.toBeInTheDocument();
    expect(screen.queryByText('Suche Balken')).not.toBeInTheDocument();

    const cards = screen.getAllByTestId(/^ad-card-ev-/);
    expect(cards.map((el) => el.getAttribute('data-testid'))).toEqual([
      'ad-card-ev-sooner',
      'ad-card-ev-later',
    ]);
  });

  it('does not hide non-remote ads when myCoordinates is whitespace or unparseable while maxDistanceKm is set', () => {
    const mockPeople: FrontendMarketPerson[] = [
      {
        did: 'did:key:z6MkuAlice',
        profile: { id: 'did:key:z6MkuAlice', coordinates: '52.52, 13.40' },
        skills: [],
        activeAds: [],
        adsPending: 0,
        allowInquiries: true,
        sources: [],
        asOf: nowSecs,
        standards: [],
        trust: 'trusted',
        canForward: false,
        cardSyncs: [],
      },
      {
        did: 'did:key:z6MkuBob',
        profile: { id: 'did:key:z6MkuBob', coordinates: '48.14, 11.58' },
        skills: [],
        activeAds: [],
        adsPending: 0,
        allowInquiries: true,
        sources: [],
        asOf: nowSecs,
        standards: [],
        trust: 'unknown',
        canForward: false,
        cardSyncs: [],
      },
    ];

    const { rerender } = render(
      <BoardTab
        items={mockItems}
        people={mockPeople}
        myCoordinates="   "
        maxDistanceKm={25}
        hasSyncService={true}
        onCreateAd={vi.fn()}
        onImportCard={vi.fn()}
        onNavigateToSyncSettings={vi.fn()}
      />
    );

    expect(screen.getByTestId('board-radius-select')).toBeDisabled();
    expect(screen.getByTestId('missing-coordinates-hint')).toBeInTheDocument();
    expect(screen.getByText('Bio-Honig 500g')).toBeInTheDocument();
    expect(screen.getByText('Suche Balken')).toBeInTheDocument();

    rerender(
      <BoardTab
        items={mockItems}
        people={mockPeople}
        myCoordinates="not-a-coord"
        maxDistanceKm={25}
        hasSyncService={true}
        onCreateAd={vi.fn()}
        onImportCard={vi.fn()}
        onNavigateToSyncSettings={vi.fn()}
      />
    );

    expect(screen.getByTestId('board-radius-select')).toBeDisabled();
    expect(screen.getByTestId('missing-coordinates-hint')).toBeInTheDocument();
    expect(screen.getByText('Bio-Honig 500g')).toBeInTheDocument();
    expect(screen.getByText('Suche Balken')).toBeInTheDocument();
  });

  it('shows empty filter state and resets search, kind, and controlled radius when reset is clicked', () => {
    const onMaxDistanceChange = vi.fn();
    const { rerender } = render(
      <BoardTab
        items={mockItems}
        myCoordinates="52.52, 13.40"
        maxDistanceKm={25}
        onMaxDistanceChange={(val) => {
          onMaxDistanceChange(val);
          rerender(
            <BoardTab
              items={mockItems}
              myCoordinates="52.52, 13.40"
              maxDistanceKm={val}
              onMaxDistanceChange={onMaxDistanceChange}
              hasSyncService={true}
              onCreateAd={vi.fn()}
              onImportCard={vi.fn()}
              onNavigateToSyncSettings={vi.fn()}
            />
          );
        }}
        hasSyncService={true}
        onCreateAd={vi.fn()}
        onImportCard={vi.fn()}
        onNavigateToSyncSettings={vi.fn()}
      />
    );

    fireEvent.click(screen.getByTestId('filter-chip-offers'));
    fireEvent.change(screen.getByTestId('board-search-input'), { target: { value: 'does-not-exist' } });

    expect(screen.getByText(/Keine passenden Inserate gefunden|No matching ads found/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /Filter zurücksetzen|Reset filters/i }));

    expect(onMaxDistanceChange).toHaveBeenCalledWith(null);
    expect(screen.getByTestId('board-search-input')).toHaveValue('');
    expect(screen.getByText('Bio-Honig 500g')).toBeInTheDocument();
    expect(screen.getByText('Suche Balken')).toBeInTheDocument();
  });

  it('excludes non-remote ads without owner coordinates and resolves recipientDid for prefixed accounts', () => {
    const aliceSub = 'shop:ab12@did:key:z6MkuAlice';
    const subAccountItems: FrontendBoardItem[] = [
      {
        ownerDid: 'did:key:z6MkuAlice',
        recipientDid: aliceSub,
        ownerName: 'Alice Shop',
        trust: 'trusted',
        ad: {
          id: 'ad-sub-alice',
          kind: 'offer',
          title: 'Subaccount Käse',
          publishedAt: nowSecs,
          expiresAt: nowSecs + 86400 * 10,
          allowInquiries: true,
        },
        daysLeft: 10,
      },
      {
        ownerDid: 'did:key:z6MkuNoCoords',
        ownerName: 'No Location',
        trust: 'unknown',
        ad: {
          id: 'ad-no-coords',
          kind: 'offer',
          title: 'Ohne Standort',
          publishedAt: nowSecs,
          expiresAt: nowSecs + 86400 * 10,
          allowInquiries: true,
        },
        daysLeft: 10,
      },
    ];
    const people: FrontendMarketPerson[] = [
      {
        did: 'did:key:z6MkuAlice',
        recipientDid: aliceSub,
        profile: { id: aliceSub, coordinates: '52.52, 13.40' },
        skills: [],
        activeAds: [],
        adsPending: 0,
        allowInquiries: true,
        sources: [{ kind: 'card', hops: 1 }],
        asOf: nowSecs,
        standards: [],
        trust: 'trusted',
        canForward: true,
        cardSyncs: [],
      },
      {
        did: 'did:key:z6MkuNoCoords',
        profile: { id: 'did:key:z6MkuNoCoords' },
        skills: [],
        activeAds: [],
        adsPending: 0,
        allowInquiries: true,
        sources: [{ kind: 'card', hops: 1 }],
        asOf: nowSecs,
        standards: [],
        trust: 'unknown',
        canForward: false,
        cardSyncs: [],
      },
    ];

    render(
      <BoardTab
        items={subAccountItems}
        people={people}
        myCoordinates="52.52, 13.40"
        maxDistanceKm={10}
        hasSyncService={true}
        onCreateAd={vi.fn()}
        onImportCard={vi.fn()}
        onNavigateToSyncSettings={vi.fn()}
      />
    );

    expect(screen.getByText('Subaccount Käse')).toBeInTheDocument();
    expect(screen.queryByText('Ohne Standort')).not.toBeInTheDocument();
  });

  it('notifies onMaxDistanceChange when selecting a radius or clearing back to all distances', () => {
    const onMaxDistanceChange = vi.fn();
    render(
      <BoardTab
        items={mockItems}
        myCoordinates="52.52, 13.40"
        maxDistanceKm={null}
        onMaxDistanceChange={onMaxDistanceChange}
        hasSyncService={true}
        onCreateAd={vi.fn()}
        onImportCard={vi.fn()}
        onNavigateToSyncSettings={vi.fn()}
      />
    );

    const select = screen.getByTestId('board-radius-select');
    fireEvent.change(select, { target: { value: '50' } });
    expect(onMaxDistanceChange).toHaveBeenCalledWith(50);

    fireEvent.change(select, { target: { value: '' } });
    expect(onMaxDistanceChange).toHaveBeenCalledWith(null);
  });

  it('wires Pay, Inquire, Owner, Edit callbacks and defers image loading using authorHops from people', () => {
    const getAdImageSpy = vi.spyOn(marketService, 'getAdImage').mockResolvedValue('data:image/webp;base64,ok');
    const onPay = vi.fn();
    const onInquire = vi.fn();
    const onOwnerClick = vi.fn();
    const onEditOwnAd = vi.fn();

    const targetDid = 'farm:ab12@did:key:z6MkuFarAlice';
    const itemsWithPrefixedAndOwn: FrontendBoardItem[] = [
      {
        ownerDid: 'did:key:z6MkuFarAlice',
        recipientDid: targetDid,
        ownerName: 'Far Alice',
        trust: 'trusted',
        ad: {
          id: 'ad-hop3-img',
          kind: 'offer',
          title: 'Hop 3 Bild-Inserat',
          imageToken: 'tok-hop3',
          publishedAt: nowSecs,
          expiresAt: nowSecs + 86400 * 10,
          allowInquiries: true,
        },
        daysLeft: 10,
      },
      {
        ownerDid: 'did:key:z6MkuSelf',
        ownerName: 'Ich',
        trust: 'trusted',
        isOwn: true,
        ad: {
          id: 'ad-own-board',
          kind: 'offer',
          title: 'Eigenes Inserat',
          publishedAt: nowSecs,
          expiresAt: nowSecs + 86400 * 20,
          allowInquiries: true,
        },
        daysLeft: 20,
      },
    ];
    const people: FrontendMarketPerson[] = [
      {
        did: 'did:key:z6MkuFarAlice',
        recipientDid: targetDid,
        profile: { id: targetDid },
        skills: [],
        activeAds: [],
        adsPending: 0,
        allowInquiries: true,
        sources: [{ kind: 'card', hops: 3 }],
        asOf: nowSecs,
        standards: [],
        trust: 'trusted',
        canForward: false,
        cardSyncs: [],
      },
    ];

    render(
      <BoardTab
        items={itemsWithPrefixedAndOwn}
        people={people}
        mediaAutoLoadMaxHops={2}
        hasSyncService={true}
        onCreateAd={vi.fn()}
        onImportCard={vi.fn()}
        onNavigateToSyncSettings={vi.fn()}
        onPay={onPay}
        onInquire={onInquire}
        onOwnerClick={onOwnerClick}
        onEditOwnAd={onEditOwnAd}
      />
    );

    // Hop 3 ad image must be deferred instead of auto-fetched
    expect(screen.getByTestId('ad-card-image-placeholder')).toBeInTheDocument();
    expect(getAdImageSpy).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: /Gutschein senden|Send voucher/i }));
    expect(onPay).toHaveBeenCalledWith(targetDid);

    fireEvent.click(screen.getByRole('button', { name: /^Anfragen$|^Inquire$/i }));
    expect(onInquire).toHaveBeenCalledWith(itemsWithPrefixedAndOwn[0].ad, targetDid, 'Far Alice');

    fireEvent.click(screen.getByText('Far Alice'));
    expect(onOwnerClick).toHaveBeenCalledWith(targetDid);

    fireEvent.click(screen.getByTestId('ad-edit-own-btn'));
    expect(onEditOwnAd).toHaveBeenCalledWith(itemsWithPrefixedAndOwn[1].ad);
  });
});
