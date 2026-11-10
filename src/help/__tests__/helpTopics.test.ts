import { describe, it, expect } from 'vitest';
import { HELP_TOPICS, HelpTopicId, HELP_CATEGORIES } from '../helpTopics';
import deJson from '../../locales/de.json';
import enJson from '../../locales/en.json';

describe('helpTopics Registry', () => {
    const topicIds = Object.keys(HELP_TOPICS) as HelpTopicId[];
    const de = deJson as Record<string, string>;
    const en = enJson as Record<string, string>;

    it('has non-empty topic list', () => {
        expect(topicIds.length).toBeGreaterThan(20);
    });

    it('ensures every topic has valid category defined in HELP_CATEGORIES', () => {
        const validCategories = new Set(HELP_CATEGORIES.map((c) => c.id));
        for (const id of topicIds) {
            const topic = HELP_TOPICS[id];
            expect(validCategories.has(topic.category)).toBe(true);
        }
    });

    it('ensures every topic has non-empty titleKey, shortKey, and longKey in de.json and en.json', () => {
        for (const id of topicIds) {
            const topic = HELP_TOPICS[id];

            // titleKey
            expect(topic.titleKey).toBe(`help.${id}.title`);
            expect(de[topic.titleKey], `Missing DE for ${topic.titleKey}`).toBeDefined();
            expect(de[topic.titleKey].trim().length).toBeGreaterThan(0);
            expect(en[topic.titleKey], `Missing EN for ${topic.titleKey}`).toBeDefined();
            expect(en[topic.titleKey].trim().length).toBeGreaterThan(0);

            // shortKey
            expect(topic.shortKey).toBe(`help.${id}.short`);
            expect(de[topic.shortKey], `Missing DE for ${topic.shortKey}`).toBeDefined();
            expect(de[topic.shortKey].trim().length).toBeGreaterThan(0);
            expect(en[topic.shortKey], `Missing EN for ${topic.shortKey}`).toBeDefined();
            expect(en[topic.shortKey].trim().length).toBeGreaterThan(0);

            // longKey
            expect(topic.longKey).toBe(`help.${id}.long`);
            expect(de[topic.longKey], `Missing DE for ${topic.longKey}`).toBeDefined();
            expect(de[topic.longKey].trim().length).toBeGreaterThan(0);
            expect(en[topic.longKey], `Missing EN for ${topic.longKey}`).toBeDefined();
            expect(en[topic.longKey].trim().length).toBeGreaterThan(0);
        }
    });

    it('ensures every related topic references a valid HelpTopicId and not itself', () => {
        const topicIdSet = new Set(topicIds);
        for (const id of topicIds) {
            const topic = HELP_TOPICS[id];
            if (topic.related) {
                for (const relId of topic.related) {
                    expect(topicIdSet.has(relId), `Invalid related topic ID: ${relId} on ${id}`).toBe(true);
                    expect(relId, `Topic ${id} should not relate to itself`).not.toBe(id);
                }
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
            'helpCenter.categoryBasics',
            'helpCenter.categoryVouchers',
            'helpCenter.categoryTrust',
            'helpCenter.categoryMarketplace',
            'helpCenter.categorySecurity',
            'helpCenter.categoryConnection',
            'helpCenter.relatedTopics',
            'helpCenter.openInHelpCenter',
            'helpCenter.learnMore',
            'helpCenter.backToTopics',
            'helpCenter.backToLogin',
            'helpCenter.selectTopicPrompt',
        ];

        for (const k of expectedUiKeys) {
            expect(de[k], `Missing DE key ${k}`).toBeDefined();
            expect(de[k].trim().length).toBeGreaterThan(0);
            expect(en[k], `Missing EN key ${k}`).toBeDefined();
            expect(en[k].trim().length).toBeGreaterThan(0);
        }
    });
});
