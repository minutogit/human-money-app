// src/types.ts

// --- Basis-Strukturen ---

export interface ProfileInfo {
    profile_name: string;
    folder_name: string;
}

export interface Address {
    street?: string;
    house_number?: string;
    zip_code?: string;
    city?: string;
    country?: string;
    full_address?: string;
}

export interface PublicProfile {
    id: string;
    first_name?: string;
    last_name?: string;
    organization?: string;
    community?: string;
    address?: Address;
    gender?: string;
    email?: string;
    phone?: string;
    coordinates?: string;
    url?: string;
    service_offer?: string;
    needs?: string;
}

// Struktur für das Erstellen neuer Gutscheine (CreateVoucher.tsx)
export interface CreatorData {
    first_name: string;
    last_name: string;
    address: Address;
    organization?: string;
    community?: string;
    phone?: string;
    email?: string;
    url?: string;
    gender: string;
    service_offer?: string;
    needs?: string;
    coordinates: string;
}

export interface AggregatedBalance {
    standard_name: string;
    standard_uuid: string;
    unit: string;
    total_amount: string;
}

// --- Voucher Strukturen ---

export type VoucherStatus = string | { [key: string]: any };

export interface VoucherSummary {
    local_instance_id: string;
    status: VoucherStatus;
    valid_until: string;
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
    creator_first_name?: string;
    creator_last_name?: string;
    creator_coordinates?: string;
    non_redeemable_test_voucher: boolean;
    // Client-side enrichment
    divisible?: boolean;
}

export interface VoucherStandardInfo {
    id: string;
    content: string;
}

export interface VoucherTemplateData {
    description: string;
    primary_redemption_type?: string;
    divisible: boolean;
    standard_minimum_issuance_validity?: string;
    signature_requirements_description?: string;
    footnote?: string;
}

export interface VoucherStandardData {
    name: string;
    uuid: string;
    standard_definition_hash: string;
    template: VoucherTemplateData;
}

export interface NominalValueData {
    unit: string;
    amount: string;
    abbreviation?: string;
    description?: string;
}

export interface CollateralData {
    unit: string;
    amount: string;
    abbreviation?: string; // Optional gemacht um Konflikte zu vermeiden
    type?: string;
    redeem_condition?: string;
}

export interface VoucherSignature {
    voucher_id: string;
    signature_id: string;
    signer_id: string;
    signature: string;
    signature_time: string;
    role: string;
    details?: PublicProfile;
}

export interface NewVoucherData {
    validity_duration: string | null;
    non_redeemable_test_voucher: boolean;
    nominal_value: {
        amount: string;
        unit: string;
    };
    collateral: CollateralData;
    creator: CreatorData;
}


export interface TransactionHistoryEntry {
    t_id: string;
    t_type: string;
    t_time: string;
    sender_id: string;
    recipient_id: string;
    amount: string;
}

export interface VoucherDetails {
    voucher_standard: VoucherStandardData;
    voucher_id: string;
    voucher_nonce: string;
    creation_date: string;
    valid_until?: string;
    non_redeemable_test_voucher: boolean;
    nominal_value: NominalValueData;
    collateral?: CollateralData;
    creator: PublicProfile;
    signatures: VoucherSignature[];
    transactions: TransactionHistoryEntry[];
}

// --- Transaktion & Transfer Strukturen ---

export interface InvolvedVoucherInfo {
    local_instance_id: string;
    voucher_id?: string;
    standard_name: string;
    amount: string;
    unit: string;
}

export interface SourceTransfer {
    local_instance_id: string;
    amount_to_send: string;
}

export interface TransactionRecord {
    id: string;
    direction: 'sent' | 'received';
    recipient_id?: string;
    sender_id?: string;
    timestamp: string;
    summableAmounts: Record<string, string>;
    countableItems: Record<string, number>;
    involved_vouchers: string[];

    // Optionale Felder für detaillierte Records
    bundle_data?: number[];
    bundle_id?: string;
    notes?: string;
    sender_profile_name?: string;
    involved_sources_details?: InvolvedVoucherInfo[];
}

export interface TransferSummary {
    summableAmounts: Record<string, string>;
    countableItems: Record<string, number>;
}

export interface ReceiveSuccessPayload {
    senderId: string;
    senderProfileName?: string;
    notes?: string;
    transferSummary: TransferSummary;
    involvedVouchers: string[];
    involvedVouchersDetails?: InvolvedVoucherInfo[];
}

export interface AppSettings {
    bundle_retention_days: number;
    session_timeout_seconds: number;
    last_used_directory?: string;
}