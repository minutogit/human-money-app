// src/components/__tests__/TransactionHistoryView.test.tsx
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { TransactionHistoryView } from '../TransactionHistoryView';
import { transferService } from '../../services/transferService';
import { contactService } from '../../services/contactService';
import { TransactionRecord, Contact } from '../../types';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';

vi.mock('../../services/transferService');
vi.mock('../../services/contactService');
vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({
    writeText: vi.fn(),
    readText: vi.fn(),
}));
vi.mock('@tauri-apps/plugin-dialog', () => ({
    save: vi.fn(),
}));
vi.mock('@tauri-apps/plugin-fs', () => ({
    writeFile: vi.fn(),
}));

const mockNavigate = vi.fn();
const mockGoBack = vi.fn();

vi.mock('../../context/NavigationContext', () => ({
    useNavigation: () => ({
        navigate: mockNavigate,
        goBack: mockGoBack,
        appState: { view: 'transaction_history' },
    }),
}));

vi.mock('react-i18next', () => ({
    useTranslation: () => ({
        t: (key: string, options?: Record<string, unknown>) => {
            if (options?.count !== undefined) return `${key}:${options.count}`;
            if (options?.error !== undefined) return `${key}:${options.error}`;
            return key;
        },
        i18n: { language: 'de' },
    }),
}));

const mockRecords: TransactionRecord[] = [
    {
        id: 'tx-1-rec',
        direction: 'received',
        senderId: 'did:key:z6MkhaXG',
        senderProfileName: 'Alice Wonder',
        timestamp: '2026-06-01T12:00:00.000Z',
        summableAmounts: { 'Minuto': '100.00' },
        countableItems: {},
        involvedVouchers: ['v-1'],
        bundleId: 'bundle-alice-1111',
        notes: 'Honig und Apfelsaft',
        bundleData: [1, 2, 3]
    },
    {
        id: 'tx-2-sent',
        direction: 'sent',
        recipientId: 'did:key:z6MkpTHR',
        timestamp: '2026-06-02T15:30:00.000Z',
        summableAmounts: { 'Minuto': '30.00' },
        countableItems: {},
        involvedVouchers: ['v-2'],
        bundleId: 'bundle-bob-2222',
        notes: 'Werkstattmiete Juni',
        bundleData: [4, 5, 6]
    },
    {
        id: 'tx-3-sent-freetaler',
        direction: 'sent',
        recipientId: 'did:key:z6Mkunknown',
        timestamp: '2025-01-01T10:00:00.000Z',
        summableAmounts: { 'FreeTaler': '15.00' },
        countableItems: {},
        involvedVouchers: ['v-3'],
        bundleId: 'bundle-charlie-3333',
        notes: 'Alte Zahlung aus 2025'
    }
];

const mockContacts: Contact[] = [
    {
        did: 'did:key:z6MkpTHR',
        profile: {
            id: 'did:key:z6MkpTHR',
            firstName: 'Bob',
            lastName: 'Baumeister'
        },
        tags: [],
        addedAt: '2026-01-01T00:00:00.000Z',
        trustLevel: 2,
        desiredHops: 3
    }
];

describe('TransactionHistoryView', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(transferService.getHistory).mockResolvedValue([...mockRecords]);
        vi.mocked(contactService.getContacts).mockResolvedValue([...mockContacts]);
        vi.mocked(contactService.saveContact).mockResolvedValue();
    });

    it('renders transaction records with resolved counterparty names and amounts', async () => {
        render(<TransactionHistoryView />);

        await waitFor(() => {
            // Bob Baumeister resolved via contactService
            expect(screen.getByText('Bob Baumeister')).toBeInTheDocument();
        });

        // Alice Wonder resolved via senderProfileName
        expect(screen.getByText('Alice Wonder')).toBeInTheDocument();

        // Notes visible
        expect(screen.getByText('"Honig und Apfelsaft"')).toBeInTheDocument();
        expect(screen.getByText('"Werkstattmiete Juni"')).toBeInTheDocument();

        // Short receipt numbers visible
        expect(screen.getByText('#bundle-a')).toBeInTheDocument();
        expect(screen.getByText('#bundle-b')).toBeInTheDocument();
    });

    it('toggles transaction details open and closed on row click', async () => {
        render(<TransactionHistoryView />);

        await waitFor(() => {
            expect(screen.getByText('Bob Baumeister')).toBeInTheDocument();
        });

        // Initially expanded details not shown
        expect(screen.queryByText('bundle-alice-1111')).not.toBeInTheDocument();

        // Click on Alice's row
        const aliceRow = screen.getByText('Alice Wonder').closest('.p-4');
        expect(aliceRow).toBeDefined();
        fireEvent.click(aliceRow!);

        // Expanded details now visible
        await waitFor(() => {
            expect(screen.getByText('bundle-alice-1111')).toBeInTheDocument();
            expect(screen.getByText('did:key:z6MkhaXG')).toBeInTheDocument();
        });

        // Click on the same row again -> closes it
        fireEvent.click(aliceRow!);
        await waitFor(() => {
            expect(screen.queryByText('bundle-alice-1111')).not.toBeInTheDocument();
        });
    });

    it('copies transaction ID and DID to clipboard when clicking copy buttons', async () => {
        render(<TransactionHistoryView />);

        await waitFor(() => {
            expect(screen.getByText('Alice Wonder')).toBeInTheDocument();
        });

        // Open Alice's row
        const aliceRow = screen.getByText('Alice Wonder').closest('.p-4');
        fireEvent.click(aliceRow!);

        await waitFor(() => {
            expect(screen.getByText('history.copyTxId')).toBeInTheDocument();
        });

        // Click copy txId
        fireEvent.click(screen.getByText('history.copyTxId'));
        expect(writeText).toHaveBeenCalledWith('bundle-alice-1111');

        // Click copy DID
        fireEvent.click(screen.getByText('history.copyDid'));
        expect(writeText).toHaveBeenCalledWith('did:key:z6MkhaXG');
    });

    it('filters transactions by direction (all, inbound, outbound)', async () => {
        render(<TransactionHistoryView />);

        await waitFor(() => {
            expect(screen.getByText('Alice Wonder')).toBeInTheDocument();
            expect(screen.getByText('Bob Baumeister')).toBeInTheDocument();
        });

        // Filter only received (inbound)
        const inboundBtn = screen.getByText('history.filterInbound');
        fireEvent.click(inboundBtn);

        expect(screen.getByText('Alice Wonder')).toBeInTheDocument();
        expect(screen.queryByText('Bob Baumeister')).not.toBeInTheDocument();

        // Filter only sent (outbound)
        const outboundBtn = screen.getByText('history.filterOutbound');
        fireEvent.click(outboundBtn);

        expect(screen.queryByText('Alice Wonder')).not.toBeInTheDocument();
        expect(screen.getByText('Bob Baumeister')).toBeInTheDocument();

        // Reset to all
        const allBtn = screen.getByText('history.filterAllDirections');
        fireEvent.click(allBtn);

        expect(screen.getByText('Alice Wonder')).toBeInTheDocument();
        expect(screen.getByText('Bob Baumeister')).toBeInTheDocument();
    });

    it('filters transactions by search query matching contact name or notes', async () => {
        render(<TransactionHistoryView />);

        await waitFor(() => {
            expect(screen.getByText('Alice Wonder')).toBeInTheDocument();
        });

        const searchInput = screen.getByPlaceholderText('history.searchPlaceholder');
        
        // Search by resolved contact name
        fireEvent.change(searchInput, { target: { value: 'Baumeister' } });
        expect(screen.getByText('Bob Baumeister')).toBeInTheDocument();
        expect(screen.queryByText('Alice Wonder')).not.toBeInTheDocument();

        // Search by notes
        fireEvent.change(searchInput, { target: { value: 'Apfelsaft' } });
        expect(screen.getByText('Alice Wonder')).toBeInTheDocument();
        expect(screen.queryByText('Bob Baumeister')).not.toBeInTheDocument();

        // Search by bundle ID
        fireEvent.change(searchInput, { target: { value: 'bundle-bob' } });
        expect(screen.getByText('Bob Baumeister')).toBeInTheDocument();
        expect(screen.queryByText('Alice Wonder')).not.toBeInTheDocument();
    });

    it('shows "+ Als Kontakt speichern" only for unknown counterparties and opens dialog', async () => {
        render(<TransactionHistoryView />);

        await waitFor(() => {
            expect(screen.getByText('Alice Wonder')).toBeInTheDocument();
        });

        // 1. Open Bob's row (Bob is already in mockContacts)
        const bobRow = screen.getByText('Bob Baumeister').closest('.p-4');
        fireEvent.click(bobRow!);

        await waitFor(() => {
            expect(screen.getByText('bundle-bob-2222')).toBeInTheDocument();
        });
        // Bob is known -> no save contact button
        expect(screen.queryByText('history.saveAsContact')).not.toBeInTheDocument();

        // Close Bob's row
        fireEvent.click(bobRow!);

        // 2. Open Alice's row (Alice's DID is NOT in mockContacts)
        const aliceRow = screen.getByText('Alice Wonder').closest('.p-4');
        fireEvent.click(aliceRow!);

        await waitFor(() => {
            expect(screen.getByText('history.saveAsContact')).toBeInTheDocument();
        });

        // Click save contact -> opens ContactDialog
        fireEvent.click(screen.getByText('history.saveAsContact'));

        await waitFor(() => {
            expect(screen.getByText('contacts.addNewContact')).toBeInTheDocument();
        });
    });

    it('navigates to activities view when clicking the event log button', async () => {
        render(<TransactionHistoryView />);

        await waitFor(() => {
            expect(screen.getByText('history.eventLogButton')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByText('history.eventLogButton'));
        expect(mockNavigate).toHaveBeenCalledWith({
            view: 'activities',
            previousView: { view: 'transaction_history' }
        });
    });

    it('triggers CSV export via save dialog and writes file', async () => {
        vi.mocked(save).mockResolvedValue('/path/to/kontoauszug.csv');
        vi.mocked(writeFile).mockResolvedValue();

        render(<TransactionHistoryView />);

        await waitFor(() => {
            expect(screen.getByText('history.exportCsv')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByText('history.exportCsv'));

        await waitFor(() => {
            expect(save).toHaveBeenCalledWith(expect.objectContaining({
                filters: [{ name: 'CSV (*.csv)', extensions: ['csv'] }]
            }));
            expect(writeFile).toHaveBeenCalled();
            expect(screen.getByText('history.exportCsvSuccess')).toBeInTheDocument();
        });
    });
});
