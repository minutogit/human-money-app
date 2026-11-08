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
    | { kind: 'drop'; readUrl: string };

export interface FrontendStandardRef {
    name: string;
    uuid: string;
}

export interface FrontendDropDescriptor {
    writeUrl: string;
    readUrl: string;
    slotId?: string | null;
    dropPub: string;
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
    cardDrops: FrontendDropDescriptor[];
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
    drops: FrontendDropDescriptor[];
}

export interface FrontendOwnContactCard {
    card: FrontendContactCardDto;
    cardJws: string;
    qrSvg?: string | null;
    shareUri: string;
    envelopeJson: string;
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

export interface FrontendDropSetupDraft {
    descriptor: FrontendDropDescriptor;
    dataDir: string;
    adminPubkey: string;
    generation: number;
    maxMembers: number;
    maxTotalStorageMb: number;
    policy: FrontendMemberPolicyDto;
    zipBase64: string;
}

export interface FrontendDropAdminStatus {
    descriptor: FrontendDropDescriptor;
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

export interface FrontendDropAdminUpdateRequest {
    cardPolicy?: FrontendMemberPolicyDto | null;
    allow?: string[] | null;
    deny?: string[] | null;
    blockedPubkeys?: string[] | null;
    maxMembers?: number | null;
    maxTotalStorageMb?: number | null;
}

export interface FrontendPublicDropInfo {
    writeUrl: string;
    readUrl: string;
    dropPub: string;
    registerUrl?: string | null;
    accessPubkey: string;
    authorizedChannels: string[];
}

export interface FrontendDropOverviewStatus {
    connectedCount: number;
    friendDrops: number;
    publicDrops: number;
    hasOwnDrop: boolean;
    ownDropSoftWarning: boolean;
    lastSyncAt?: number | null;
}

export interface StoredPublicDrop {
    writeUrl: string;
    readUrl: string;
    dropPub: string;
    registerUrl?: string | null;
    accessPubkey: string;
    enabled: boolean;
    authorizedChannels: string[];
}

export interface FrontendDropConnectionTestResult {
    success: boolean;
    message: string;
    writeOk: boolean;
    readOk: boolean;
}

export interface FrontendDiscoveredDrop {
    descriptor: FrontendDropDescriptor;
    isAuthorized: boolean;
    isSelected: boolean;
    sourceLabel: string;
}

export interface FrontendDropSyncResult {
    skippedWifiOnly: boolean;
    uploadedTo: number;
    polledDrops: number;
    decryptedSlots: number;
    importedCards: number;
    importedWotLists: number;
    errors: string[];
}

export interface FrontendTrustChangeImpact {
    canReadMyMarketBefore: boolean;
    canReadMyMarketAfter: boolean;
    canReadMyWotBefore: boolean;
    canReadMyWotAfter: boolean;
    canWriteMyDropBefore: boolean;
    canWriteMyDropAfter: boolean;
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
    replyDrops: FrontendDropDescriptor[];
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
    replyDrops: FrontendDropDescriptor[];
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
    polledDrops: number;
    importedPayments: number;
    importedReceipts: number;
    importedSignatures: number;
    newSignRequests: number;
    newWotHandshakes: number;
    newInquiries: number;
    newInquiryReplies: number;
    prunedBoxes: number;
    errors: string[];
}

