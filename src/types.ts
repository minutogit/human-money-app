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
    status: string | object; // Can be a simple string or a complex object like { Incomplete: {} }
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