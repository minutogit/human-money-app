// src/types.ts

// --- Mnemonic Language Support ---
export type MnemonicLanguage =
    | "english"
    | "german"
    | "spanish"
    | "french"
    | "italian"
    | "japanese"
    | "korean"
    | "portuguese"
    | "czech"
    | "chineseSimplified"
    | "chineseTraditional";

// --- Basis-Strukturen ---

export interface ProfileInfo {
    profileName: string;
    folderName: string;
    lastUsed?: string; // ISO 8601 timestamp
}

export interface Address {
    street?: string;
    houseNumber?: string;
    zipCode?: string;
    city?: string;
    country?: string;
    fullAddress?: string;
}

export interface PublicProfile {
    id: string;
    firstName?: string;
    lastName?: string;
    organization?: string;
    community?: string;
    address?: Address;
    gender?: string;
    email?: string;
    phone?: string;
    coordinates?: string;
    url?: string;
    serviceOffer?: string;
    needs?: string;
    pictureUrl?: string;
}

export interface Contact {
    did: string;
    profile: PublicProfile;
    tags: string[];
    addedAt: string;
    notes?: string;
}

// Struktur für das Erstellen neuer Gutscheine (CreateVoucher.tsx)
export interface CreatorData {
    firstName: string;
    lastName: string;
    address: Address;
    organization?: string;
    community?: string;
    phone?: string;
    email?: string;
    url?: string;
    gender: string;
    serviceOffer?: string;
    needs?: string;
    coordinates: string;
    pictureUrl?: string;
}

export interface AggregatedBalance {
    standardName: string;
    standardUuid: string;
    unit: string;
    totalAmount: string;
    displayCurrency: string;
    displayStandardName: string;
    isTestVoucher: boolean;
}

// --- Voucher Strukturen ---

export type VoucherStatus = 
    | "active" 
    | "archived" 
    | "incomplete"
    | "quarantined"
    | "endorsed"
    | "expired"
    | { [key: string]: any };

export interface ValidationFailureReason {
    BusinessRule?: { message: string };
    AdditionalSignatureCountLow?: { required: number; current: number };
    RequiredSignatureMissing?: { roleDescription: string };
}

export interface VoucherSummary {
    localInstanceId: string;
    status: VoucherStatus;
    validUntil: string;
    creatorId: string;
    description: string;
    currentAmount: string;
    unit: string;
    rawStandardName: string;
    voucherStandardUuid: string;
    transactionCount: number;
    signaturesCount: number; // In core it was changed to signaturesCount (from guarantor_signaturesCount + additional_signaturesCount)
    hasCollateral: boolean;
    creatorFirstName?: string;
    creatorLastName?: string;
    creatorCoordinates?: string;
    isTestVoucher: boolean;
    displayCurrency: string;
    displayStandardName: string;
    // Client-side enrichment
    divisible?: boolean;
}

export interface VoucherStandardInfo {
    id: string;
    content: string;
}

export interface VoucherTemplateData {
    description: string;
    primaryRedemptionType?: string;
    divisible: boolean;
    standardMinimumIssuanceValidity?: string;
    signatureRequirementsDescription?: string;
    footnote?: string;
}

export interface VoucherStandardData {
    name: string;
    uuid: string;
    standardDefinitionHash: string;
    template: VoucherTemplateData;
}

export interface DynamicRule {
    expression: string;
    message: string;
}

export interface SignatureRule {
    roleDescription: string;
    weight: number;
    validationRules: Record<string, DynamicRule>;
}

export interface ImmutableIdentity {
    uuid: string;
    name: string;
    abbreviation: string;
}

export type PrimaryRedemptionType = "goodsOrServices" | "time" | "physicalAsset";
export type CollateralType = "personalGuarantee" | "fiatBacked" | "cryptoBacked" | "physicalAsset";
export type PrivacyMode = "public" | "stealth" | "flexible";

export interface ImmutableBlueprint {
    unit: string;
    primaryRedemptionType: PrimaryRedemptionType;
    collateralType: CollateralType;
}

export interface ImmutableFeatures {
    allowPartialTransfers: boolean;
    balancesAreSummable: boolean;
    amountDecimalPlaces: number;
    privacyMode: PrivacyMode;
    allowedTTypes: string[];
}

export interface ImmutableIssuance {
    validityDurationRange: string[];
    issuanceMinimumValidityDuration: string;
    additionalSignaturesRange: number[];
    allowedSignatureRoles: string[];
}

export interface ImmutableZone {
    identity: ImmutableIdentity;
    blueprint: ImmutableBlueprint;
    features: ImmutableFeatures;
    issuance: ImmutableIssuance;
    customRules: Record<string, DynamicRule>;
    signatureRules: Record<string, SignatureRule>;
}

export interface MutableMetadata {
    issuerName: string;
    homepageUrl?: string;
    documentationUrl?: string;
    keywords: string[];
}

export interface MutableAppConfig {
    defaultValidityDuration?: string;
    roundUpValidityTo?: string;
    serverHistoryRetention?: string;
}

export interface MutableI18n {
    descriptions: Record<string, string>;
    footnotes: Record<string, string>;
    collateralDescriptions: Record<string, string>;
}

export interface MutableZone {
    metadata: MutableMetadata;
    appConfig: MutableAppConfig;
    i18n: MutableI18n;
}

export interface SignatureBlock {
    issuerId: string;
    signature: string;
}

export interface VoucherStandardDefinition {
    immutable: ImmutableZone;
    mutable: MutableZone;
    signature?: SignatureBlock;
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
    abbreviation?: string;
    type?: string;
    redeemCondition?: string;
}

export interface AssetClassSummary {
    standardUuid: string;
    isTestVoucher: boolean;
    displayStandardName: string;
    displayCurrency: string;
}

export interface VoucherSignature {
    voucherId: string;
    signatureId: string;
    signerId: string;
    signature: string;
    signatureTime: string;
    role: string;
    details?: PublicProfile;
}

export interface NewVoucherData {
    validityDuration: string | null;
    nonRedeemableTestVoucher: boolean;
    nominalValue: {
        amount: string;
        unit: string;
    };
    collateral: CollateralData;
    creator: CreatorData;
}


export interface TransactionHistoryEntry {
    tId: string;
    tType: string;
    tTime: string;
    senderId: string;
    recipientId: string;
    amount: string;
}

export interface VoucherDetails {
    localInstanceId: string;
    status: VoucherStatus;
    voucher: Voucher;
    displayCurrency: string;
    displayStandardName: string;
    isTestVoucher: boolean;
}

export interface Voucher {
    voucherStandard: VoucherStandardData;
    voucherId: string;
    voucherNonce: string;
    creationDate: string;
    validUntil?: string;
    nonRedeemableTestVoucher: boolean;
    nominalValue: NominalValueData;
    collateral?: CollateralData;
    creator: PublicProfile;
    signatures: VoucherSignature[];
    transactions: TransactionHistoryEntry[];
}

// --- Transaktion & Transfer Strukturen ---

export interface InvolvedVoucherInfo {
    localInstanceId: string;
    voucherId: string;
    standardName: string;
    amount: string;
    unit: string;
    isTestVoucher: boolean;
    displayCurrency: string;
    displayStandardName: string;
}

export interface SourceTransfer {
    localInstanceId: string;
    amountToSend: string;
}

export interface MultiTransferRequest {
    recipientId: string;
    sources: SourceTransfer[];
    notes?: string | null;
    senderProfileName?: string | null;
    standardDefinitionsToml: Record<string, string>;
    usePrivacyMode?: boolean | null;
    password?: string | null;
}

// --- Tauri Command Argument Interfaces ---

export interface ReceiveBundleArgs {
    bundleData: number[];
    standardDefinitionsToml: Record<string, string>;
    password?: string | null;
    forceAcceptToleranceBundle: boolean;
}

export interface OpenVoucherSigningRequestArgs {
    containerBytes: number[];
    password?: string | null;
}

export interface ProcessAndAttachSignatureArgs {
    containerBytes: number[];
    standardTomlContent: string;
    containerPassword?: string | null;
    walletPassword?: string | null;
}

export interface TransactionRecord {
    id: string;
    direction: 'sent' | 'received';
    recipientId?: string;
    senderId?: string;
    timestamp: string;
    summableAmounts: Record<string, string>;
    countableItems: Record<string, number>;
    involvedVouchers: string[];

    // Optionale Felder für detaillierte Records
    bundleData?: number[];
    bundleId?: string;
    notes?: string;
    senderProfileName?: string;
    involvedSourcesDetails?: InvolvedVoucherInfo[];
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
    isSignatureAttached?: boolean;
    voucherId?: string;
    verifiableConflicts?: Record<string, TransactionFingerprint[]>;
}

export interface TransactionFingerprint {
    dsTag: string;
    u: string;
    blindedId: string;
    tId: string;
    encryptedTimestamp: string; // u128 usually serializes to string
    layer2Signature: string;
    deletableAt: string;
}

export type ConflictRole = "victim" | "witness";
export type TrustStatus = "clean" | { knownOffender: string } | { resolved: [string, boolean] };

export interface ProofOfDoubleSpendSummary {
    proofId: string;
    offenderId: string;
    forkPointPrevHash: string;
    reportTimestamp: string;
    isResolved: boolean;
    hasL2Verdict: boolean;
    localOverride: boolean;
    conflictRole: ConflictRole;
    affectedVoucherName?: string;
    voucherStandardUuid?: string;
    localNote?: string;
    isTestVoucher: boolean;
}

export interface ProofOfDoubleSpend {
    proofId: string;
    offenderId: string;
    forkPointPrevHash: string;
    conflictingTransactions: TransactionHistoryEntry[];
    deletableAt: string;
    reporterId: string;
    reportTimestamp: string;
    reporterSignature: string;
    resolutions?: ResolutionEndorsement[];
    layer2Verdict?: any;
    affectedVoucherName?: string;
    voucherStandardUuid?: string;
    isResolved?: boolean;
    nonRedeemableTestVoucher: boolean;
}

export interface ResolutionEndorsement {
    endorsementId: string;
    proofId: string;
    victimId: string;
    resolutionTimestamp: string;
    notes?: string;
    victimSignature: string;
}

export type PrivacyDefault = 'ask' | 'stealth' | 'public';

export type SealSyncStatus = 'Synced' | 'PendingUpload';

export interface SealUploadData {
    sealBytes: number[];
    sealHash: string;
}

export interface AppSettings {
    bundleRetentionDays: number;
    sessionTimeoutSeconds: number;
    lastUsedDirectory?: string;
    privacyDefault: PrivacyDefault;
}

export interface SignatureImpact {
    isAllowedRole: boolean;
    fatalConflicts: string[];
    resolvedRules: string[];
    gentleHints: string[];
}
export interface FullProofDetails {
    proof: ProofOfDoubleSpend;
    localOverride: boolean;
    conflictRole: ConflictRole;
    localNote?: string;
}

export type IntegrityReport =
    | { type: "valid" }
    | { type: "missingItems", items: string[] }
    | { type: "manipulatedItems", items: string[] }
    | { type: "unknownItems", items: string[] }
    | { type: "integrityOutdated" }
    | { type: "invalidSignature" }
    | { type: "missingIntegrityRecord" };

// --- Event Log / Activity History ---

export type WalletEventType =
    | "voucherCreated"
    | "transferSent"
    | "transferReceived"
    | "voucherQuarantined"
    | "voucherActivated"
    | "voucherVoided"
    | "voucherExpired"
    | { unknown: string };

export interface EventBffData {
    displayCurrency: string;
    amount: string;
    isTestVoucher: boolean;
    counterpartyId?: string;
    counterpartyName?: string;
}

export interface WalletEvent {
    eventId: string;
    localInstanceId: string;
    voucherId: string;
    timestamp: string; // ISO 8601
    eventType: WalletEventType;
    bffData: EventBffData;
}
