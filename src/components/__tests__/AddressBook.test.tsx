import { render, screen, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import AddressBook from '../AddressBook';
import { invoke } from '@tauri-apps/api/core';
import { Contact } from '../../types';

// Mock the Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock('./ContactDialog', () => ({
  default: ({ isOpen, onClose }: any) => {
    if (!isOpen) return null;
    return (
      <div data-testid="contact-dialog">
        <button onClick={onClose}>Cancel</button>
      </div>
    );
  },
}));

describe('AddressBook Component', () => {
  const mockContacts: Contact[] = [
    {
      did: 'did:key:z123',
      profile: {
        id: 'did:key:z123',
        first_name: 'John',
        last_name: 'Doe',
      },
      tags: ['friend'],
      added_at: '2024-01-01',
    },
    {
      did: 'did:key:z456',
      profile: {
        id: 'did:key:z456',
        first_name: 'Jane',
        last_name: 'Smith',
      },
      tags: ['colleague'],
      added_at: '2024-01-02',
    },
  ];

  const mockOnBack = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_contacts') {
        return Promise.resolve(mockContacts);
      }
      return Promise.resolve(undefined);
    });
  });

  it('renders address book header and contact list', async () => {
    render(<AddressBook onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getByText(/Address Book/i)).toBeInTheDocument();
    });

    await waitFor(() => {
      expect(screen.getByText(/John Doe/i)).toBeInTheDocument();
      expect(screen.getByText(/Jane Smith/i)).toBeInTheDocument();
    });
  });

  it('loads contacts from backend on mount', async () => {
    render(<AddressBook onBack={mockOnBack} />);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('get_contacts');
    });
  });

  it('displays contact tags', async () => {
    render(<AddressBook onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getAllByText(/friend/i)[0]).toBeInTheDocument();
      expect(screen.getAllByText(/colleague/i)[0]).toBeInTheDocument();
    });
  });
});
