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
    expect(screen.getByText(/Lock-Server haben den Transfer vorläufig abgesichert/)).toBeInTheDocument();
  });

  it('displays Level 2 green final state when verified', () => {
    render(<L2QuorumBadge status="final" isVerifiedCheckout={true} showDetails signerCount={3} activeNodesCount={4} />);
    expect(screen.getByText('Quorum bestätigt')).toBeInTheDocument();
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('final');
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('●●');
    expect(screen.getByTestId('l2-quorum-badge').className).toContain('emerald');
    expect(screen.getByText(/Vertraute Lock-Server haben den Transfer nach der Grün-Regel abgesichert/)).toBeInTheDocument();
    expect(screen.getByTestId('l2-quorum-count')).toHaveTextContent(/3\/4 Server/);

    fireEvent.click(screen.getByTestId('l2-toggle-tech-diagnostics'));
    expect(screen.getByText('Sync-Lock Quorum')).toBeInTheDocument();
  });

  it('displays offline_only (N=0) and yellow warning badges cleanly', () => {
    const { rerender } = render(<L2QuorumBadge status="offline_only" showDetails />);
    expect(screen.getByText('Nur Offline (N = 0)')).toBeInTheDocument();
    expect(screen.getByTestId('l2-quorum-badge').className).toContain('slate');

    rerender(<L2QuorumBadge status="yellow" showDetails />);
    expect(screen.getByText('Eingeschränkt (Gelb)')).toBeInTheDocument();
    expect(screen.getByTestId('l2-quorum-badge').className).toContain('amber');
  });
});
