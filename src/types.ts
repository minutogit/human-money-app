// src/types.ts

/**
 * Represents a summary of a voucher, suitable for list displays.
 * Matches the Rust struct `VoucherSummary`.
 */
export interface VoucherSummary {
    local_id: string;
    instance_id: string;
    standard_definition_id: string;
    amount: string;
    currency: string;
    status: string; // e.g., "active", "partially_redeemed"
}
