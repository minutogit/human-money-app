// src/types/wot.ts

import { PublicProfile } from './profile';

export interface VoucherTrustDto {
    score: number;
    trustPercent: number;
    isOwnVoucher: boolean;
    blockedByOffender: boolean;
    shortestHops?: number | null;
    primaryBridgeDid?: string | null;
    primaryBridgeName?: string | null;
    additionalPathsCount: number;
    viaGuarantor: boolean;
    bestPathNames: string[];
}

export interface WotSummaryDto {
    ratedContactsCount: number;
    verifiedHumansCount: number;
    closeConfidantsCount: number;
    importedPeerListsCount: number;
    ownListVersion?: number | null;
}

export interface WotImportResultDto {
    senderDid: string;
    senderProfile?: PublicProfile | null;
    imported: number;
    updated: number;
    merged: number;
    skippedUnchanged: number;
    rejectedSpam: number;
    isKnownContact: boolean;
    existingTrustLevel?: number | null;
}
