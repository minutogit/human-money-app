import { PublicProfile, VoucherStandardDefinition } from "../types";

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
            !!profile.address.zipCode?.trim()
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
    'serviceOffer': {
        label: 'Service Offer',
        checkFn: (profile) => !!profile.serviceOffer && profile.serviceOffer.trim() !== ''
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
 * Main function to generate a profile hint based on voucher standard content.
 * 
 * @param standard - The parsed voucher standard definition
 * @param currentProfile - The user's current PublicProfile
 * @returns A compact hint string describing missing profile fields, or null if no hints
 */
export function getMissingProfileHint(
    standard: VoucherStandardDefinition,
    currentProfile: PublicProfile | null
): string | null {
    if (!currentProfile) {
        return null; // Return null instead of a message to avoid noise when no profile is loaded
    }
    
    // Collect all field references from all rules
    const allFields = new Set<string>();
    
    // Check custom rules in immutable zone
    if (standard.immutable.customRules) {
        for (const rule of Object.values(standard.immutable.customRules)) {
            const fields = extractFieldReferences(rule.expression);
            fields.forEach(f => allFields.add(f));
        }
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
