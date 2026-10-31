import React from 'react';

export interface SegmentedTabItem<T extends string = string> {
    id: T;
    label: React.ReactNode;
    icon?: React.ReactNode;
    disabled?: boolean;
}

export interface SegmentedTabsProps<T extends string = string> {
    tabs: SegmentedTabItem<T>[];
    activeTab: T;
    onChange: (id: T) => void;
    fullWidth?: boolean;
    className?: string;
}

export function SegmentedTabs<T extends string = string>({
    tabs,
    activeTab,
    onChange,
    fullWidth = false,
    className = '',
}: SegmentedTabsProps<T>) {
    return (
        <div
            role="tablist"
            className={`p-1.5 bg-white/50 backdrop-blur-md rounded-2xl border border-theme-subtle shadow-inner-soft gap-1 ${
                fullWidth ? 'flex flex-wrap sm:flex-nowrap w-full' : 'inline-flex flex-wrap justify-center max-w-full'
            } ${className}`}
        >
            {tabs.map((tab) => {
                const isActive = activeTab === tab.id;
                return (
                    <button
                        key={tab.id}
                        type="button"
                        role="tab"
                        aria-selected={isActive}
                        disabled={tab.disabled}
                        onClick={() => onChange(tab.id)}
                        className={`${
                            fullWidth ? 'flex-1 min-w-[8rem]' : ''
                        } flex items-center justify-center gap-2 px-4 py-2.5 rounded-xl text-xs font-black uppercase tracking-wider transition-all cursor-pointer disabled:cursor-not-allowed disabled:opacity-50 ${
                            isActive
                                ? 'bg-white text-theme-primary shadow-sm'
                                : 'text-theme-light hover:text-theme-secondary hover:bg-white/40'
                        }`}
                    >
                        {tab.icon && <span className="shrink-0">{tab.icon}</span>}
                        <span>{tab.label}</span>
                    </button>
                );
            })}
        </div>
    );
}
