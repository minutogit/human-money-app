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
          published_at: 1000,
          expires_at: 2000,
          allow_inquiries: true,
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
});
