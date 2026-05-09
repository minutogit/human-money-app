// src/types/profile.ts

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
