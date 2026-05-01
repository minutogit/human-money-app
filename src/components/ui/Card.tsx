// src/components/ui/Card.tsx
import React from 'react';

interface CardProps {
    children: React.ReactNode;
    className?: string;
    header?: React.ReactNode;
    variant?: 'default' | 'glass' | 'accent' | 'none' | 'danger' | 'warning';
    hover?: boolean;
    onClick?: () => void;
}

export function Card({ 
    children, 
    className = "", 
    header,
    variant = 'default', 
    hover = false,
    onClick 
}: CardProps) {
    const baseStyles = "rounded-3xl transition-all duration-300 overflow-hidden";
    
    const variantStyles = {
        default: "bg-white shadow-premium border border-theme-subtle/50",
        glass: "bg-white/70 backdrop-blur-md shadow-premium border border-white/40",
        accent: "bg-surface-accent shadow-premium border border-theme-accent/20",
        danger: "bg-rose-50/50 backdrop-blur-md shadow-premium border border-rose-200/50",
        warning: "bg-amber-50/50 backdrop-blur-md shadow-premium border border-amber-200/50",
        none: ""
    };

    const hoverStyles = hover 
        ? "hover:shadow-premium-hover hover:-translate-y-0.5 active:scale-[0.99] cursor-pointer" 
        : "";

    return (
        <div 
            className={`${baseStyles} ${variantStyles[variant]} ${hoverStyles} ${className}`}
            onClick={onClick}
        >
            {header && (
                <div className={`px-6 py-4 border-b transition-colors ${
                    variant === 'danger' ? 'border-rose-200/50 bg-rose-100/30' : 
                    variant === 'warning' ? 'border-amber-200/50 bg-amber-100/30' : 
                    'border-theme-subtle/40 bg-slate-50/50'
                }`}>
                    {header}
                </div>
            )}
            <div className={header ? "p-6" : ""}>
                {children}
            </div>
        </div>
    );
}
