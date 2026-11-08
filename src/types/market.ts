// src/types/market.ts

import { FrontendUserProfile } from './profile';

export type FrontendListingType = 'offer' | 'request';
export type FrontendListingOrigin = 'voucher' | 'card';
export type FrontendTrustTier = 'trusted' | 'unknown' | 'hidden';

export interface FrontendMarketListing {
    listingType: FrontendListingType;
    title: string;
    text?: string | null;
    pricing?: string | null;
    origin: FrontendListingOrigin;
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
    adminTokenHash: string;
    maxMembers: number;
    policy: FrontendMemberPolicyDto;
    zipBase64: string;
}

export interface FrontendDropAdminStatus {
    descriptor: FrontendDropDescriptor;
    dataDir: string;
    maxMembers: number;
    cardPolicy: FrontendMemberPolicyDto;
    allow: string[];
    deny: string[];
    blockedAccessIds: string[];
    directMemberCount: number;
    hop2MemberCount: number;
    totalMemberCount: number;
    activeSlotIds: string[];
}

export interface FrontendDropAdminUpdateRequest {
    cardPolicy?: FrontendMemberPolicyDto | null;
    allow?: string[] | null;
    deny?: string[] | null;
    blockedAccessIds?: string[] | null;
    maxMembers?: number | null;
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
    errors: string[];
}

export interface FrontendTrustChangeImpact {
    canReadMyMarketBefore: boolean;
    canReadMyMarketAfter: boolean;
    canWriteMyDropBefore: boolean;
    canWriteMyDropAfter: boolean;
}

