import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { ReceiveSuccessView } from '../ReceiveSuccessView';
import { ReceiveSuccessPayload, L2QuorumInfo } from '../../types';

vi.mock('../../utils/log', () => ({ logger: { info: vi.fn(), error: vi.fn() } }));

const payload: ReceiveSuccessPayload = {
  senderId: '', transferSummary: { summableAmounts: { EUR: '10' }, countableItems: {} },
  involvedVouchers: ['received-voucher'],
};

describe('ReceiveSuccessView payment evidence', () => {
  it.each([undefined, { status: 'final', signerCount: 15, activeNodesCount: 20, shardId: 1 }])(
    'does not confirm payment from bundle import or a claimed quorum', (l2Quorum) => {
      render(<ReceiveSuccessView payload={{ ...payload, l2Quorum: l2Quorum as L2QuorumInfo | undefined }} onDone={vi.fn()} />);
      expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('unverified');
      expect(screen.queryByTestId('l2-quorum-count')).not.toBeInTheDocument();
      fireEvent.click(screen.getByTestId('l2-toggle-details'));
      expect(screen.getByText(/Keine Server-Verbindung|No server connection/)).toBeInTheDocument();
    },
  );

  it('renders 2-pillar security indicator', () => {
    render(<ReceiveSuccessView payload={payload} onDone={vi.fn()} />);
    expect(screen.getByTestId('two-pillar-security')).toBeInTheDocument();
    expect(screen.getByTestId('pillar-1-l1')).toHaveTextContent(/1\. Gutschein-Echtheit|1\. Voucher Authenticity/i);
    expect(screen.getByTestId('pillar-1-l1')).toHaveTextContent(/Echt & gültig|Authentic & valid/i);
    expect(screen.getByTestId('l2-quorum-badge')).toBeInTheDocument();
  });

  it('retains detected conflict warnings', () => {
    render(<ReceiveSuccessView payload={{ ...payload, verifiableConflicts: { conflict: [] } }} onDone={vi.fn()} />);
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('conflict');
    expect(screen.getByTestId('pillar-1-l1')).toHaveTextContent(/Format intakt|Format intact/i);
  });

  it('releases goods automatically on verified Confirmed checkout verdict (Level 2 green)', () => {
    render(
      <ReceiveSuccessView
        payload={{
          ...payload,
          checkoutVerdict: {
            outcome: 'Confirmed',
            canReleaseGoodsAutomatically: true,
            requiresMerchantApproval: false,
            isGoodsReleaseAuthorized: true,
          },
        }}
        onDone={vi.fn()}
      />,
    );
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('final');
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('●●');
    const goodsCard = screen.getByTestId('goods-release-card');
    expect(goodsCard).toHaveTextContent(/Ware sicher herausgeben|Safely hand over goods/i);
    expect(goodsCard).toHaveTextContent(/Stufe 2|Level 2/i);
    expect(screen.queryByTestId('merchant-approve-btn')).not.toBeInTheDocument();
  });

  it('releases goods automatically on Provisional checkout (Level 1 green) without requiring a warning click', () => {
    render(
      <ReceiveSuccessView
        payload={{
          ...payload,
          checkoutVerdict: {
            outcome: 'Provisional',
            canReleaseGoodsAutomatically: true,
            requiresMerchantApproval: false,
            isGoodsReleaseAuthorized: true,
            merchantWarning: 'Provisional quorum registered',
          },
        }}
        onDone={vi.fn()}
      />,
    );
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('provisional');
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('●○');
    const goodsCard = screen.getByTestId('goods-release-card');
    expect(goodsCard).toHaveTextContent(/Ware freigegeben|Goods released/i);
    expect(goodsCard).toHaveTextContent(/Stufe 1|Level 1/i);
    expect(screen.queryByTestId('merchant-approve-btn')).not.toBeInTheDocument();
  });

  it('requires cashier acceptance on trust basis for offline/unverified checkout', () => {
    render(
      <ReceiveSuccessView
        payload={{
          ...payload,
          checkoutVerdict: {
            outcome: 'Found',
            canReleaseGoodsAutomatically: false,
            requiresMerchantApproval: true,
            isGoodsReleaseAuthorized: false,
            merchantWarning: 'Keine Server-Verbindung vorhanden.',
          },
        }}
        onDone={vi.fn()}
      />,
    );
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('unverified');
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('○○');
    const goodsCard = screen.getByTestId('goods-release-card');
    expect(goodsCard).toHaveTextContent(/Vertrauensbasis|Trust Basis/i);
    const approveBtn = screen.getByTestId('merchant-approve-btn');
    expect(approveBtn).toBeInTheDocument();

    // Click explicit merchant approval button
    fireEvent.click(approveBtn);
    expect(screen.getByTestId('merchant-approved-badge')).toBeInTheDocument();
    expect(screen.getByTestId('merchant-approved-badge')).toHaveTextContent(/Auf Vertrauensbasis angenommen|Accepted on trust basis/i);
  });

  it('blocks goods release on Quarantined double-spend collision', () => {
    render(
      <ReceiveSuccessView
        payload={{
          ...payload,
          checkoutVerdict: {
            outcome: 'Quarantined:Double spend detected',
            canReleaseGoodsAutomatically: false,
            requiresMerchantApproval: false,
            isGoodsReleaseAuthorized: false,
            merchantWarning: 'Double-spend detected',
          },
        }}
        onDone={vi.fn()}
      />,
    );
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('conflict');
    expect(screen.getByTestId('goods-release-card')).toHaveTextContent(/WARE NICHT HERAUSGEBEN|DO NOT HAND OVER GOODS|Ware keinesfalls herausgeben/i);
    expect(screen.queryByTestId('merchant-approve-btn')).not.toBeInTheDocument();
  });
});
