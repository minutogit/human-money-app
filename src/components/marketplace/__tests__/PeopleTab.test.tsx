import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { PeopleTab } from '../PeopleTab';
import { FrontendMarketPerson } from '../../../types';

describe('PeopleTab Component', () => {
  const mockPeople: FrontendMarketPerson[] = [
    {
      did: 'did:key:z6MkuAlice',
      profile: {
        id: 'alice-id',
        firstName: 'Alice',
        lastName: 'Wonderland',
        organization: 'Alice Honey',
        address: { city: 'Berlin' },
      },
      trust: 'trusted',
      skills: [
        { title: 'Imkerei', text: 'Bio-Honig direkt vom Imker', origin: 'card' },
        { title: 'Kerzenziehen', text: 'Bienenwachskerzen', origin: 'card' },
      ],
      activeAds: [],
      adsPending: 0,
      allowInquiries: true,
      sources: [{ kind: 'card', hops: 1 }],
      standards: [],
      cardSyncs: [],
      asOf: 1000,
      canForward: true,
    },
    {
      did: 'did:key:z6MkuBob',
      profile: {
        id: 'bob-id',
        firstName: 'Bob',
        lastName: 'Builder',
        community: 'Handwerkerbund',
        address: { city: 'Hamburg' },
      },
      trust: 'unknown',
      skills: [
        { title: 'Zimmerei', text: 'Holzbau', origin: 'voucher' },
      ],
      activeAds: [
        {
          id: 'ad-bob',
          kind: 'offer',
          title: 'Bauholz abzugeben',
          publishedAt: 1000,
          expiresAt: 2000,
          allowInquiries: true,
        },
      ],
      adsPending: 0,
      allowInquiries: true,
      sources: [{ kind: 'card', hops: 1 }],
      standards: [],
      cardSyncs: [],
      asOf: 1000,
      canForward: false,
    },
  ];

  it('renders people list with names, locations, and skills', () => {
    const onSelectPerson = vi.fn();

    render(
      <PeopleTab
        people={mockPeople}
        onSelectPerson={onSelectPerson}
      />
    );

    expect(screen.getByText('Alice Wonderland')).toBeInTheDocument();
    expect(screen.getByText('Bob Builder')).toBeInTheDocument();
    expect(screen.getByText('Berlin')).toBeInTheDocument();
    expect(screen.getByText('Hamburg · Handwerkerbund')).toBeInTheDocument();
    expect(screen.getByText(/^1 Inserat$|^1 ad$/i)).toBeInTheDocument();
    expect(screen.getByText('Imkerei')).toBeInTheDocument();
    expect(screen.getByText('Zimmerei')).toBeInTheDocument();

    // Select person
    const aliceCard = screen.getByTestId('person-card-did:key:z6MkuAlice');
    fireEvent.click(aliceCard);
    expect(onSelectPerson).toHaveBeenCalledWith(mockPeople[0]);
  });

  it('filters people by search input', () => {
    render(
      <PeopleTab
        people={mockPeople}
        onSelectPerson={vi.fn()}
      />
    );

    const searchInput = screen.getByPlaceholderText(/Wer kann was\?|Who can do what\?/i);
    fireEvent.change(searchInput, { target: { value: 'Hamburg' } });

    expect(screen.queryByText('Alice Wonderland')).not.toBeInTheDocument();
    expect(screen.getByText('Bob Builder')).toBeInTheDocument();
  });

  it('renders empty state when people list is empty', () => {
    render(
      <PeopleTab
        people={[]}
        onSelectPerson={vi.fn()}
      />
    );

    expect(screen.getByText(/Noch keine Visitenkarten vorhanden|No business cards available yet/i)).toBeInTheDocument();
  });

  it('filters people by radius and retains people with remote skills', () => {
    const peopleWithCoords: FrontendMarketPerson[] = [
      {
        ...mockPeople[0],
        profile: { ...mockPeople[0].profile, coordinates: '52.52, 13.40' },
      },
      {
        ...mockPeople[1],
        profile: { ...mockPeople[1].profile, coordinates: '48.14, 11.58' },
      },
      {
        ...mockPeople[1],
        did: 'did:key:z6MkuRemoteCharlie',
        profile: { ...mockPeople[1].profile, firstName: 'Charlie', lastName: 'Online', coordinates: '48.14, 11.58' },
        skills: [{ title: 'Online Yoga', origin: 'card', isRemote: true }],
      },
    ];

    const { rerender } = render(
      <PeopleTab
        people={peopleWithCoords}
        myCoordinates="52.52, 13.40"
        onSelectPerson={vi.fn()}
      />
    );

    // All 3 initially visible
    expect(screen.getByText('Alice Wonderland')).toBeInTheDocument();
    expect(screen.getByText('Bob Builder')).toBeInTheDocument();
    expect(screen.getByText('Charlie Online')).toBeInTheDocument();

    // Select 50 km radius
    const radiusSelect = screen.getByTestId('people-radius-select');
    fireEvent.change(radiusSelect, { target: { value: '50' } });

    // Alice (0 km) and Charlie (has isRemote skill) are shown; Bob (~500 km, no remote) is filtered out
    expect(screen.getByText('Alice Wonderland')).toBeInTheDocument();
    expect(screen.getByText('Charlie Online')).toBeInTheDocument();
    expect(screen.queryByText('Bob Builder')).not.toBeInTheDocument();

    // When myCoordinates is missing and radius is selected, show warning
    rerender(
      <PeopleTab
        people={peopleWithCoords}
        myCoordinates={null}
        onSelectPerson={vi.fn()}
      />
    );

    expect(screen.getByTestId('missing-coordinates-hint')).toBeInTheDocument();
  });

  it('does not hide non-remote people when myCoordinates is whitespace or unparseable while maxDistanceKm is set', () => {
    const peopleWithCoords: FrontendMarketPerson[] = [
      {
        ...mockPeople[0],
        profile: { ...mockPeople[0].profile, coordinates: '52.52, 13.40' },
      },
      {
        ...mockPeople[1],
        profile: { ...mockPeople[1].profile, coordinates: '48.14, 11.58' },
      },
    ];

    const { rerender } = render(
      <PeopleTab
        people={peopleWithCoords}
        myCoordinates="   "
        maxDistanceKm={25}
        onSelectPerson={vi.fn()}
      />
    );

    expect(screen.getByTestId('people-radius-select')).toBeDisabled();
    expect(screen.getByTestId('missing-coordinates-hint')).toBeInTheDocument();
    expect(screen.getByText('Alice Wonderland')).toBeInTheDocument();
    expect(screen.getByText('Bob Builder')).toBeInTheDocument();

    rerender(
      <PeopleTab
        people={peopleWithCoords}
        myCoordinates="invalid-coords"
        maxDistanceKm={25}
        onSelectPerson={vi.fn()}
      />
    );

    expect(screen.getByTestId('people-radius-select')).toBeDisabled();
    expect(screen.getByTestId('missing-coordinates-hint')).toBeInTheDocument();
    expect(screen.getByText('Alice Wonderland')).toBeInTheDocument();
    expect(screen.getByText('Bob Builder')).toBeInTheDocument();
  });

  it('filters people across organization, community, skill title, and skill text case-insensitively', () => {
    render(<PeopleTab people={mockPeople} onSelectPerson={vi.fn()} />);
    const searchInput = screen.getByTestId('people-search-input');

    fireEvent.change(searchInput, { target: { value: 'alice honey' } });
    expect(screen.getByText('Alice Wonderland')).toBeInTheDocument();
    expect(screen.queryByText('Bob Builder')).not.toBeInTheDocument();

    fireEvent.change(searchInput, { target: { value: 'handwerkerbund' } });
    expect(screen.queryByText('Alice Wonderland')).not.toBeInTheDocument();
    expect(screen.getByText('Bob Builder')).toBeInTheDocument();

    fireEvent.change(searchInput, { target: { value: 'kerzenziehen' } });
    expect(screen.getByText('Alice Wonderland')).toBeInTheDocument();
    expect(screen.queryByText('Bob Builder')).not.toBeInTheDocument();

    fireEvent.change(searchInput, { target: { value: 'holzbau' } });
    expect(screen.queryByText('Alice Wonderland')).not.toBeInTheDocument();
    expect(screen.getByText('Bob Builder')).toBeInTheDocument();
  });

  it('renders empty filter state and resets query and controlled distance filter', () => {
    const onMaxDistanceChange = vi.fn();
    const { rerender } = render(
      <PeopleTab
        people={mockPeople}
        myCoordinates="52.52, 13.40"
        maxDistanceKm={25}
        onMaxDistanceChange={(val) => {
          onMaxDistanceChange(val);
          rerender(
            <PeopleTab
              people={mockPeople}
              myCoordinates="52.52, 13.40"
              maxDistanceKm={val}
              onMaxDistanceChange={onMaxDistanceChange}
              onSelectPerson={vi.fn()}
            />
          );
        }}
        onSelectPerson={vi.fn()}
      />
    );

    fireEvent.change(screen.getByTestId('people-search-input'), { target: { value: 'zzzz-no-match' } });
    expect(screen.getByText(/Keine Kontakte zu deiner Suchanfrage gefunden|No contacts found matching your search/i)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /Filter zurücksetzen|Reset filters/i }));
    expect(onMaxDistanceChange).toHaveBeenCalledWith(null);
    expect(screen.getByTestId('people-search-input')).toHaveValue('');
    expect(screen.getByText('Alice Wonderland')).toBeInTheDocument();
    expect(screen.getByText('Bob Builder')).toBeInTheDocument();
  });

  it('retains far person with remote activeAd and excludes non-remote person missing coordinates under radius filter', () => {
    const peopleMixed: FrontendMarketPerson[] = [
      {
        ...mockPeople[0],
        did: 'did:key:z6MkuRemoteAdPerson',
        profile: { ...mockPeople[0].profile, firstName: 'Remote', lastName: 'AdOwner', coordinates: '48.14, 11.58' },
        skills: [],
        activeAds: [{ ...mockPeople[1].activeAds[0], id: 'ad-rem', isRemote: true }],
      },
      {
        ...mockPeople[1],
        did: 'did:key:z6MkuNoCoordBob',
        profile: { id: 'bob-nocoord', firstName: 'NoCoord', lastName: 'Person' },
        skills: [{ title: 'Tischlerei', origin: 'card' }],
        activeAds: [],
      },
    ];

    render(
      <PeopleTab
        people={peopleMixed}
        myCoordinates="52.52, 13.40"
        maxDistanceKm={10}
        onSelectPerson={vi.fn()}
      />
    );

    expect(screen.getByText('Remote AdOwner')).toBeInTheDocument();
    expect(screen.queryByText('NoCoord Person')).not.toBeInTheDocument();
  });

  it('renders organization and truncated DID name fallbacks, plural ads badge, and +N skill overflow cap', () => {
    const longDid = 'did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH';
    const richPeople: FrontendMarketPerson[] = [
      {
        ...mockPeople[0],
        did: 'did:key:z6MkuOrgOnly',
        profile: { id: 'org-only', organization: 'Solidarhof e.V.' },
        skills: [
          { title: 'Skill 1', origin: 'card' },
          { title: 'Skill 2', origin: 'card' },
          { title: 'Skill 3', origin: 'card' },
          { title: 'Skill 4', origin: 'card' },
          { title: 'Skill 5', origin: 'card' },
        ],
        activeAds: [
          mockPeople[1].activeAds[0],
          { ...mockPeople[1].activeAds[0], id: 'ad-bob-2', title: 'Zweites Inserat' },
        ],
      },
      {
        ...mockPeople[1],
        did: longDid,
        profile: { id: longDid },
        skills: [],
        activeAds: [],
      },
    ];

    render(<PeopleTab people={richPeople} onSelectPerson={vi.fn()} />);

    expect(screen.getByText('Solidarhof e.V.')).toBeInTheDocument();
    expect(screen.getByText('did:key:z6MkpTHR8VNsBxYAAWHu...dvktH')).toBeInTheDocument();
    expect(screen.getByText(/^2 Inserate$|^2 ads$/i)).toBeInTheDocument();
    expect(screen.getByText('Skill 1')).toBeInTheDocument();
    expect(screen.getByText('Skill 2')).toBeInTheDocument();
    expect(screen.getByText('Skill 3')).toBeInTheDocument();
    expect(screen.queryByText('Skill 4')).not.toBeInTheDocument();
    expect(screen.getByText('+2')).toBeInTheDocument();
  });

  it('disables radius select without coordinates and propagates controlled radius changes', () => {
    const onMaxDistanceChange = vi.fn();
    const { rerender } = render(
      <PeopleTab
        people={mockPeople}
        myCoordinates={null}
        onMaxDistanceChange={onMaxDistanceChange}
        onSelectPerson={vi.fn()}
      />
    );

    expect(screen.getByTestId('people-radius-select')).toBeDisabled();

    rerender(
      <PeopleTab
        people={mockPeople}
        myCoordinates="52.52, 13.40"
        maxDistanceKm={null}
        onMaxDistanceChange={onMaxDistanceChange}
        onSelectPerson={vi.fn()}
      />
    );

    const select = screen.getByTestId('people-radius-select');
    expect(select).toBeEnabled();
    fireEvent.change(select, { target: { value: '10' } });
    expect(onMaxDistanceChange).toHaveBeenCalledWith(10);

    fireEvent.change(select, { target: { value: '' } });
    expect(onMaxDistanceChange).toHaveBeenCalledWith(null);
  });
});
