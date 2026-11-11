// src/types/market.ts

import { FrontendUserProfile } from './profile';

export type FieldAudience = 'everyone' | 'contacts' | 'nobody';

export interface CardFieldVisibility {
    name: FieldAudience;
    organization: FieldAudience;
    community: FieldAudience;
    city: FieldAudience;
    url: FieldAudience;
    picture: FieldAudience;
    street: FieldAudience;
    email: FieldAudience;
    phone: FieldAudience;
    coordinates: FieldAudience;
    gender: FieldAudience;
}

export interface Skill {
    title: string;
    text?: string | null;
    pricing?: string | null;
    is_remote?: boolean;
}

export type AdKind = 'offer' | 'request';

export interface Ad {
    id: string;
    kind: AdKind;
    title: string;
    text?: string | null;
    pricing?: string | null;
    published_at: number;
    renewed_at?: number | null;
    expires_at: number;
    allow_inquiries: boolean;
    is_remote?: boolean;
}

export interface FrontendMyAd {
    id: string;
    kind: AdKind;
    title: string;
    text?: string | null;
    pricing?: string | null;
    publishedAt: number;
    renewedAt?: number | null;
    expiresAt: number;
    allowInquiries: boolean;
    isRemote?: boolean;
    status: 'active' | 'expired' | string;
    daysLeft: number;
}

export interface FrontendMyMarketProfile {
    profile: FrontendUserProfile;
    visibility: CardFieldVisibility;
    skills: Skill[];
    ads: FrontendMyAd[];
    maxHops: number;
    validityYears?: number;
    allowInquiries: boolean;
    published: boolean;
    shareUri?: string | null;
    fileWire?: string | null;
    qrSvg?: string | null;
    qrIsPartial: boolean;
}

export interface FrontendSaveMyMarketProfileRequest {
    profile: FrontendUserProfile;
    visibility: CardFieldVisibility;
    skills: Skill[];
    maxHops: number;
    validityYears?: number;
    allowInquiries: boolean;
}

export interface FrontendAdInput {
    id?: string;
    kind: AdKind;
    title: string;
    text?: string | null;
    pricing?: string | null;
    durationDays: number;
    allowInquiries: boolean;
    isRemote?: boolean;
}

export type FrontendTrustTier = 'trusted' | 'unknown' | 'hidden';

export interface FrontendBoardItem {
    ownerDid: string;
    ownerName: string;
    trust: FrontendTrustTier;
    ad: Ad;
    daysLeft: number;
}

export type FrontendSkillOrigin = 'card' | 'voucher';

export interface FrontendMarketSkill {
    title: string;
    text?: string | null;
    pricing?: string | null;
    origin: FrontendSkillOrigin;
    isRemote?: boolean;
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
    skills: FrontendMarketSkill[];
    activeAds: Ad[];
    adsPending: number;
    allowInquiries: boolean;
    sources: FrontendEntrySource[];
    asOf: number; // unix seconds
    standards: FrontendStandardRef[];
    trust: FrontendTrustTier;
    canForward: boolean;
    cardSyncs: FrontendSyncDescriptor[];
}

export interface FrontendMarketSnapshot {
    board: FrontendBoardItem[];
    people: FrontendMarketPerson[];
    now: number;
    storedCardCount?: number;
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

export type InquiryTarget =
    | { kind: 'ad'; ad_id: string; ad_title: string }
    | { kind: 'person'; skill_title?: string };

export interface FrontendMarketInquiryReplyDto {
    inquiryId: string;
    decision: 'confirm' | 'decline' | string;
    note?: string | null;
    paymentUri?: string | null;
    amountHint?: string | null;
    standardUuid?: string | null;
    repliedAt: number;
}

export interface FrontendMarketInquiryItem {
    inquiryId: string;
    target: InquiryTarget;
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
