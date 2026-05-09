// src/components/voucher/VoucherFilterBar.tsx
import { useState } from 'react';
import { Filter, ChevronDown, History, X } from "lucide-react";
import { Card } from "../ui/Card";

interface VoucherFilterBarProps {
    statusFilters: string[];
    standardFilters: string[];
    statusCounts: Record<string, number>;
    standardCounts: Record<string, number>;
    activeFilterCount: number;
    toggleStatusFilter: (status: string) => void;
    toggleStandardFilter: (standard: string) => void;
    resetFilters: () => void;
    availableStatuses: string[];
    availableStandards: string[];
}

export function VoucherFilterBar({
    statusFilters,
    standardFilters,
    statusCounts,
    standardCounts,
    activeFilterCount,
    toggleStatusFilter,
    toggleStandardFilter,
    resetFilters,
    availableStatuses,
    availableStandards
}: VoucherFilterBarProps) {
    const [isFiltersExpanded, setIsFiltersExpanded] = useState(false);

    return (
        <Card variant="glass" className="overflow-hidden border-none shadow-premium">
            <button 
                onClick={() => setIsFiltersExpanded(!isFiltersExpanded)}
                className="w-full flex items-center justify-between px-6 py-4 hover:bg-white/40 transition-colors group"
            >
                <div className="flex items-center gap-3">
                    <div className={`p-2 rounded-lg transition-colors ${activeFilterCount > 0 ? 'bg-theme-primary text-white' : 'bg-theme-subtle/50 text-theme-light'}`}>
                        <Filter size={16} />
                    </div>
                    <span className="text-xs font-black text-theme-secondary uppercase tracking-[0.15em]">Filters</span>
                    {activeFilterCount > 0 && (
                        <span className="bg-theme-primary text-white text-[10px] px-2 py-0.5 rounded-full font-bold">
                            {activeFilterCount}
                        </span>
                    )}
                </div>
                <ChevronDown size={18} className={`text-theme-light transition-transform duration-300 ${isFiltersExpanded ? 'rotate-180' : ''}`} />
            </button>

            <div className={`transition-all duration-500 ease-in-out ${isFiltersExpanded ? 'max-h-[500px] p-6 pt-0 opacity-100' : 'max-h-0 opacity-0'}`}>
                <div className="space-y-6 pt-4 border-t border-theme-subtle/30">
                    {/* Status Filter */}
                    <div className="space-y-3">
                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest px-2">By Status</label>
                        <div className="flex flex-wrap gap-2">
                            {availableStatuses.map(status => {
                                const isActive = statusFilters.includes(status);
                                const Icon = History; // Using History as default as per original code
                                return (
                                    <button
                                        key={status}
                                        onClick={() => toggleStatusFilter(status)}
                                        className={`px-4 py-2 rounded-xl text-xs font-bold transition-all border flex items-center gap-2 ${
                                            isActive 
                                            ? 'bg-theme-primary text-white border-theme-primary shadow-md scale-105' 
                                            : 'bg-white text-theme-secondary border-theme-subtle/50 hover:border-theme-primary hover:bg-white shadow-sm'
                                        }`}
                                    >
                                        <Icon size={14} className={isActive ? 'text-white' : 'text-theme-light'} />
                                        <span className="capitalize">{status}</span>
                                        <span className={`text-[10px] px-1.5 py-0.5 rounded-md ${isActive ? 'bg-white/20 text-white' : 'bg-theme-subtle/30 text-theme-light'}`}>
                                            {statusCounts[status] || 0}
                                        </span>
                                    </button>
                                );
                            })}
                        </div>
                    </div>

                    {/* Standard Filter */}
                    <div className="space-y-3">
                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest px-2">By Standard</label>
                        <div className="flex flex-wrap gap-2">
                            {availableStandards.map(standard => {
                                const isActive = standardFilters.includes(standard);
                                return (
                                    <button
                                        key={standard}
                                        onClick={() => toggleStandardFilter(standard)}
                                        className={`px-4 py-2 rounded-xl text-xs font-bold transition-all border flex items-center gap-2 ${
                                            isActive 
                                            ? 'bg-theme-accent text-white border-theme-accent shadow-md scale-105' 
                                            : 'bg-white text-theme-secondary border-theme-subtle/50 hover:border-theme-accent hover:bg-white shadow-sm'
                                        }`}
                                    >
                                        <span className="capitalize">{standard}</span>
                                        <span className={`text-[10px] px-1.5 py-0.5 rounded-md ${isActive ? 'bg-white/20 text-white' : 'bg-theme-subtle/30 text-theme-light'}`}>
                                            {standardCounts[standard] || 0}
                                        </span>
                                    </button>
                                );
                            })}
                        </div>
                    </div>
                    
                    {activeFilterCount > 0 && (
                        <div className="flex justify-end pt-2">
                            <button 
                                onClick={resetFilters}
                                className="text-[10px] font-black text-rose-500 uppercase tracking-widest flex items-center gap-1.5 hover:underline px-2"
                            >
                                <X size={12} />
                                Reset Filters
                            </button>
                        </div>
                    )}
                </div>
            </div>
        </Card>
    );
}
