import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { BoardTab } from '../BoardTab';
import { FrontendBoardItem } from '../../../types';

describe('BoardTab Component', () => {
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
        published_at: nowSecs - 3600,
        expires_at: nowSecs + 86400 * 20,
        allow_inquiries: true,
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
        published_at: nowSecs - 7200,
        expires_at: nowSecs + 86400 * 10,
        allow_inquiries: true,
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

    // Filter by search
    const searchInput = screen.getByPlaceholderText(/Inserate nach Titel, Text oder Name durchsuchen|Search ads/i);
    fireEvent.change(searchInput, { target: { value: 'Blütenhonig' } });
    expect(screen.getByText('Bio-Honig 500g')).toBeInTheDocument();
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
    const mockPeople: any[] = [
      {
        did: 'did:key:z6MkuAlice',
        profile: { coordinates: '52.52, 13.40' }, // Berlin
      },
      {
        did: 'did:key:z6MkuBob',
        profile: { coordinates: '48.14, 11.58' }, // Munich (~500 km)
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
        published_at: nowSecs,
        expires_at: nowSecs + 86400 * 10,
        allow_inquiries: true,
        is_remote: true,
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
});
