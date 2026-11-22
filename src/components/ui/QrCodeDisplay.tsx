import React from 'react';
import { QrCode } from 'lucide-react';

export interface QrCodeDisplayProps {
    /** Backend-rendered QR SVG markup string (or null/undefined when unavailable). */
    svg?: string | null;
    /** Optional raw payload label used as accessible fallback context when svg is omitted. */
    value?: string;
    /** Pixel size (width and height) of the square QR box. Defaults to 192. */
    size?: number;
    /** Background color for the QR surface. Defaults to '#ffffff'. */
    bgColor?: string;
    /** Foreground CSS color inherited by currentColor SVGs. */
    fgColor?: string;
    /** Whether to apply inner quiet-zone padding around the QR SVG. Defaults to true. */
    includeMargin?: boolean;
    /** How to render a valid SVG string: inline DOM SVG or <img> data URI. Defaults to 'inline'. */
    renderAs?: 'inline' | 'img';
    /** Accessible label / alt text describing the QR code. */
    ariaLabel?: string;
    /** Optional caption rendered below the QR container. */
    caption?: React.ReactNode;
    /** Optional custom fallback when no valid SVG is available. */
    fallback?: React.ReactNode;
    /** Additional CSS classes for the outer wrapper. */
    className?: string;
    /** Optional test ID for the outer wrapper. */
    'data-testid'?: string;
    /** Optional test ID for the QR box element. */
    qrTestId?: string;
}

const ALLOWED_QR_SVG_TAGS = new Set([
    'svg',
    'rect',
    'path',
    'circle',
    'g',
    'defs',
    'title',
    'desc',
]);

/**
 * Validates and returns sanitized QR SVG markup (strict tag/attribute allowlist), or `null` if unsafe.
 */
export function sanitizeQrSvg(svg: string | null | undefined): string | null {
    if (typeof svg !== 'string') return null;
    const trimmed = svg.trim();
    if (!trimmed.toLowerCase().includes('<svg')) return null;
    if (typeof DOMParser === 'undefined') {
        const isSafeFallback =
            /<svg[\s>]/i.test(trimmed) &&
            !/<(?:script|foreignObject|iframe|object|embed|style|link|meta)\b/i.test(trimmed) &&
            !/\son[a-z]+\s*=/i.test(trimmed) &&
            !/(?:javascript|vbscript|data):/i.test(trimmed);
        return isSafeFallback ? trimmed : null;
    }
    try {
        const doc = new DOMParser().parseFromString(trimmed, 'image/svg+xml');
        if (doc.querySelector('parsererror')) return null;
        const root = doc.documentElement;
        if (!root || root.tagName.toLowerCase() !== 'svg') return null;
        const all = [root, ...Array.from(root.querySelectorAll('*'))];
        for (const el of all) {
            if (!ALLOWED_QR_SVG_TAGS.has(el.tagName.toLowerCase())) return null;
            for (const attr of Array.from(el.attributes)) {
                const name = attr.name.toLowerCase();
                const val = attr.value.trim().toLowerCase();
                if (name.startsWith('on')) return null;
                if (
                    (name === 'href' || name === 'xlink:href' || name === 'src') &&
                    (val.startsWith('javascript:') || val.startsWith('vbscript:') || val.startsWith('data:'))
                ) {
                    return null;
                }
                if (name === 'style' && /(?:javascript:|expression\s*\(|url\s*\()/i.test(val)) {
                    return null;
                }
            }
        }
        return typeof XMLSerializer !== 'undefined'
            ? new XMLSerializer().serializeToString(root)
            : (root.outerHTML || trimmed);
    } catch {
        return null;
    }
}

/**
 * Pure presentational QR display container styled with the app's card and border tokens.
 * Renders backend-generated SVG markup (inline or as an image data URI) with an accessible fallback.
 */
export const QrCodeDisplay: React.FC<QrCodeDisplayProps> = ({
    svg,
    value,
    size = 192,
    bgColor = '#ffffff',
    fgColor,
    includeMargin = true,
    renderAs = 'inline',
    ariaLabel,
    caption,
    fallback,
    className = '',
    'data-testid': testId,
    qrTestId,
}) => {
    const safeSvg = sanitizeQrSvg(svg);
    const hasSvg = Boolean(safeSvg);
    const accessibleLabel =
        ariaLabel ||
        (typeof caption === 'string' && caption.trim() ? caption : undefined) ||
        (value ? `QR Code: ${value}` : 'QR Code');

    return (
        <div
            data-testid={testId}
            className={`flex flex-col items-center justify-center gap-3 ${className}`}
        >
            <div
                data-testid={qrTestId}
                style={{
                    width: size,
                    height: size,
                    backgroundColor: bgColor,
                    color: fgColor,
                }}
                className={`max-w-full rounded-2xl border border-theme-subtle shadow-sm flex items-center justify-center overflow-hidden ${
                    includeMargin ? 'p-3' : 'p-0'
                }`}
            >
                {hasSvg && safeSvg ? (
                    renderAs === 'img' ? (
                        <img
                            src={`data:image/svg+xml;utf8,${encodeURIComponent(safeSvg)}`}
                            alt={accessibleLabel}
                            className="w-full h-full object-contain"
                        />
                    ) : (
                        <div
                            role="img"
                            aria-label={accessibleLabel}
                            className="w-full h-full flex items-center justify-center [&>svg]:w-full [&>svg]:h-full"
                            dangerouslySetInnerHTML={{ __html: safeSvg }}
                        />
                    )
                ) : (
                    fallback ?? (
                        <div
                            role="img"
                            aria-label={accessibleLabel}
                            className="w-full h-full flex items-center justify-center rounded-xl bg-bg-app/60 text-theme-light/40"
                        >
                            <QrCode size={Math.max(24, Math.round(size * 0.36))} aria-hidden="true" />
                        </div>
                    )
                )}
            </div>

            {caption && (
                <div className="text-xs text-theme-light text-center max-w-xs leading-relaxed">
                    {caption}
                </div>
            )}
        </div>
    );
};
