"use client";

import { motion } from "framer-motion";
import type { MockupVariant } from "@/lib/types";

const variantLabels: Record<string, { title: string; subtitle: string }> = {
  A: { title: "Variant A", subtitle: "Minimal" },
  B: { title: "Variant B", subtitle: "Colorful" },
  C: { title: "Variant C", subtitle: "Dark Neon" },
};

interface MockupCardsProps {
  mockups: Record<string, MockupVariant>;
  canSelect: boolean;
  onSelect: (id: string) => void;
  loading: boolean;
}

export function MockupCards({ mockups, canSelect, onSelect, loading }: MockupCardsProps) {
  const ids = Object.keys(mockups);
  if (ids.length === 0) return null;

  return (
    <div className="grid grid-cols-3 gap-4">
      {ids.map((id, i) => {
        const mockup = mockups[id];
        const labels = variantLabels[id] ?? { title: id, subtitle: "" };

        return (
          <motion.div
            key={id}
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: i * 0.1, duration: 0.4, ease: [0.25, 0.46, 0.45, 0.94] }}
            className="group relative flex flex-col overflow-hidden rounded-xl border border-border-primary bg-bg-secondary transition-colors hover:border-border-hover"
          >
            {/* Image */}
            <div className="relative aspect-[4/3] overflow-hidden bg-bg-tertiary">
              {mockup.imageBase64 ? (
                <img
                  src={`data:image/jpeg;base64,${mockup.imageBase64}`}
                  alt={labels.title}
                  className="h-full w-full object-cover transition-transform duration-500 group-hover:scale-105"
                />
              ) : (
                <div className="flex h-full items-center justify-center">
                  <div className="flex flex-col items-center gap-2 text-text-tertiary">
                    <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                      <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
                      <circle cx="8.5" cy="8.5" r="1.5" />
                      <polyline points="21 15 16 10 5 21" />
                    </svg>
                    <span className="text-xs">Text-based mockup</span>
                  </div>
                </div>
              )}

              {/* Design Link overlay */}
              {mockup.designLink && (
                <a
                  href={mockup.designLink}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="absolute right-2 top-2 flex items-center gap-1 rounded-md bg-bg-primary/80 px-2 py-1 text-[10px] font-medium text-accent-hover backdrop-blur-sm transition-colors hover:bg-bg-primary"
                >
                  View Design
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                    <polyline points="15 3 21 3 21 9" />
                    <line x1="10" y1="14" x2="21" y2="3" />
                  </svg>
                </a>
              )}
            </div>

            {/* Content */}
            <div className="flex flex-1 flex-col gap-2 p-3">
              <div>
                <h3 className="text-sm font-medium text-text-primary">{labels.title}</h3>
                <p className="text-xs text-text-tertiary">{labels.subtitle}</p>
              </div>

              {mockup.text && (
                <p className="line-clamp-3 text-xs leading-relaxed text-text-secondary">
                  {mockup.text}
                </p>
              )}

              {canSelect && (
                <motion.button
                  whileHover={{ scale: 1.02 }}
                  whileTap={{ scale: 0.98 }}
                  disabled={loading}
                  onClick={() => onSelect(id)}
                  className="mt-auto rounded-lg border border-accent/30 bg-accent-muted px-3 py-2 text-xs font-medium text-accent-hover transition-colors hover:bg-accent/20 disabled:opacity-40"
                >
                  Select {labels.title}
                </motion.button>
              )}
            </div>
          </motion.div>
        );
      })}
    </div>
  );
}
