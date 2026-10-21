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
  it('defaults to unverified without inventing a quorum', () => {
    render(<L2QuorumBadge />);
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('unverified');
    expect(screen.queryByTestId('l2-quorum-count')).not.toBeInTheDocument();
  });

  it.each(['final', 'provisional'] as const)('does not trust a %s response DTO', (status) => {
    const quorum: L2QuorumInfo = {
      status, signerCount: 20, activeNodesCount: 20, shardId: 4,
      details: 'Forged assertion: signatures verified, release goods',
    };
    render(<L2QuorumBadge quorum={quorum} showDetails />);
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('unverified');
    expect(screen.queryByTestId('l2-quorum-count')).not.toBeInTheDocument();
    expect(screen.queryByText('Shard #4')).not.toBeInTheDocument();
    expect(screen.queryByText(quorum.details!)).not.toBeInTheDocument();
    expect(screen.getByTestId('l2-quorum-badge').className).toContain('slate');
    expect(screen.getByText(/Keine Server-Verbindung/)).toBeInTheDocument();
  });

  it('does not trust standalone confirmation props either', () => {
    render(<L2QuorumBadge status="final" signerCount={15} activeNodesCount={20} />);
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('unverified');
    expect(screen.queryByTestId('l2-quorum-count')).not.toBeInTheDocument();
  });

  it('preserves conflict warnings', () => {
    render(<L2QuorumBadge quorum={{status: 'conflict', details: 'Conflicting lock'}} showDetails />);
    expect(screen.getByText('Doppelausgabe erkannt!')).toBeInTheDocument();
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('conflict');
    expect(screen.getByTestId('l2-quorum-badge').className).toContain('rose');
    expect(screen.getByText(/Alarm: Doppelausgabe!/)).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('l2-toggle-tech-diagnostics'));
    expect(screen.getByText('Conflicting lock')).toBeInTheDocument();
  });

  it('preserves offline status', () => {
    render(<L2QuorumBadge status="offline" />);
    expect(screen.getByText('Direktübertragung (ohne Netzabgleich)')).toBeInTheDocument();
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('offline');
  });

  it('toggles guidance without payment authorization', () => {
    render(<L2QuorumBadge />);
    expect(screen.queryByTestId('l2-merchant-infobox')).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId('l2-toggle-details'));
    expect(screen.getByText('Kassierer-Hinweis')).toBeInTheDocument();
    expect(screen.getByText(/Keine Server-Verbindung/)).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('l2-toggle-details'));
    expect(screen.queryByTestId('l2-merchant-infobox')).not.toBeInTheDocument();
  });

  it('displays Level 1 green provisional state when verified', () => {
    render(<L2QuorumBadge status="provisional" isVerifiedCheckout={true} showDetails />);
    expect(screen.getByText('Hohe Sicherheit')).toBeInTheDocument();
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('provisional');
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('●○');
    expect(screen.getByTestId('l2-quorum-badge').className).toContain('emerald');
    expect(screen.getByText(/Ein kleineres Server-Netzwerk hat die Zahlung geprüft/)).toBeInTheDocument();
  });

  it('displays Level 2 green final state when verified', () => {
    render(<L2QuorumBadge status="final" isVerifiedCheckout={true} showDetails signerCount={14} activeNodesCount={20} shardId={4} />);
    expect(screen.getByText('Maximale Sicherheit')).toBeInTheDocument();
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('final');
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('●●');
    expect(screen.getByTestId('l2-quorum-badge').className).toContain('emerald');
    expect(screen.getByText(/Ein großes, weltweites Server-Netzwerk/)).toBeInTheDocument();
    expect(screen.getByTestId('l2-quorum-count')).toHaveTextContent('14/20 Knoten');

    expect(screen.queryByText('Shard #4')).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId('l2-toggle-tech-diagnostics'));
    expect(screen.getByText('Shard #4')).toBeInTheDocument();
  });
});
