// src/types.ts

/**
 * Represents information about a user profile.
 * Matches the Rust struct `ProfileInfo`.
 */
export interface ProfileInfo {
    profileName: string;
    folderName: string;
}

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
    non_redeemable_test_voucher: boolean;
    divisible?: boolean; // Add optional divisible property
}

/**
 * Represents an aggregated balance for a specific voucher standard.
 * Matches the Rust struct `AggregatedBalance`.
 */
export interface AggregatedBalance {
    standard_name: string;
    standard_uuid: string;
    unit: string;
    total_amount: string;
}

export interface VoucherStandardInfo {
    id: string;
    content: string;
    decimal_places?: number; // NEU: Optionale Angabe der erlaubten Dezimalstellen
}

// NEU: Detaillierte Adress-Struktur
export interface Address {
    street: string;
    house_number: string;
    zip_code: string;
    city: string;
    country: string;
    full_address: string; // Wird im Frontend konstruiert
}

// NEU: Detaillierte Ersteller-Struktur
export interface CreatorData {
    first_name: string;
    last_name: string;
    address: Address;
    organization?: string;
    community?: string;
    phone?: string;
    email?: string;
    url?: string;
    gender: string; // ISO 5218: 0=Not known, 1=Male, 2=Female, 9=Not applicable
    service_offer?: string;
    needs?: string;
    coordinates: string; // z.B. "Breitengrad, Längengrad"
}

// NEU: Struktur für Besicherungsdaten
export interface CollateralData {
    amount: string;
    unit: string;
    abbreviation: string;
}

// AKTUALISIERT: Die vollständige Struktur für neue Gutscheindaten
export interface NewVoucherData {
    validity_duration: string | null;
    non_redeemable_test_voucher: boolean;
    nominal_value: {
        amount: string;
        unit: string; // Wird vom Prototyp fest auf "Minuto" gesetzt
    };
    collateral: CollateralData;
    creator: CreatorData;
}

// NEU: Definiert einen Wert (Betrag und Einheit)
// Entspricht `ValueDefinition` in voucher.rs
export interface ValueDefinition {
    unit: string;
    amount: string;
    abbreviation?: string;
    description?: string;
}

// NEU: Definiert die (optionale) Besicherung
// Entspricht `Collateral` in voucher.rs
export interface Collateral {
    // ValueDefinition-Felder sind hier flach eingebettet
    unit: string;
    amount: string;
    abbreviation?: string;
    description?: string;
    // Das JSON-Feld heißt 'type'
    type?: string;
    redeem_condition?: string;
}

// NEU: Definiert das öffentliche Profil für Ersteller und Bürgen
// Entspricht `PublicProfile` in voucher_lib::models::profile
export interface PublicProfile {
    id?: string;
    first_name?: string;
    last_name?: string;
    address?: Address;
    organization?: string;
    community?: string;
    phone?: string;
    email?: string;
    url?: string;
    gender?: string;
    coordinates?: string;
    service_offer?: string; // Wieder hinzugefügt
    needs?: string; // Wieder hinzugefügt
}

// NEU: Definiert die Template-Daten aus dem Standard
// Entspricht `VoucherTemplateData` in voucher.rs
export interface VoucherTemplateData {
    description: string;
    primary_redemption_type: string;
    divisible: boolean;
    standard_minimum_issuance_validity: string;
    signature_requirements_description: string;
    footnote: string;
}

/**
 * Represents the full, detailed structure of a voucher.
 * Matches the Rust struct `VoucherDetails` (and by extension, `Voucher`).
 */
export interface VoucherDetails {
    voucher_standard: {
        name: string;
        uuid: string;
        standard_definition_hash: string;
    };
    voucher_id: string;
    voucher_nonce: string;
    description: string;
    primary_redemption_type: string;
    divisible: boolean;
    creation_date: string; // ISO 8601
    valid_until: string | null; // ISO 8601
    standard_minimum_issuance_validity: string | null;
    non_redeemable_test_voucher: boolean;
    nominal_value: {
        unit: string;
        amount: string;
    };
    creator: {
        id: string;
        first_name: string;
        last_name: string;
        signature: string;
    };
    guarantor_requirements_description: string;
    footnote: string;
    guarantor_signatures: GuarantorSignature[];
    needed_guarantors: number;
    transactions: Transaction[];
}

export interface GuarantorSignature {
    voucher_id: string;
    signature_id: string;
    guarantor_id: string;
    first_name: string;
    last_name: string;
    gender: string;
    signature: string;
    signature_time: string; // ISO 8601
}

export interface Transaction {
    t_id: string;
    t_type: 'init' | 'split' | 'transfer' | 'merge_credit';
    t_time: string; // ISO 8601
    sender_id: string;
    recipient_id: string;
    amount: string;
    sender_remaining_amount?: string;
    sender_signature: string;
    prev_hash: string;
}

// NEU: Typen für die Transfer-Erstellung

/**
 * Definiert eine einzelne Quelle für einen Transfer.
 * Entspricht `SourceTransfer` in voucher_lib.
 */
export interface SourceTransfer {
    local_instance_id: string;
    amount_to_send: string;
}

/**
 * Definiert die Anfrage für einen Transfer mit mehreren Quellen.
 * Entspricht `MultiTransferRequest` in voucher_lib.
 */
export interface MultiTransferRequest {
    recipient_id: string;
    sources: SourceTransfer[];
    notes?: string;
    sender_profile_name?: string;
}

/**
 * NEU: Detaillierte Info zu einem beteiligten Gutschein.
 * Entspricht `InvolvedVoucherInfo` in voucher_lib.
 */
export interface InvolvedVoucherInfo {
    local_instance_id: string;
    voucher_id: string;
    standard_name: string;
    unit: string;
    amount: string;
    is_divisible: boolean;
}


/**
 * Repräsentiert einen einzelnen Eintrag im Transaktionsverlauf ("Kontoauszug").
 * Entspricht der `TransactionRecord` Rust-Struktur.
 */
export interface TransactionRecord {
    id: string; // Eindeutige ID, z.B. UUID v4, im Frontend generiert
    direction: 'sent' | 'received';
    recipient_id: string;
    sender_id: string; // Eigene User-ID bei 'sent'
    timestamp: string; // ISO 8601 Zeitstempel
    summableAmounts: Record<string, string>; // z.B. { "Minuto": "50.00" }
    countableItems: Record<string, number>; // z.B. { "Brot": 3 }
    involved_vouchers: string[]; // local_instance_ids (wird von "received" verwendet)
    involvedSourcesDetails?: InvolvedVoucherInfo[]; // KORRIGIERT: (wird von "sent" verwendet)
    bundle_data: number[]; // Das `Vec<u8>` Transfer-Bundle
    bundle_id: string;
    notes?: string;
    sender_profile_name?: string;
}

/**
 * Repräsentiert die anwendungsspezifischen Einstellungen.
 * Entspricht der `AppSettings` Rust-Struktur.
 */
export interface AppSettings {
    bundle_retention_days: number;
}

/**
 * Represents the summary of a transfer, detailing summable and countable items.
 * Matches the Rust struct `TransferSummary`.
 */
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
    involvedVouchersDetails: InvolvedVoucherInfo[]; // <--- NEU
}