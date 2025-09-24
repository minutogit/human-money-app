// src/types.ts

// The status of a voucher can be represented as an object with a single key,
// e.g., { "Active": null } or { "Quarantined": "Reason why" }.
export type VoucherStatus = { [key: string]: string | null };

/**
 * Represents a summary of a voucher, suitable for list displays.
 * Matches the Rust struct `VoucherSummary`.
 */
export interface VoucherSummary {
    local_instance_id: string;
    status: VoucherStatus;
    valid_until: string; // ISO 8601 format
    creator_id: string;
    description: string;
    current_amount: string;
    unit: string;
    voucher_standard_name: string;
    voucher_standard_uuid: string;
    transaction_count: number;
    guarantor_signatures_count: number;
    additional_signatures_count: number;
    has_collateral: boolean;
    creator_first_name: string;
    creator_last_name: string;
    creator_coordinates: string;
}

export interface VoucherStandardInfo {
    id: string;
    content: string;
}

export interface CreatorData {
    first_name: string;
    last_name: string;
}

export interface NewVoucherData {
    nominal_value: { amount: string; unit: string };
    creator: CreatorData;
    validity_duration: string | null;
}