// src/utils/signatureHints.ts
import { PublicProfile } from "../types";

/**
 * Mapping of CEL field references to human-readable display names
 * and corresponding PublicProfile properties.
 */
export const PROFILE_FIELD_MAPPINGS: Record<string, { label: string; checkFn: (profile: PublicProfile) => boolean }> = {
    'gender': {
        label: 'Gender',
        checkFn: (profile) => !!profile.gender && profile.gender.trim() !== ''
    },
    'address': {
        label: 'Address',
        checkFn: (profile) => !!profile.address && (
            !!profile.address.street?.trim() ||
            !!profile.address.city?.trim() ||
            !!profile.address.zip_code?.trim()
        )
    },
    'coordinates': {
        label: 'Coordinates',
        checkFn: (profile) => !!profile.coordinates && profile.coordinates.trim() !== ''
    },
    'email': {
        label: 'Email',
        checkFn: (profile) => !!profile.email && profile.email.trim() !== ''
    },
    'phone': {
        label: 'Phone',
        checkFn: (profile) => !!profile.phone && profile.phone.trim() !== ''
    },
    'organization': {
        label: 'Organization',
        checkFn: (profile) => !!profile.organization && profile.organization.trim() !== ''
    },
    'community': {
        label: 'Community',
        checkFn: (profile) => !!profile.community && profile.community.trim() !== ''
    },
    'url': {
        label: 'Website',
        checkFn: (profile) => !!profile.url && profile.url.trim() !== ''
    },
    'service_offer': {
        label: 'Service Offer',
        checkFn: (profile) => !!profile.service_offer && profile.service_offer.trim() !== ''
    },
    'needs': {
        label: 'Needs',
        checkFn: (profile) => !!profile.needs && profile.needs.trim() !== ''
    }
};

/**
 * Extracts CEL field references from a custom rule expression.
 * Uses robust string-based pattern matching to find references like:
 * - s.details.gender
 * - s.details.address
 * - has(s.details.field)
 */
function extractFieldReferences(expression: string): string[] {
    const fields: string[] = [];
    
    // Matches .details.fieldName or s.details.fieldName
    // We look for anything that follows '.details.'
    const detailsPattern = /\.details\.(\w+)/g;
    let match;
    while ((match = detailsPattern.exec(expression)) !== null) {
        const field = match[1];
        if (!fields.includes(field)) {
            fields.push(field);
        }
    }
    
    return fields;
}

/**
 * Parses TOML content to extract custom_rules expressions.
 * This is a simple string-based parser that finds [immutable.custom_rules.*] sections.
 */
function extractCustomRules(tomlContent: string): Array<{ name: string; expression: string }> {
    const rules: Array<{ name: string; expression: string }> = [];
    const lines = tomlContent.split('\n');
    let currentRule: { name: string; expression?: string } | null = null;
    
    for (const line of lines) {
        // Check for rule section header: [immutable.custom_rules.rule_name] or [mutable.custom_rules.rule_name]
        const sectionMatch = line.match(/^\[(?:immutable|mutable)\.custom_rules\.([^\]]+)\]/);
        if (sectionMatch) {
            // Save previous rule if it has an expression
            if (currentRule && currentRule.expression) {
                rules.push({ name: currentRule.name, expression: currentRule.expression });
            }
            currentRule = { name: sectionMatch[1] };
            continue;
        }
        
        // Check for expression line within a rule section
        if (currentRule && line.trim().startsWith('expression')) {
            // Find the first and last quote on the line to be more robust
            const firstQuoteIdx = line.indexOf('"');
            const lastQuoteIdx = line.lastIndexOf('"');
            
            if (firstQuoteIdx !== -1 && lastQuoteIdx !== -1 && firstQuoteIdx !== lastQuoteIdx) {
                currentRule.expression = line.substring(firstQuoteIdx + 1, lastQuoteIdx);
            } else {
                // Try single quotes if double quotes failed
                const firstSQuoteIdx = line.indexOf("'");
                const lastSQuoteIdx = line.lastIndexOf("'");
                if (firstSQuoteIdx !== -1 && lastSQuoteIdx !== -1 && firstSQuoteIdx !== lastSQuoteIdx) {
                    currentRule.expression = line.substring(firstSQuoteIdx + 1, lastSQuoteIdx);
                }
            }
        }
    }
    
    // Save the last rule
    if (currentRule && currentRule.expression) {
        rules.push({ name: currentRule.name, expression: currentRule.expression });
    }
    
    return rules;
}

/**
 * Main function to generate a profile hint based on voucher standard content.
 * 
 * @param standardContent - The TOML content of the voucher standard
 * @param currentProfile - The user's current PublicProfile
 * @returns A compact hint string describing missing profile fields, or null if no hints
 */
export function getMissingProfileHint(
    standardContent: string,
    currentProfile: PublicProfile | null
): string | null {
    if (!currentProfile) {
        return null; // Return null instead of a message to avoid noise when no profile is loaded
    }
    
    // Extract custom rules from the standard
    const customRules = extractCustomRules(standardContent);
    
    if (customRules.length === 0) {
        return null; // No custom rules, no hints needed
    }
    
    // Collect all field references from all rules
    const allFields = new Set<string>();
    for (const rule of customRules) {
        const fields = extractFieldReferences(rule.expression);
        fields.forEach(f => allFields.add(f));
    }
    
    // Check which fields are missing or empty in the profile
    const missingFields: string[] = [];
    for (const field of allFields) {
        const mapping = PROFILE_FIELD_MAPPINGS[field];
        if (mapping && !mapping.checkFn(currentProfile)) {
            missingFields.push(mapping.label);
        }
    }
    
    if (missingFields.length === 0) {
        return null; // All required fields are present
    }
    
    // Generate a compact hint string
    const prefix = "This standard likely checks for: ";
    if (missingFields.length === 1) {
        return `${prefix}${missingFields[0]}`;
    } else if (missingFields.length === 2) {
        return `${prefix}${missingFields[0]} and ${missingFields[1]}`;
    } else {
        const lastField = missingFields.pop();
        return `${prefix}${missingFields.join(', ')} and ${lastField}`;
    }
}
