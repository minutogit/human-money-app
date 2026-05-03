import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import AddressBook from '../AddressBook';
import { invoke } from '@tauri-apps/api/core';
import { Contact } from '../../types';

// Mock the Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('../../context/SessionContext', () => ({
  useSession: () => ({
    protectAction: vi.fn((action) => action(undefined)),
  }),
}));

vi.mock('../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock('../ContactDialog', () => ({
  default: ({ isOpen, onClose, onSave }: any) => {
    if (!isOpen) return null;
    return (
      <div data-testid="contact-dialog">
        <button onClick={() => onSave({ did: 'did:key:z789', profile: { id: 'did:key:z789', firstName: 'New', lastName: 'Contact' }, tags: [], addedAt: '2024-01-03' })}>Save</button>
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
        firstName: 'John',
        lastName: 'Doe',
      },
      tags: ['friend'],
      addedAt: '2024-01-01',
    },
    {
      did: 'did:key:z456',
      profile: {
        id: 'did:key:z456',
        firstName: 'Jane',
        lastName: 'Smith',
      },
      tags: ['colleague'],
      addedAt: '2024-01-02',
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

  it('calls save_contact via protectAction when saving a contact', async () => {
    render(<AddressBook onBack={mockOnBack} />);
    
    // Wait for initial load
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_contacts'));
    (invoke as Mock).mockClear(); // Clear the initial 'get_contacts' call
    
    const newBtn = screen.getByText(/New Contact/i);
    fireEvent.click(newBtn);
    
    await screen.findByTestId('contact-dialog');
    const saveBtn = screen.getByRole('button', { name: /Save/i });
    fireEvent.click(saveBtn);
    
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('save_contact', expect.objectContaining({
        password: undefined,
        contact: expect.objectContaining({ did: 'did:key:z789' })
      }));
    });
    
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('get_contacts');
    });
  });

  it('calls delete_contact via protectAction when deleting a contact', async () => {
    render(<AddressBook onBack={mockOnBack} />);
    
    // Wait for contacts to load
    await waitFor(() => screen.getByText(/John Doe/i));
    
    // Click delete button for John Doe
    const deleteBtns = screen.getAllByTitle(/Remove Contact/i);
    fireEvent.click(deleteBtns[0]);
    
    // Confirm in modal - Use more specific selector to avoid ambiguity
    const confirmBtn = screen.getByRole('button', { name: /^Delete Contact$/i });
    fireEvent.click(confirmBtn);
    
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('delete_contact', expect.objectContaining({
        did: 'did:key:z123',
        password: undefined
      }));
    });
  });
});
