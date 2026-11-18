import { useState, useEffect, useCallback } from 'react';
import { profileApi, subscriptionApi } from '../../utils/api';
import { cacheInterests } from '../FeedColumn';
import { INTEREST_CATEGORIES, MAX_INTERESTS } from '../../utils/interests';
import InterestStep from '../InterestStep';

interface InterestWidgetProps {
  interests: string[];
  isOwnProfile: boolean;
  onInterestsChange?: (interests: string[]) => void;
}



export default function InterestWidget({ interests: initialInterests, isOwnProfile, onInterestsChange }: InterestWidgetProps) {
  const [interests, setInterests] = useState<string[]>(initialInterests);
  const [showPicker, setShowPicker] = useState(false);
  const [saving, setSaving] = useState(false);

  // Sync from parent
  useEffect(() => {
    setInterests(initialInterests);
  }, [initialInterests]);

  // Listen for tour-driven InterestStep open
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      if (detail?.action === 'openInterests') setShowPicker(true);
    };
    window.addEventListener('bytescape:tourAction', handler);
    return () => window.removeEventListener('bytescape:tourAction', handler);
  }, []);

  const getLabel = useCallback((id: string): string => {
    for (const cat of INTEREST_CATEGORIES) {
      const found = cat.interests.find(i => i.id === id);
      if (found) return `${found.emoji} ${found.label}`;
    }
    return id;
  }, []);

  const handleSave = async (newInterests: string[]) => {
    setSaving(true);
    try {
      const limited = newInterests.slice(0, MAX_INTERESTS);
      // bootstrap() is the single write (sets interests + reconciles sources);
      // it must run even for an empty selection so deselection unsubscribes.
      await subscriptionApi.bootstrap(limited);
      setInterests(limited);
      cacheInterests(limited);
      window.dispatchEvent(new Event('bytescape:interestsUpdated'));
      window.dispatchEvent(new Event('bytescape:refreshFeed'));
      onInterestsChange?.(limited);
    } catch {
      // Revert on error
      setInterests(initialInterests);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      data-tour="interests"
      style={{
        background: 'var(--surface)',
        border: '1px solid var(--border-color)',
        borderRadius: 6,
        padding: 16,
        position: 'relative',
      }}
    >
      {/* Header */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 12 }}>
        <span style={{ fontFamily: 'var(--font-retro)', fontSize: 15, color: 'var(--text-primary)' }}>
          ⭐ Interests
        </span>
        {isOwnProfile && (
          <button
            onClick={() => setShowPicker(true)}
            title="Edit interests"
            style={{
              background: 'none',
              border: 'none',
              color: 'var(--text-muted)',
              cursor: 'pointer',
              fontSize: 16,
              padding: 2,
              lineHeight: 1,
            }}
          >
            ⚙️
          </button>
        )}
      </div>

      {/* Interest tags */}
      {interests.length === 0 ? (
        <p style={{ color: 'var(--text-muted)', fontSize: 12, margin: 0, lineHeight: 1.5 }}>
          {isOwnProfile
            ? 'No interests yet. Click ⚙️ to add some.'
            : 'No interests selected.'}
        </p>
      ) : (
        <div
          style={{
            display: 'flex',
            flexWrap: 'wrap',
            gap: 6,
            maxHeight: interests.length > 8 ? 200 : 'none',
            overflowY: interests.length > 8 ? 'auto' : 'visible',
          }}
        >
          {interests.map(id => (
            <span
              key={id}
              style={{
                display: 'inline-block',
                padding: '4px 8px',
                background: 'rgba(0, 255, 65, 0.08)',
                border: '1px solid rgba(0, 255, 65, 0.2)',
                borderRadius: 4,
                color: 'var(--accent)',
                fontFamily: 'var(--font-mono)',
                fontSize: 11,
                whiteSpace: 'nowrap',
              }}
            >
              {getLabel(id)}
            </span>
          ))}
        </div>
      )}

      {saving && (
        <div style={{ color: 'var(--text-muted)', fontSize: 11, marginTop: 8, fontStyle: 'italic' }}>
          Saving...
        </div>
      )}

      {/* Interest picker modal */}
      {showPicker && (
        <InterestStep
          onComplete={() => {
            setShowPicker(false);
            const stored = localStorage.getItem('bytescape_interests');
            if (stored) {
              try {
                const parsed = JSON.parse(stored);
                if (Array.isArray(parsed)) handleSave(parsed);
              } catch {}
            }
            // If tour is active, advance to next step
            if (localStorage.getItem('onboarding_complete') !== 'true') {
              window.dispatchEvent(new Event('bytescape:tourAdvance'));
            }
          }}
          onSkip={() => {
            setShowPicker(false);
            // If tour is active, advance to next step
            if (localStorage.getItem('onboarding_complete') !== 'true') {
              window.dispatchEvent(new Event('bytescape:tourAdvance'));
            }
          }}
        />
      )}
    </div>
  );
}
