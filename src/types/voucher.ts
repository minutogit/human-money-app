// src/types/voucher.ts

import { PublicProfile, CreatorData } from './profile';

export type VoucherStatus = 
    | "active" 
    | "archived" 
    | "incomplete"
    | "quarantined"
    | "endorsed"
    | "expired"
    | { [key: string]: unknown };

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
    signaturesCount: number;
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
    displayName: string;
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

export interface VoucherDetails {
    localInstanceId: string;
    status: VoucherStatus;
    voucher: Voucher;
    displayCurrency: string;
    displayStandardName: string;
    isTestVoucher: boolean;
}

export interface SignatureImpact {
    isAllowedRole: boolean;
    fatalConflicts: string[];
    resolvedRules: string[];
    gentleHints: string[];
}
