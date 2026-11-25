import { describe, it, expect } from 'vitest';
import { HELP_TOPICS, HelpTopicId, HELP_GROUPS } from '../helpTopics';
import deJson from '../../locales/de.json';
import enJson from '../../locales/en.json';

describe('helpTopics Registry', () => {
    const topicIds = Object.keys(HELP_TOPICS) as HelpTopicId[];
    const de = deJson as Record<string, string>;
    const en = enJson as Record<string, string>;

    const assertNonEmptyLocaleString = (
        dict: Record<string, string>,
        key: string,
        label: string
    ) => {
        expect(typeof dict[key], `${label} (${key}) must exist as a string`).toBe('string');
        expect(dict[key].trim().length, `${label} (${key}) must not be blank`).toBeGreaterThan(0);
    };

    it('has non-empty topic list', () => {
        expect(topicIds.length).toBeGreaterThan(20);
    });

    it('ensures every topic has valid group defined in HELP_GROUPS and aligned audience', () => {
        const validGroups = new Set(HELP_GROUPS.map((g) => g.id));
        expect(HELP_GROUPS).toHaveLength(7);
        const groupCounts = new Map<string, number>();

        for (const group of HELP_GROUPS) {
            assertNonEmptyLocaleString(de, group.labelKey, 'DE group label');
            assertNonEmptyLocaleString(en, group.labelKey, 'EN group label');
            groupCounts.set(group.id, 0);
        }

        for (const id of topicIds) {
            const topic = HELP_TOPICS[id];
            expect(topic.id).toBe(id);
            expect(validGroups.has(topic.group)).toBe(true);
            expect(['everyone', 'expert']).toContain(topic.audience);
            expect(topic.audience === 'expert').toBe(topic.group === 'expert');
            groupCounts.set(topic.group, (groupCounts.get(topic.group) ?? 0) + 1);
        }

        for (const group of HELP_GROUPS) {
            expect(groupCounts.get(group.id) ?? 0, `Group ${group.id} must not be empty`).toBeGreaterThan(0);
        }
        expect('wotReach' in HELP_TOPICS).toBe(false);
        expect('wotSharing' in HELP_TOPICS).toBe(false);
        expect('posMode' in HELP_TOPICS).toBe(false);
    });

    it('ensures every topic has non-empty titleKey, shortKey, longKey, and deepDiveKey in de.json and en.json', () => {
        for (const id of topicIds) {
            const topic = HELP_TOPICS[id];

            expect(topic.titleKey).toBe(`help.${id}.title`);
            assertNonEmptyLocaleString(de, topic.titleKey, 'DE title');
            assertNonEmptyLocaleString(en, topic.titleKey, 'EN title');

            expect(topic.shortKey).toBe(`help.${id}.short`);
            assertNonEmptyLocaleString(de, topic.shortKey, 'DE short');
            assertNonEmptyLocaleString(en, topic.shortKey, 'EN short');

            expect(topic.longKey).toBe(`help.${id}.long`);
            assertNonEmptyLocaleString(de, topic.longKey, 'DE long');
            assertNonEmptyLocaleString(en, topic.longKey, 'EN long');
        }

        const deepDiveTopics = topicIds
            .map((id) => HELP_TOPICS[id])
            .filter(
                (t): t is (typeof HELP_TOPICS)[HelpTopicId] & { deepDiveKey: string } =>
                    typeof t.deepDiveKey === 'string'
            );
        expect(deepDiveTopics.length).toBeGreaterThan(0);

        for (const topic of deepDiveTopics) {
            expect(topic.deepDiveKey).toBe(`help.${topic.id}.deepDive`);
            assertNonEmptyLocaleString(de, topic.deepDiveKey, 'DE deepDive');
            assertNonEmptyLocaleString(en, topic.deepDiveKey, 'EN deepDive');
        }
    });

    it('ensures every topic defines non-empty, unique, valid related links without self-references', () => {
        const topicIdSet = new Set(topicIds);
        for (const id of topicIds) {
            const topic = HELP_TOPICS[id];
            const related = topic.related ?? [];
            expect(Array.isArray(topic.related), `Topic ${id} must declare related topics`).toBe(true);
            expect(related.length, `Topic ${id} must have at least 1 related topic`).toBeGreaterThan(0);
            expect(new Set(related).size, `Topic ${id} has duplicate related links`).toBe(related.length);

            for (const relId of related) {
                expect(topicIdSet.has(relId), `Invalid related topic ID: ${relId} on ${id}`).toBe(true);
                expect(relId, `Topic ${id} should not relate to itself`).not.toBe(id);
            }
        }
    });

    it('ensures all helpCenter UI keys exist in de.json and en.json', () => {
        const expectedUiKeys = [
            'helpCenter.title',
            'helpCenter.subtitle',
            'helpCenter.menuTitle',
            'helpCenter.readConceptLink',
            'helpCenter.searchPlaceholder',
            'helpCenter.noResults',
            'helpCenter.clearSearch',
            'helpCenter.categoryAll',
            'helpCenter.groupFirstSteps',
            'helpCenter.groupDailyVouchers',
            'helpCenter.groupContactsTrust',
            'helpCenter.groupMarketplace',
            'helpCenter.groupSecurityBackup',
            'helpCenter.groupFaq',
            'helpCenter.groupExpert',
            'helpCenter.forAdvancedUsers',
            'helpCenter.relatedTopics',
            'helpCenter.openInHelpCenter',
            'helpCenter.learnMore',
            'helpCenter.backToTopics',
            'helpCenter.backToLogin',
            'helpCenter.selectTopicPrompt',
            'helpCenter.deepDiveTitle',
        ];

        for (const k of expectedUiKeys) {
            assertNonEmptyLocaleString(de, k, 'DE UI key');
            assertNonEmptyLocaleString(en, k, 'EN UI key');
        }

        const expectedNewIds: HelpTopicId[] = [
            'whichBackup',
            'deviceLoss',
            'socialRecovery',
            'emergencySheet',
            'voucherStandards',
            'voucherExpiry',
            'offlineRisk',
            'multiDevice',
            'profileSeal',
            'guarantorVsRecommendation',
            'visibilityReach',
            'uiModes',
            'sendReceive',
            'faqIsItCrypto',
            'faqWhatDoesItCost',
            'faqCanIBeCheated',
            'faqSyncDown',
        ];
        for (const newId of expectedNewIds) {
            expect(HELP_TOPICS[newId]?.id).toBe(newId);
        }
    });
});
