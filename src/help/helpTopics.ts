// src/help/helpTopics.ts
export type HelpCategory = 'basics' | 'vouchers' | 'trust' | 'marketplace' | 'security' | 'connection';

export type HelpTopicId =
    // Basics
    | 'conceptParadigm'
    | 'voucherBasics'
    | 'benutzerId'
    | 'conceptCommunity'
    | 'conceptPrivacy'
    | 'conceptFuture'
    | 'profile'
    // Vouchers
    | 'voucherMode'
    | 'voucherCreator'
    | 'voucherSignatures'
    | 'signatureRequest'
    | 'voucherTrust'
    | 'senderName'
    | 'privacy'
    // Trust
    | 'wotNetwork'
    | 'trustLevels'
    | 'wotReach'
    | 'wotSharing'
    // Marketplace
    | 'marketplace'
    | 'marketCard'
    | 'cardVisibility'
    | 'ads'
    // Security
    | 'backup'
    | 'offlineProtection'
    | 'conflicts'
    | 'mnemonic'
    | 'passphrase'
    | 'subaccount'
    | 'handover'
    | 'deviceBinding'
    | 'settingsPrivacy'
    // Connection
    | 'connection'
    | 'posMode'
    | 'exclusiveGateways'
    | 'gatewayTokens'
    | 'l2Protection';

export type HelpAudience = 'everyone' | 'expert';

export interface HelpTopic {
    id: HelpTopicId;
    category: HelpCategory;
    audience?: HelpAudience;
    titleKey: string;   // help.<id>.title
    shortKey: string;   // help.<id>.short  – max. 1 sentence, ~120 chars
    longKey: string;    // help.<id>.long   – Markdown-light: paragraphs (\n\n), "• " bullet lists, **bold**
    deepDiveKey?: string; // help.<id>.deepDive - Collapsible "Mehr erfahren / Technischer Hintergrund" section
    related?: HelpTopicId[];
}

export const HELP_CATEGORIES: { id: HelpCategory; labelKey: string }[] = [
    { id: 'basics', labelKey: 'helpCenter.categoryBasics' },
    { id: 'vouchers', labelKey: 'helpCenter.categoryVouchers' },
    { id: 'trust', labelKey: 'helpCenter.categoryTrust' },
    { id: 'marketplace', labelKey: 'helpCenter.categoryMarketplace' },
    { id: 'security', labelKey: 'helpCenter.categorySecurity' },
    { id: 'connection', labelKey: 'helpCenter.categoryConnection' },
];

export const HELP_TOPICS: Record<HelpTopicId, HelpTopic> = {
    // --- Basics ---
    conceptParadigm: {
        id: 'conceptParadigm',
        category: 'basics',
        titleKey: 'help.conceptParadigm.title',
        shortKey: 'help.conceptParadigm.short',
        longKey: 'help.conceptParadigm.long',
        related: ['voucherBasics', 'benutzerId', 'conceptCommunity'],
    },
    voucherBasics: {
        id: 'voucherBasics',
        category: 'basics',
        titleKey: 'help.voucherBasics.title',
        shortKey: 'help.voucherBasics.short',
        longKey: 'help.voucherBasics.long',
        related: ['conceptParadigm', 'voucherMode', 'voucherCreator', 'voucherTrust'],
    },
    benutzerId: {
        id: 'benutzerId',
        category: 'basics',
        titleKey: 'help.benutzerId.title',
        shortKey: 'help.benutzerId.short',
        longKey: 'help.benutzerId.long',
        related: ['profile', 'subaccount', 'mnemonic'],
    },
    conceptCommunity: {
        id: 'conceptCommunity',
        category: 'basics',
        titleKey: 'help.conceptCommunity.title',
        shortKey: 'help.conceptCommunity.short',
        longKey: 'help.conceptCommunity.long',
        related: ['voucherSignatures', 'signatureRequest', 'wotNetwork'],
    },
    conceptPrivacy: {
        id: 'conceptPrivacy',
        category: 'basics',
        titleKey: 'help.conceptPrivacy.title',
        shortKey: 'help.conceptPrivacy.short',
        longKey: 'help.conceptPrivacy.long',
        related: ['privacy', 'offlineProtection', 'connection'],
    },
    conceptFuture: {
        id: 'conceptFuture',
        category: 'basics',
        titleKey: 'help.conceptFuture.title',
        shortKey: 'help.conceptFuture.short',
        longKey: 'help.conceptFuture.long',
        related: ['conceptParadigm', 'marketplace'],
    },
    profile: {
        id: 'profile',
        category: 'basics',
        titleKey: 'help.profile.title',
        shortKey: 'help.profile.short',
        longKey: 'help.profile.long',
        related: ['benutzerId', 'backup', 'mnemonic', 'handover'],
    },

    // --- Vouchers ---
    voucherMode: {
        id: 'voucherMode',
        category: 'vouchers',
        titleKey: 'help.voucherMode.title',
        shortKey: 'help.voucherMode.short',
        longKey: 'help.voucherMode.long',
        related: ['voucherBasics', 'voucherCreator'],
    },
    voucherCreator: {
        id: 'voucherCreator',
        category: 'vouchers',
        titleKey: 'help.voucherCreator.title',
        shortKey: 'help.voucherCreator.short',
        longKey: 'help.voucherCreator.long',
        related: ['voucherBasics', 'voucherSignatures', 'voucherTrust'],
    },
    voucherSignatures: {
        id: 'voucherSignatures',
        category: 'vouchers',
        titleKey: 'help.voucherSignatures.title',
        shortKey: 'help.voucherSignatures.short',
        longKey: 'help.voucherSignatures.long',
        related: ['signatureRequest', 'voucherCreator', 'conceptCommunity'],
    },
    signatureRequest: {
        id: 'signatureRequest',
        category: 'vouchers',
        titleKey: 'help.signatureRequest.title',
        shortKey: 'help.signatureRequest.short',
        longKey: 'help.signatureRequest.long',
        related: ['voucherSignatures', 'voucherCreator'],
    },
    voucherTrust: {
        id: 'voucherTrust',
        category: 'vouchers',
        titleKey: 'help.voucherTrust.title',
        shortKey: 'help.voucherTrust.short',
        longKey: 'help.voucherTrust.long',
        related: ['wotNetwork', 'trustLevels', 'voucherCreator'],
    },
    senderName: {
        id: 'senderName',
        category: 'vouchers',
        titleKey: 'help.senderName.title',
        shortKey: 'help.senderName.short',
        longKey: 'help.senderName.long',
        related: ['privacy', 'benutzerId'],
    },
    privacy: {
        id: 'privacy',
        category: 'vouchers',
        titleKey: 'help.privacy.title',
        shortKey: 'help.privacy.short',
        longKey: 'help.privacy.long',
        related: ['settingsPrivacy', 'conceptPrivacy', 'senderName'],
    },

    // --- Trust ---
    wotNetwork: {
        id: 'wotNetwork',
        category: 'trust',
        titleKey: 'help.wotNetwork.title',
        shortKey: 'help.wotNetwork.short',
        longKey: 'help.wotNetwork.long',
        related: ['trustLevels', 'wotReach', 'wotSharing', 'voucherTrust'],
    },
    trustLevels: {
        id: 'trustLevels',
        category: 'trust',
        titleKey: 'help.trustLevels.title',
        shortKey: 'help.trustLevels.short',
        longKey: 'help.trustLevels.long',
        related: ['wotNetwork', 'wotReach', 'wotSharing'],
    },
    wotReach: {
        id: 'wotReach',
        category: 'trust',
        titleKey: 'help.wotReach.title',
        shortKey: 'help.wotReach.short',
        longKey: 'help.wotReach.long',
        related: ['wotNetwork', 'trustLevels', 'wotSharing'],
    },
    wotSharing: {
        id: 'wotSharing',
        category: 'trust',
        titleKey: 'help.wotSharing.title',
        shortKey: 'help.wotSharing.short',
        longKey: 'help.wotSharing.long',
        deepDiveKey: 'help.wotSharing.deepDive',
        related: ['wotNetwork', 'wotReach', 'trustLevels'],
    },

    // --- Marketplace ---
    marketplace: {
        id: 'marketplace',
        category: 'marketplace',
        titleKey: 'help.marketplace.title',
        shortKey: 'help.marketplace.short',
        longKey: 'help.marketplace.long',
        related: ['marketCard', 'ads', 'cardVisibility'],
    },
    marketCard: {
        id: 'marketCard',
        category: 'marketplace',
        titleKey: 'help.marketCard.title',
        shortKey: 'help.marketCard.short',
        longKey: 'help.marketCard.long',
        related: ['marketplace', 'cardVisibility', 'ads'],
    },
    cardVisibility: {
        id: 'cardVisibility',
        category: 'marketplace',
        titleKey: 'help.cardVisibility.title',
        shortKey: 'help.cardVisibility.short',
        longKey: 'help.cardVisibility.long',
        related: ['marketCard', 'marketplace'],
    },
    ads: {
        id: 'ads',
        category: 'marketplace',
        titleKey: 'help.ads.title',
        shortKey: 'help.ads.short',
        longKey: 'help.ads.long',
        related: ['marketplace', 'marketCard'],
    },

    // --- Security ---
    backup: {
        id: 'backup',
        category: 'security',
        titleKey: 'help.backup.title',
        shortKey: 'help.backup.short',
        longKey: 'help.backup.long',
        related: ['profile', 'mnemonic', 'passphrase'],
    },
    offlineProtection: {
        id: 'offlineProtection',
        category: 'security',
        titleKey: 'help.offlineProtection.title',
        shortKey: 'help.offlineProtection.short',
        longKey: 'help.offlineProtection.long',
        related: ['conflicts', 'conceptPrivacy'],
    },
    conflicts: {
        id: 'conflicts',
        category: 'security',
        titleKey: 'help.conflicts.title',
        shortKey: 'help.conflicts.short',
        longKey: 'help.conflicts.long',
        related: ['offlineProtection'],
    },
    mnemonic: {
        id: 'mnemonic',
        category: 'security',
        titleKey: 'help.mnemonic.title',
        shortKey: 'help.mnemonic.short',
        longKey: 'help.mnemonic.long',
        related: ['passphrase', 'backup', 'profile'],
    },
    passphrase: {
        id: 'passphrase',
        category: 'security',
        audience: 'expert',
        titleKey: 'help.passphrase.title',
        shortKey: 'help.passphrase.short',
        longKey: 'help.passphrase.long',
        related: ['mnemonic', 'backup'],
    },
    subaccount: {
        id: 'subaccount',
        category: 'security',
        audience: 'expert',
        titleKey: 'help.subaccount.title',
        shortKey: 'help.subaccount.short',
        longKey: 'help.subaccount.long',
        related: ['handover', 'deviceBinding', 'benutzerId'],
    },
    handover: {
        id: 'handover',
        category: 'security',
        audience: 'expert',
        titleKey: 'help.handover.title',
        shortKey: 'help.handover.short',
        longKey: 'help.handover.long',
        related: ['subaccount', 'deviceBinding', 'offlineProtection'],
    },
    deviceBinding: {
        id: 'deviceBinding',
        category: 'security',
        audience: 'expert',
        titleKey: 'help.deviceBinding.title',
        shortKey: 'help.deviceBinding.short',
        longKey: 'help.deviceBinding.long',
        related: ['profile', 'handover'],
    },
    settingsPrivacy: {
        id: 'settingsPrivacy',
        category: 'security',
        titleKey: 'help.settingsPrivacy.title',
        shortKey: 'help.settingsPrivacy.short',
        longKey: 'help.settingsPrivacy.long',
        related: ['privacy', 'conceptPrivacy'],
    },

    // --- Connection ---
    connection: {
        id: 'connection',
        category: 'connection',
        titleKey: 'help.connection.title',
        shortKey: 'help.connection.short',
        longKey: 'help.connection.long',
        related: ['conceptPrivacy', 'wotSharing'],
    },
    posMode: {
        id: 'posMode',
        category: 'connection',
        audience: 'expert',
        titleKey: 'help.posMode.title',
        shortKey: 'help.posMode.short',
        longKey: 'help.posMode.long',
        related: ['voucherBasics'],
    },
    exclusiveGateways: {
        id: 'exclusiveGateways',
        category: 'connection',
        audience: 'expert',
        titleKey: 'help.exclusiveGateways.title',
        shortKey: 'help.exclusiveGateways.short',
        longKey: 'help.exclusiveGateways.long',
        related: ['gatewayTokens', 'connection'],
    },
    gatewayTokens: {
        id: 'gatewayTokens',
        category: 'connection',
        audience: 'expert',
        titleKey: 'help.gatewayTokens.title',
        shortKey: 'help.gatewayTokens.short',
        longKey: 'help.gatewayTokens.long',
        related: ['exclusiveGateways'],
    },
    l2Protection: {
        id: 'l2Protection',
        category: 'connection',
        audience: 'expert',
        titleKey: 'help.l2Protection.title',
        shortKey: 'help.l2Protection.short',
        longKey: 'help.l2Protection.long',
        deepDiveKey: 'help.l2Protection.deepDive',
        related: ['connection', 'offlineProtection', 'posMode'],
    },
};
