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
});
