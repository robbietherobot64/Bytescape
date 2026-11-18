import { useState, useEffect, useCallback } from 'react';
import { createPortal } from 'react-dom';
import { cacheInterests } from '../../FeedColumn';
import type { ProfileData } from '../../../utils/api';
import { profileApi, subscriptionApi } from '../../../utils/api';
import { INTEREST_CATEGORIES, MAX_INTERESTS } from '../../../utils/interests';
import InterestStep from '../../InterestStep';

interface Props {
  profile: ProfileData;
  isOwnProfile: boolean;
}



export default function WidgetInterests({ profile, isOwnProfile }: Props) {
  const [interests, setInterests] = useState<string[]>(profile.interests || []);
  const [showPicker, setShowPicker] = useState(false);
  const [saving, setSaving] = useState(false);

  // Sync from parent profile
  useEffect(() => {
    setInterests(profile.interests || []);
  }, [profile.interests]);

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
    } catch {
      setInterests(profile.interests || []);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="widget-interests" style={{ padding: 16 }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 12 }}>
        <span style={{ fontFamily: 'var(--font-retro)', fontSize: 14, color: 'var(--accent)' }}>Interests</span>
        {isOwnProfile && (
          <button
            className="btn-link"
            onClick={() => setShowPicker(true)}
            style={{ fontSize: 12, color: 'var(--text-muted)', padding: 0, cursor: 'pointer' }}
            title="Edit interests"
          >
            ⚙
          </button>
        )}
      </div>
      {interests.length === 0 ? (
        <div>
          <p style={{ color: 'var(--text-muted)', fontSize: 12, margin: '0 0 8px' }}>
            No interests yet.
          </p>
          {isOwnProfile && (
            <button
              className="btn"
              onClick={() => setShowPicker(true)}
              style={{
                background: 'var(--accent)',
                color: '#000',
                border: 'none',
                fontWeight: 600,
                fontSize: 12,
                padding: '6px 12px',
              }}
            >
              Add some!
            </button>
          )}
        </div>
      ) : (
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
          {interests.map((interest, i) => (
            <span
              key={i}
              style={{
                display: 'inline-block',
                padding: '3px 10px',
                background: 'rgba(0, 255, 65, 0.1)',
                border: '1px solid var(--border-dim)',
                borderRadius: 12,
                fontSize: 12,
                color: 'var(--text-primary)',
                fontFamily: 'var(--font-mono)',
              }}
            >
              {getLabel(interest)}
            </span>
          ))}
        </div>
      )}

      {saving && (
        <div style={{ color: 'var(--text-muted)', fontSize: 11, marginTop: 8, fontStyle: 'italic' }}>
          Saving...
        </div>
      )}

      {/* Interest picker modal — portaled to body so position:fixed works */}
      {showPicker && createPortal(
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
        />,
        document.body
      )}
    </div>
  );
}
