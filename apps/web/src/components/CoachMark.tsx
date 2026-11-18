import { useEffect, useRef, useState } from 'react';

export interface CoachMarkProps {
  targetSelector: string;
  title: string;
  description: string;
  primaryAction: string;
  secondaryAction: string;
  onPrimary: () => void;
  onSecondary: () => void;
  onSkip?: () => void;
}

export default function CoachMark({
  targetSelector,
  title,
  description,
  primaryAction,
  secondaryAction,
  onPrimary,
  onSecondary,
  onSkip,
}: CoachMarkProps) {
  const [targetEl, setTargetEl] = useState<HTMLElement | null>(null);
  const [position, setPosition] = useState<{ top: number; left: number; arrow: 'top' | 'bottom' }>({ top: 0, left: 0, arrow: 'bottom' });
  const tooltipRef = useRef<HTMLDivElement>(null);

  // Wait/retry for target element
  useEffect(() => {
    setTargetEl(null); // clear stale target while searching for new one
    let attempts = 0;
    const maxAttempts = 6; // 3 seconds at 500ms intervals
    let timer: ReturnType<typeof setTimeout> | null = null;
    let cancelled = false;

    const findTarget = () => {
      if (cancelled) return;
      const el = document.querySelector(targetSelector) as HTMLElement | null;
      if (el) {
        setTargetEl(el);
      } else if (attempts < maxAttempts) {
        attempts++;
        timer = setTimeout(findTarget, 500);
      }
    };

    findTarget();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [targetSelector]);

  // Dim the target slightly so the highlight cutout stands out. No clone needed —
  // the original stays in the grid and the cutout border provides the visual highlight.
  useEffect(() => {
    if (!targetEl) return;
    const prevFilter = targetEl.style.filter;
    const prevTransition = targetEl.style.transition;
    targetEl.style.filter = 'brightness(0.85)';
    targetEl.style.transition = 'none';
    return () => {
      targetEl.style.filter = prevFilter;
      targetEl.style.transition = prevTransition;
    };
  }, [targetEl]);

  // Calculate position when target is found
  useEffect(() => {
    if (!targetEl || !tooltipRef.current) return;

    const calculate = () => {
      const rect = targetEl.getBoundingClientRect();
      const tooltip = tooltipRef.current;
      if (!tooltip) return;

      const tooltipRect = tooltip.getBoundingClientRect();
      const padding = 12;
      const gap = 8;

      // Try below first, then above
      let top = rect.bottom + gap;
      let left = rect.left + rect.width / 2 - tooltipRect.width / 2;
      let arrow: 'top' | 'bottom' = 'top';

      // If below would go off screen, try above
      if (top + tooltipRect.height > window.innerHeight - padding) {
        top = rect.top - gap - tooltipRect.height;
        arrow = 'bottom';
      }

      // Clamp horizontal
      if (left < padding) left = padding;
      if (left + tooltipRect.width > window.innerWidth - padding) {
        left = window.innerWidth - padding - tooltipRect.width;
      }

      setPosition({ top: Math.max(padding, top), left, arrow });
    };

    calculate();
    window.addEventListener('resize', calculate);
    window.addEventListener('scroll', calculate, true);
    return () => {
      window.removeEventListener('resize', calculate);
      window.removeEventListener('scroll', calculate, true);
    };
  }, [targetEl]);

  if (!targetEl) {
    return null;
  }

  const rect = targetEl.getBoundingClientRect();

  return (
    <>
      {/* Dark overlay — does NOT dismiss on click (stays until button click) */}
      <div
        style={{
          position: 'fixed',
          inset: 0,
          background: 'rgba(0, 0, 0, 0.6)',
          zIndex: 9998,
          pointerEvents: 'none',
        }}
      />

      {/* Highlight cutout around target */}
      <div
        style={{
          position: 'fixed',
          top: rect.top - 4,
          left: rect.left - 4,
          width: rect.width + 8,
          height: rect.height + 8,
          border: '2px solid var(--accent)',
          borderRadius: 6,
          boxShadow: '0 0 16px var(--accent), 0 0 32px rgba(0, 255, 65, 0.2)',
          zIndex: 9999,
          pointerEvents: 'none',
          transition: 'all 0.3s ease',
        }}
      />

      {/* Tooltip */}
      <div
        ref={tooltipRef}
        style={{
          position: 'fixed',
          top: position.top,
          left: position.left,
          zIndex: 10000,
          pointerEvents: 'all',
          background: 'var(--bg-secondary, #1a1a2e)',
          border: '1px solid var(--accent)',
          borderRadius: 8,
          padding: '16px 20px',
          maxWidth: 320,
          minWidth: 260,
          boxShadow: '0 4px 24px rgba(0, 0, 0, 0.5), 0 0 8px var(--accent-dim)',
          fontFamily: 'var(--font-mono)',
          fontSize: 13,
        }}
      >
        {/* Arrow indicator */}
        <div style={{
          position: 'absolute',
          [position.arrow === 'top' ? 'top' : 'bottom']: -6,
          left: '50%',
          transform: 'translateX(-50%) rotate(45deg)',
          width: 10,
          height: 10,
          background: 'var(--bg-secondary, #1a1a2e)',
          [position.arrow === 'top' ? 'borderTop' : 'borderBottom']: '1px solid var(--accent)',
          [position.arrow === 'top' ? 'borderLeft' : 'borderRight']: '1px solid var(--accent)',
        }} />

        <div style={{ fontFamily: 'var(--font-retro)', fontSize: 16, color: 'var(--accent)', marginBottom: 8 }}>
          {title}
        </div>
        <p style={{ color: 'var(--text-muted)', margin: '0 0 16px', lineHeight: 1.5 }}>
          {description}
        </p>

        <div style={{ display: 'flex', gap: 8 }}>
          <button
            className="btn"
            onClick={onPrimary}
            style={{
              background: 'var(--accent)',
              color: '#000',
              border: 'none',
              fontWeight: 600,
              fontSize: 12,
              padding: '8px 14px',
            }}
          >
            {primaryAction}
          </button>
          <button
            className="btn"
            onClick={onSecondary}
            style={{
              background: 'transparent',
              border: '1px solid var(--text-muted)',
              color: 'var(--text-muted)',
              fontSize: 12,
              padding: '8px 14px',
            }}
          >
            {secondaryAction}
          </button>
        </div>

        {onSkip && (
          <div style={{ marginTop: 12, borderTop: '1px solid var(--border-color)', paddingTop: 10 }}>
            <button
              className="btn-link"
              onClick={onSkip}
              style={{ color: 'var(--text-muted)', fontSize: 12, padding: 0 }}
            >
              Skip tutorial →
            </button>
          </div>
        )}
      </div>
    </>
  );
}
