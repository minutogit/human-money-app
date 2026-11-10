// src/types/market.ts

import { FrontendUserProfile } from './profile';

export type FrontendListingType = 'offer' | 'request';
export type FrontendListingOrigin = 'voucher' | 'card';
export type FrontendTrustTier = 'trusted' | 'unknown' | 'hidden';

export interface FrontendMarketListing {
    listingId: string;
    listingType: FrontendListingType;
    title: string;
    text?: string | null;
    pricing?: string | null;
    origin: FrontendListingOrigin;
    allowInquiries: boolean;
}

export type FrontendEntrySource =
    | { kind: 'voucher' }
    | { kind: 'card'; hops: number }
    | { kind: 'sync'; readUrl: string };

export interface FrontendStandardRef {
    name: string;
    uuid: string;
}

export interface FrontendSyncDescriptor {
    writeUrl: string;
    readUrl: string;
    slotId?: string | null;
    dropPub: string;
    lastSeenAt?: number | null;
    lastErrorAt?: number | null;
    failCount?: number;
}

export interface FrontendMarketPerson {
    did: string;
    profile: FrontendUserProfile;
    listings: FrontendMarketListing[];
    sources: FrontendEntrySource[];
    asOf: number; // unix seconds
    standards: FrontendStandardRef[];
    trust: FrontendTrustTier;
    canForward: boolean;
    cardSyncs: FrontendSyncDescriptor[];
}

export interface FrontendCardListingInput {
    listingType: FrontendListingType;
    title: string;
    text?: string | null;
    pricing?: string | null;
    allowInquiries?: boolean;
}

export interface FrontendContactCardDto {
    did: string;
    v: number;
    validUntil: number;
    maxHops: number;
    profile: FrontendUserProfile;
    listings: FrontendCardListingInput[];
    syncs: FrontendSyncDescriptor[];
}

export interface FrontendOwnContactCard {
    card: FrontendContactCardDto;
    cardJws: string;
    qrSvg?: string | null;
    shareUri: string;
    envelopeJson: string;
    qrJws?: string | null;
    isCompactQr?: boolean;
}

export interface FrontendSaveOwnCardRequest {
    profile: FrontendUserProfile;
    listings: FrontendCardListingInput[];
    maxHops: number;
    validityDays?: number | null;
}

export interface FrontendForwardCardExport {
    did: string;
    envelopeJson: string;
    shareUri: string;
    qrSvg?: string | null;
}

export interface FrontendImportCardResult {
    status: string;
    did?: string | null;
    replaced: boolean;
}

export interface FrontendMemberPolicyDto {
    directMin: number;
    hop2Enabled: boolean;
    bridgeMin: number;
    hop2Min: number;
}

export interface FrontendSyncSetupDraft {
    descriptor: FrontendSyncDescriptor;
    dataDir: string;
    adminPubkey: string;
    generation: number;
    maxMembers: number;
    maxTotalStorageMb: number;
    policy: FrontendMemberPolicyDto;
    zipBase64: string;
}

export interface FrontendSyncAdminStatus {
    descriptor: FrontendSyncDescriptor;
    adminPubkey: string;
    generation: number;
    rotationRetireAt?: number | null;
    dataDir: string;
    maxMembers: number;
    maxTotalStorageMb: number;
    usedBytes?: number | null;
    maxBytes?: number | null;
    softWarning: boolean;
    cardPolicy: FrontendMemberPolicyDto;
    allow: string[];
    deny: string[];
    blockedPubkeys: string[];
    directMemberCount: number;
    hop2MemberCount: number;
    totalMemberCount: number;
    activeSlotIds: string[];
    channelCounts?: Record<string, number>;
}

export interface FrontendSyncAdminUpdateRequest {
    cardPolicy?: FrontendMemberPolicyDto | null;
    allow?: string[] | null;
    deny?: string[] | null;
    blockedPubkeys?: string[] | null;
    maxMembers?: number | null;
    maxTotalStorageMb?: number | null;
}

export interface FrontendPublicSyncInfo {
    writeUrl: string;
    readUrl: string;
    dropPub: string;
    registerUrl?: string | null;
    accessPubkey: string;
    authorizedChannels: string[];
}

export interface FrontendSyncOverviewStatus {
    connectedCount: number;
    pendingCount: number;
    friendSyncs: number;
    publicSyncs: number;
    hasOwnSync: boolean;
    ownSyncSoftWarning: boolean;
    lastSyncAt?: number | null;
}

export interface StoredPublicSync {
    writeUrl: string;
    readUrl: string;
    dropPub: string;
    registerUrl?: string | null;
    accessPubkey: string;
    enabled: boolean;
    authorizedChannels: string[];
}

export interface FrontendSyncConnectionTestResult {
    success: boolean;
    message: string;
    writeOk: boolean;
    readOk: boolean;
}

export interface FrontendDiscoveredSync {
    descriptor: FrontendSyncDescriptor;
    isAuthorized: boolean;
    isSelected: boolean;
    sourceLabel: string;
}

export interface FrontendMarketSyncResult {
    skippedWifiOnly: boolean;
    uploadedTo: number;
    polledSyncs: number;
    decryptedSlots: number;
    importedCards: number;
    importedWotLists: number;
    errors: string[];
    retryAfterSecs?: number | null;
}

export interface FrontendTrustChangeImpact {
    canReadMyMarketBefore: boolean;
    canReadMyMarketAfter: boolean;
    canReadMyWotBefore: boolean;
    canReadMyWotAfter: boolean;
    canWriteMySyncBefore: boolean;
    canWriteMySyncAfter: boolean;
    canSendMePostBefore: boolean;
    canSendMePostAfter: boolean;
}

export interface FrontendPostDepositResult {
    depositedTo: number;
    msgId: string;
    errors: string[];
}

export interface FrontendPendingSignRequest {
    msgId: string;
    senderDid: string;
    senderName?: string | null;
    createdAt: number;
    replySyncs: FrontendSyncDescriptor[];
    containerBytes: number[];
}

export interface FrontendPendingWotHandshake {
    msgId: string;
    senderDid: string;
    senderName?: string | null;
    proposedLevel: number;
    replyRequested: boolean;
    createdAt: number;
}

export interface FrontendMarketInquiryReplyDto {
    inquiryId: string;
    listingId: string;
    decision: 'confirm' | 'decline' | string;
    note?: string | null;
    paymentUri?: string | null;
    amountHint?: string | null;
    standardUuid?: string | null;
    repliedAt: number;
}

export interface FrontendMarketInquiryItem {
    inquiryId: string;
    listingId: string;
    listingTitle: string;
    counterpartyDid: string;
    counterpartyName?: string | null;
    message: string;
    createdAt: number;
    replySyncs: FrontendSyncDescriptor[];
    reply?: FrontendMarketInquiryReplyDto | null;
}

export interface FrontendPostInboxSummary {
    pendingSignRequests: FrontendPendingSignRequest[];
    pendingWotHandshakes: FrontendPendingWotHandshake[];
    sellerInquiries: FrontendMarketInquiryItem[];
    buyerInquiries: FrontendMarketInquiryItem[];
    unacknowledgedPayments: string[];
}

export interface FrontendPostSyncResult {
    uploadedTo: number;
    polledSyncs: number;
    importedPayments: number;
    importedReceipts: number;
    importedSignatures: number;
    newSignRequests: number;
    newWotHandshakes: number;
    newInquiries: number;
    newInquiryReplies: number;
    prunedBoxes: number;
    errors: string[];
    retryAfterSecs?: number | null;
}
