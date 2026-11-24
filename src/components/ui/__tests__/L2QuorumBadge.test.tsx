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
    const { rerender } = render(
      <L2QuorumBadge
        status="final"
        signerCount={15}
        activeNodesCount={20}
        shardId={7}
        details="Forged standalone detail"
        showDetails
      />
    );
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('unverified');
    expect(screen.queryByTestId('l2-quorum-count')).toBeNull();
    expect(screen.queryByText('Shard #7')).toBeNull();
    expect(screen.queryByText('Forged standalone detail')).toBeNull();
    expect(screen.queryByTestId('l2-toggle-tech-diagnostics')).toBeNull();

    rerender(
      <L2QuorumBadge
        status="provisional"
        signerCount={10}
        activeNodesCount={20}
        shardId={3}
        details="Forged provisional detail"
        showDetails
      />
    );
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('unverified');
    expect(screen.queryByTestId('l2-quorum-count')).toBeNull();
    expect(screen.queryByText('Shard #3')).toBeNull();
    expect(screen.queryByText('Forged provisional detail')).toBeNull();
    expect(screen.queryByTestId('l2-toggle-tech-diagnostics')).toBeNull();
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

  it('renders reduced guarantor protection warning badge only when enabled', () => {
    const { rerender } = render(
      <L2QuorumBadge status="final" isVerifiedCheckout={true} />
    );
    expect(screen.queryByTestId('reduced-guarantor-protection-badge')).not.toBeInTheDocument();

    rerender(
      <L2QuorumBadge
        status="final"
        isVerifiedCheckout={true}
        reducedGuarantorProtection={true}
      />
    );
    expect(screen.getByTestId('reduced-guarantor-protection-badge')).toHaveTextContent(
      'voucher.reducedGuarantorProtectionBadge'
    );
  });

  it('prioritizes quorum.status over conflicting standalone status prop', () => {
    render(
      <L2QuorumBadge
        quorum={{ status: 'conflict', details: 'Double spend' }}
        status="final"
        isVerifiedCheckout={true}
        showDetails
      />
    );
    expect(screen.getByText('Doppelausgabe erkannt!')).toBeInTheDocument();
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('conflict');
    expect(screen.getByTestId('l2-quorum-badge').className).toContain('rose');
    expect(screen.getByTestId('l2-quorum-badge').className).not.toContain('emerald');
  });

  it('preserves yellow warning quorum counts, shard, and technical details without isVerifiedCheckout', () => {
    render(
      <L2QuorumBadge
        status="yellow"
        signerCount={1}
        activeNodesCount={3}
        shardId={2}
        details="1/3 trusted"
        showDetails
      />
    );
    expect(screen.getByText('Eingeschränkt (Gelb)')).toBeInTheDocument();
    expect(screen.getByTestId('l2-quorum-count')).toHaveTextContent('1/3 Server');

    fireEvent.click(screen.getByTestId('l2-toggle-tech-diagnostics'));
    expect(screen.getByTestId('l2-tech-details')).toHaveTextContent('1/3 (33%)');
    expect(screen.getByText('Shard #2')).toBeInTheDocument();
    expect(screen.getByText('1/3 trusted')).toBeInTheDocument();
  });

  it('avoids division by zero when activeNodesCount is 0 and clamps progress bar width to 100%', () => {
    const { rerender } = render(
      <L2QuorumBadge status="yellow" signerCount={0} activeNodesCount={0} showDetails />
    );
    fireEvent.click(screen.getByTestId('l2-toggle-tech-diagnostics'));
    const techZero = screen.getByTestId('l2-tech-details');
    expect(techZero.textContent).not.toContain('NaN');
    expect(techZero.textContent).not.toContain('Infinity');
    expect(screen.queryByText('Shard Confirmation')).toBeNull();

    rerender(
      <L2QuorumBadge
        status="final"
        isVerifiedCheckout={true}
        signerCount={5}
        activeNodesCount={4}
        showDetails
      />
    );
    const techOver = screen.getByTestId('l2-tech-details');
    expect(techOver).toHaveTextContent('5/4 (125%)');
    const bar = techOver.querySelector('.h-full.rounded-full');
    expect(bar).not.toBeNull();
    expect(bar).toHaveStyle({ width: '100%' });
  });

  it('renders conflict progress bar with rose styling rather than emerald', () => {
    render(
      <L2QuorumBadge
        status="conflict"
        signerCount={2}
        activeNodesCount={4}
        showDetails
      />
    );
    fireEvent.click(screen.getByTestId('l2-toggle-tech-diagnostics'));
    const bar = screen.getByTestId('l2-tech-details').querySelector('.h-full.rounded-full');
    expect(bar).not.toBeNull();
    expect(bar!.className).toContain('bg-rose-500');
    expect(bar!.className).not.toContain('bg-emerald-500');
  });

  it('renders optional pillarLabel and updates aria-expanded on details toggle', () => {
    render(<L2QuorumBadge pillarLabel="Layer 2 Lock" />);
    expect(screen.getByText('Layer 2 Lock')).toBeInTheDocument();

    const toggle = screen.getByTestId('l2-toggle-details');
    expect(toggle).toHaveAttribute('aria-expanded', 'false');

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByTestId('l2-merchant-infobox')).toBeInTheDocument();

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByTestId('l2-merchant-infobox')).toBeNull();
  });
});
