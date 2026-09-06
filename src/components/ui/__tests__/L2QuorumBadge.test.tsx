import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { L2QuorumBadge } from '../L2QuorumBadge';
import { L2QuorumInfo } from '../../../types';

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (_key: string, defaultValue?: string) => defaultValue || _key,
    i18n: { language: 'de' },
  }),
}));

describe('L2QuorumBadge', () => {
  it('renders correctly for final status (green / emerald)', () => {
    const quorum: L2QuorumInfo = {
      status: 'final',
      signerCount: 15,
      activeNodesCount: 20,
      shardId: 1,
    };

    render(<L2QuorumBadge quorum={quorum} />);

    expect(screen.getByText('Final gesichert')).toBeInTheDocument();
    expect(screen.getByText('Shard-Quorum erreicht')).toBeInTheDocument();
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('final');

    const badge = screen.getByTestId('l2-quorum-badge');
    expect(badge.className).toContain('emerald');
  });

  it('renders correctly for provisional status (yellow / amber)', () => {
    const quorum: L2QuorumInfo = {
      status: 'provisional',
      signerCount: 8,
      activeNodesCount: 20,
    };

    render(<L2QuorumBadge quorum={quorum} />);

    expect(screen.getByText('Vorläufig gesichert')).toBeInTheDocument();
    expect(screen.getByText('Vorläufiges Quorum registriert')).toBeInTheDocument();
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('provisional');

    const badge = screen.getByTestId('l2-quorum-badge');
    expect(badge.className).toContain('amber');
  });

  it('renders correctly for conflict status (red / rose double-spend warning)', () => {
    const quorum: L2QuorumInfo = {
      status: 'conflict',
      details: 'Conflicting lock detected on node 4',
    };

    render(<L2QuorumBadge quorum={quorum} />);

    expect(screen.getByText('Double-Spend Warnung')).toBeInTheDocument();
    expect(screen.getByText('Kollision im Layer 2 erkannt')).toBeInTheDocument();
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('conflict');

    const badge = screen.getByTestId('l2-quorum-badge');
    expect(badge.className).toContain('rose');
  });

  it('renders correctly for offline status (gray / slate)', () => {
    const quorum: L2QuorumInfo = {
      status: 'offline',
    };

    render(<L2QuorumBadge quorum={quorum} />);

    expect(screen.getByText('Offline-Prüfung')).toBeInTheDocument();
    expect(screen.getByText('Peer-to-Peer Nachweis')).toBeInTheDocument();
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('offline');

    const badge = screen.getByTestId('l2-quorum-badge');
    expect(badge.className).toContain('slate');
  });

  it('displays signerCount and activeNodesCount correctly', () => {
    render(
      <L2QuorumBadge 
        status="final" 
        signerCount={15} 
        activeNodesCount={20} 
      />
    );

    const countContainer = screen.getByTestId('l2-quorum-count');
    expect(countContainer).toHaveTextContent('15/20 Knoten');
  });

  it('displays single signerCount when activeNodesCount is omitted', () => {
    render(
      <L2QuorumBadge 
        status="provisional" 
        signerCount={3} 
      />
    );

    const countContainer = screen.getByTestId('l2-quorum-count');
    expect(countContainer).toHaveTextContent('3 Knoten');
  });

  it('toggles merchant infobox and displays cashier guidance', () => {
    const quorum: L2QuorumInfo = {
      status: 'final',
      signerCount: 16,
      activeNodesCount: 20,
      shardId: 4,
      details: 'All signatures verified via BLAKE3 domain tags',
    };

    render(<L2QuorumBadge quorum={quorum} />);

    // Initially collapsed
    expect(screen.queryByTestId('l2-merchant-infobox')).not.toBeInTheDocument();

    // Click toggle button
    const toggleBtn = screen.getByTestId('l2-toggle-details');
    fireEvent.click(toggleBtn);

    // Now expanded
    const infobox = screen.getByTestId('l2-merchant-infobox');
    expect(infobox).toBeInTheDocument();
    expect(screen.getByText('Kassierer-Hinweis')).toBeInTheDocument();
    expect(screen.getByText(/Shard-Bestätigung/)).toBeInTheDocument();
    expect(screen.getByText(/16\/20 \(80%\)/)).toBeInTheDocument();
    expect(screen.getByText('Shard #4')).toBeInTheDocument();
    expect(screen.getByText('All signatures verified via BLAKE3 domain tags')).toBeInTheDocument();

    // Collapse again
    fireEvent.click(toggleBtn);
    expect(screen.queryByTestId('l2-merchant-infobox')).not.toBeInTheDocument();
  });
});
