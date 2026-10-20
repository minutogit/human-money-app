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
      expect(screen.getByText(/does not confirm payment or authorize releasing goods/)).toBeInTheDocument();
    },
  );

  it('retains detected conflict warnings', () => {
    render(<ReceiveSuccessView payload={{ ...payload, verifiableConflicts: { conflict: [] } }} onDone={vi.fn()} />);
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('conflict');
  });

  it('releases goods automatically only on verified Confirmed checkout verdict', () => {
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
    expect(screen.getByTestId('goods-release-card')).toHaveTextContent(/Goods Release Authorized|Warenfreigabe erteilt/i);
    expect(screen.getByText('14/20 BFT')).toBeInTheDocument();
  });

  it('requires explicit merchant approval for Provisional checkout and does not auto-release', () => {
    render(
      <ReceiveSuccessView
        payload={{
          ...payload,
          checkoutVerdict: {
            outcome: 'Provisional',
            canReleaseGoodsAutomatically: false,
            requiresMerchantApproval: true,
            isGoodsReleaseAuthorized: false,
            merchantWarning: 'Provisional quorum registered',
          },
        }}
        onDone={vi.fn()}
      />,
    );
    expect(screen.getByTestId('l2-status-pill')).toHaveTextContent('provisional');
    expect(screen.getByTestId('goods-release-card')).toHaveTextContent(/Manual Merchant Approval Required|Manuelle Händlerfreigabe/i);
    const approveBtn = screen.getByTestId('merchant-approve-btn');
    expect(approveBtn).toBeInTheDocument();

    // Click explicit merchant approval button
    fireEvent.click(approveBtn);
    expect(screen.getByTestId('merchant-approved-badge')).toBeInTheDocument();
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
    expect(screen.getByTestId('goods-release-card')).toHaveTextContent(/Goods Release Blocked|Warenfreigabe blockiert/i);
    expect(screen.queryByTestId('merchant-approve-btn')).not.toBeInTheDocument();
  });
});
