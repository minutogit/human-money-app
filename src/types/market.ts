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
    isRemote?: boolean;
}

export type AdKind = 'offer' | 'request';

export interface Ad {
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
    imageToken?: string | null;
}

export interface FrontendMyAd extends Ad {
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
    mediaMaxBytes?: number;
    mediaMaxCount?: number;
    mediaUsedCount?: number;
}

export interface FrontendSaveMyMarketProfileRequest {
    profile: FrontendUserProfile;
    visibility: CardFieldVisibility;
    skills: Skill[];
    maxHops: number;
    validityYears?: number;
    allowInquiries: boolean;
    published?: boolean;
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
    imageWebpDataUri?: string | null;
}

export type FrontendTrustTier = 'trusted' | 'unknown' | 'hidden';

export interface FrontendBoardItem {
    ownerDid: string;
    recipientDid?: string;
    ownerName: string;
    trust: FrontendTrustTier;
    ad: Ad;
    daysLeft: number;
    isOwn?: boolean;
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

export type SyncEndpointStatus = 'unverified' | 'pending_auth' | 'active' | 'unreachable' | 'deprecating' | 'revoked';

export interface FrontendSyncReplacementTarget {
    writeUrl: string;
    readUrl: string;
    dropPub: string;
}

export interface SyncRevocationNotice {
    drop_pub: string;
    issued_at: number;
    effective_at: number;
    reason?: string | null;
    replacement?: {
        write_url: string;
        read_url: string;
        drop_pub: string;
    } | null;
    signature: string;
}

export interface FrontendSyncDescriptor {
    writeUrl: string;
    readUrl: string;
    slotId?: string | null;
    dropPub: string;
    lastSeenAt?: number | null;
    lastErrorAt?: number | null;
    failCount?: number;
    status?: SyncEndpointStatus | string;
    lastProbedAt?: number | null;
    sunsetAt?: number | null;
    revocationReason?: string | null;
    replacement?: FrontendSyncReplacementTarget | null;
}

export interface FrontendMarketPerson {
    did: string;
    recipientDid?: string;
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
    maxMembers: number;
    maxTotalStorageMb: number;
    maxUserStorageMb: number;
    policy: FrontendMemberPolicyDto;
    zipBase64: string;
    rustConfigToml?: string;
}

export interface FrontendSyncAdminStatus {
    descriptor: FrontendSyncDescriptor;
    adminPubkey: string;
    dataDir: string;
    maxMembers: number;
    maxTotalStorageMb: number;
    maxUserStorageMb: number;
    usedBytes?: number | null;
    maxBytes?: number | null;
    softWarning: boolean;
    cardPolicy: FrontendMemberPolicyDto;
    allow: string[];
    totalMemberCount: number;
    revocation?: SyncRevocationNotice | null;
}

export interface FrontendSyncAdminUpdateRequest {
    cardPolicy?: FrontendMemberPolicyDto | null;
    allow?: string[] | null;
    maxMembers?: number | null;
    maxTotalStorageMb?: number | null;
    maxUserStorageMb?: number | null;
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
    lastSyncAt?: number | null;
    pendingAuthCount?: number;
    failingCount?: number;
}

export interface StoredPublicSync {
    writeUrl: string;
    readUrl: string;
    dropPub: string;
    registerUrl?: string | null;
    accessPubkey: string;
    enabled: boolean;
    authorizedChannels: string[];
    status?: SyncEndpointStatus | string;
    sunsetAt?: number | null;
    revocationReason?: string | null;
    replacement?: FrontendSyncReplacementTarget | null;
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
    status?: SyncEndpointStatus | string;
}

export type MarketSyncStatus =
    | 'completed'
    | 'skippedWifiOnly'
    | 'skippedCooldown'
    | 'sessionSuperseded';

export interface FrontendMarketSyncResult {
    status: MarketSyncStatus;
    uploadedTo: number;
    polledSyncs: number;
    decryptedSlots: number;
    importedCards: number;
    newAds?: number;
    importedWotLists: number;
    errors: string[];
    retryAfterSecs?: number | null;
    revokedOwnSyncs?: FrontendSyncDescriptor[];
}

export interface FrontendAppNotice {
    id: string;
    kind: 'payment' | 'inquiry' | 'market' | 'trust' | string;
    createdAt: number;
    read: boolean;
    importedPayments: number;
    importedReceipts: number;
    importedSignatures: number;
    newSignRequests: number;
    newWotHandshakes: number;
    newInquiries: number;
    newInquiryMessages: number;
    importedCards: number;
    newAds: number;
    counterpartyName?: string | null;
    inquiryId?: string | null;
}

export interface FrontendAppNoticesResponse {
    lastSyncAt?: number | null;
    notices: FrontendAppNotice[];
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
    recipientDepositedTo: number;
    msgId: string;
    deferredCount?: number;
    queuedForNextFlush?: boolean;
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

export interface FrontendInquiryChatMessage {
    msgId: string;
    sender: 'me' | 'them' | string;
    text?: string | null;
    decision?: 'confirm' | 'decline' | string | null;
    createdAt: number;
}

export interface FrontendMarketInquiryItem {
    inquiryId: string;
    role: 'seller' | 'buyer' | string;
    target: InquiryTarget;
    counterpartyDid: string;
    counterpartyName?: string | null;
    counterpartyPictureUrl?: string | null;
    isContact: boolean;
    status: 'open' | 'confirmed' | 'declined' | string;
    paymentUri?: string | null;
    amountHint?: string | null;
    standardUuid?: string | null;
    createdAt: number;
    updatedAt: number;
    unread: boolean;
    replySyncs: FrontendSyncDescriptor[];
    messages: FrontendInquiryChatMessage[];
}

export interface FrontendResolvedCardRecipient {
    recipientId: string;
    did: string;
    displayName?: string | null;
}

export interface FrontendUnsupportedPostItem {
    msgId: string;
    senderDid: string;
    senderName?: string | null;
    payloadType: string;
    createdAt: number;
}

export interface FrontendPendingTolerancePayment {
    msgId: string;
    senderDid: string;
    senderName?: string | null;
    createdAt: number;
    zone: 'soft' | 'critical';
    errorMessage: string;
    hasConflict: boolean;
}

export interface FrontendPostInboxSummary {
    pendingSignRequests: FrontendPendingSignRequest[];
    pendingWotHandshakes: FrontendPendingWotHandshake[];
    inquiries: FrontendMarketInquiryItem[];
    unacknowledgedPayments: string[];
    unsupportedMessages?: FrontendUnsupportedPostItem[];
    pendingTolerancePayments?: FrontendPendingTolerancePayment[];
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
    newInquiryMessages: number;
    newTolerancePayments?: number;
    prunedBoxes: number;
    deferredOutboxBoxes?: number;
    skippedWifiOnly?: boolean;
    errors: string[];
    retryAfterSecs?: number | null;
}
