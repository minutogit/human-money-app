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
});
