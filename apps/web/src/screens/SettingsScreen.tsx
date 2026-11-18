import { useState, useEffect, useCallback, type ReactElement } from 'react';
import {
  settingsApi,
  muteApi,
  apiErrorDisplay,
  applyAppTheme,
  APP_THEMES,
  PRESET_FX,
  PRESET_METRICS,
  FONT_OPTIONS,
  THEME_TOKEN_REGISTRY,
  readPresetEffectiveValues,
  normalizeAppTheme,
  loadThemeOverrides,
  saveThemeOverrides,
  clearThemeOverrides,
  applyThemeOverrides,
  type ThemeOverrides,
  type AppThemeId,
  type NsfwPref,
  type FeedSort,
  type SourceMix,
  type SettingsState,
  type MutedAccount,
  type VisibilitySettings,
} from '../utils/api';
import { INTEREST_CATEGORIES, MAX_INTERESTS } from '../utils/interests';
import { cacheInterests } from '../components/FeedColumn';

type SectionId = 'appearance' | 'moderation' | 'interests' | 'feed';

/** Display labels for the Customize panel's token groups (registry order). */
const TOKEN_GROUP_LABELS: Record<string, string> = {
  colors: 'Colors',
  type: 'Font',
  metrics: 'Metrics',
  effects: 'Effects',
  image: 'Background image',
};

const SECTIONS: Array<{ id: SectionId; label: string; icon: string }> = [
  { id: 'appearance', label: 'Appearance', icon: '🎨' },
  { id: 'moderation', label: 'Moderation & content', icon: '🛡️' },
  { id: 'interests', label: 'Interests', icon: '⭐' },
  { id: 'feed', label: 'Feed & sources', icon: '📰' },
];

const NSFW_OPTIONS: Array<{ value: NsfwPref; label: string; desc: string }> = [
  { value: 'exclude', label: 'Exclude', desc: 'Hide NSFW content entirely' },
  { value: 'blur', label: 'Blur', desc: 'Show NSFW blurred behind a reveal' },
  { value: 'show', label: 'Show', desc: 'Show NSFW content normally' },
];

export default function SettingsScreen() {
  const [active, setActive] = useState<SectionId>('appearance');
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState('');

  // Appearance
  const [theme, setTheme] = useState<AppThemeId>('default');
  const [overrides, setOverrides] = useState<ThemeOverrides>(() => loadThemeOverrides());
  // Effective = override ?? active-preset value. Re-derived whenever the
  // theme or overrides change so the controls always display the EFFECTIVE
  // state (the preset's shipped values when the user has not overridden).
  const [presetVals, setPresetVals] = useState(() => readPresetEffectiveValues());
  const rederivePreset = useCallback(() => setPresetVals(readPresetEffectiveValues()), []);
  useEffect(() => { rederivePreset(); }, [theme, overrides, rederivePreset]);
  // Effective = override ?? active-preset value, for EVERY registry token.
  // Preset defaults resolve per theme: colors/font from the live computed
  // tokens (presetVals), sliders from PRESET_METRICS, toggles from PRESET_FX.
  const effective: Record<string, string | number | boolean> = (() => {
    const out: Record<string, string | number | boolean> = {};
    const ov = overrides as Record<string, unknown>;
    const pv = presetVals as unknown as Record<string, string>;
    const metrics = PRESET_METRICS[theme] as unknown as Record<string, number>;
    const fx = PRESET_FX[theme] as unknown as Record<string, boolean>;
    for (const spec of THEME_TOKEN_REGISTRY) {
      const k = spec.key as string;
      const v = ov[k];
      if (v !== undefined && v !== null) { out[k] = v as string | number | boolean; continue; }
      if (spec.kind === 'color') out[k] = pv[k] ?? '#000000';
      else if (spec.kind === 'font') out[k] = presetVals.fontFamily;
      else if (spec.kind === 'toggle') out[k] = fx[k] ?? false;
      else if (k === 'fontScale') out[k] = 1;
      else if (spec.kind === 'slider') out[k] = metrics[k] ?? 0;
    }
    return out;
  })();

  // Moderation
  const [nsfwPref, setNsfwPref] = useState<NsfwPref>('exclude');
  const [muted, setMuted] = useState<MutedAccount[]>([]);
  const [loadingMuted, setLoadingMuted] = useState(false);
  const [manuallyApprovesFollowers, setManuallyApprovesFollowers] = useState(false);
  // Visibility (identity-graph Phase 6).
  const [graphVisibility, setGraphVisibility] = useState<'public' | 'private'>('public');
  const [homeState, setHomeState] = useState<VisibilitySettings['home'] | null>(null);
  const [visibilityCaveat, setVisibilityCaveat] = useState('');

  // Interests
  const [interests, setInterests] = useState<string[]>([]);
  const [expandedCat, setExpandedCat] = useState<string | null>(INTEREST_CATEGORIES[0]?.id || null);

  // Feed & sources
  const [sort, setSort] = useState<FeedSort>('new');
  const [sourceMix, setSourceMix] = useState<SourceMix>('all');

  // Per-section save status
  const [saving, setSaving] = useState<Record<SectionId, boolean>>({
    appearance: false, moderation: false, interests: false, feed: false,
  });
  const [sectionError, setSectionError] = useState<Record<SectionId, string>>({
    appearance: '', moderation: '', interests: '', feed: '',
  });

  const loadMuted = useCallback(async () => {
    setLoadingMuted(true);
    try {
      const res = await muteApi.list();
      setMuted(res.accounts || []);
    } catch {
      setMuted([]);
    } finally {
      setLoadingMuted(false);
    }
  }, []);

  // Load the settings blob once (account-stored source of truth) and hydrate
  // each section. Interests also mirror through the existing localStorage cache.
  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const s: SettingsState = await settingsApi.get();
        if (!alive) return;
        setTheme(normalizeAppTheme(s.theme));
        setNsfwPref(s.nsfw_pref ?? 'exclude');
        setManuallyApprovesFollowers(Boolean(s.manually_approves_followers));
        setInterests(Array.isArray(s.interests) ? s.interests : []);
        setSort(s.sort === 'popular' ? 'popular' : 'new');
        setSourceMix(s.source_mix === 'bluesky' || s.source_mix === 'mastodon' ? s.source_mix : 'all');
      } catch (e) {
        if (alive) setLoadError(apiErrorDisplay(e));
      } finally {
        if (alive) setLoading(false);
      }
    })();
    loadMuted();
    settingsApi.visibility.get().then((v) => {
      if (!alive) return;
      setGraphVisibility(v.graph_visibility === 'private' ? 'private' : 'public');
      setHomeState(v.home);
      setVisibilityCaveat(v.caveat);
    }).catch(() => {});
    return () => { alive = false; };
  }, [loadMuted]);

  const setErr = (id: SectionId, msg: string) =>
    setSectionError((prev) => ({ ...prev, [id]: msg }));

  // ── Appearance ──
  const saveAppearance = async (next: AppThemeId) => {
    // Optimistic live apply; roll back to the last saved value on failure.
    const prev = theme;
    setTheme(next);
    applyAppTheme(next);
    window.dispatchEvent(new CustomEvent('bytescape:appThemeChanged', { detail: { theme: next } }));
    setSaving((s) => ({ ...s, appearance: true }));
    setErr('appearance', '');
    try {
      await settingsApi.updateAppearance(next);
    } catch (e) {
      setTheme(prev);
      applyAppTheme(prev);
      window.dispatchEvent(new CustomEvent('bytescape:appThemeChanged', { detail: { theme: prev } }));
      setErr('appearance', apiErrorDisplay(e));
    } finally {
      setSaving((s) => ({ ...s, appearance: false }));
    }
  };

  // ── Theme customization (user overrides layered on the preset) ──
  // Live-applied as the user tweaks; persisted to localStorage (no backend
  // JSON field for freeform visual overrides — no cross-device sync).
  const updateOverride = (patch: Partial<ThemeOverrides>) => {
    setOverrides((prev) => {
      const next = { ...prev, ...patch };
      // Drop undefined keys so reset-to-preset removes the inline var.
      for (const k of Object.keys(next) as Array<keyof ThemeOverrides>) {
        if (next[k] === undefined) delete next[k];
      }
      saveThemeOverrides(next);
      applyThemeOverrides(next);
      return next;
    });
  };

  const resetOverrideGroup = (keys: Array<keyof ThemeOverrides>) => {
    setOverrides((prev) => {
      const next = { ...prev };
      for (const k of keys) delete next[k];
      saveThemeOverrides(next);
      applyThemeOverrides(next);
      return next;
    });
  };

  const resetAllOverrides = () => {
    clearThemeOverrides();
    applyThemeOverrides({});
    setOverrides({});
  };

  // Apply stored overrides on mount (survives reload).
  useEffect(() => {
    applyThemeOverrides(loadThemeOverrides());
  }, []);

  const handleBgImage = (file: File | null) => {
    if (!file) return;
    if (file.size > 2 * 1024 * 1024) {
      setErr('appearance', 'Background image must be under 2MB.');
      return;
    }
    const reader = new FileReader();
    reader.onload = () => {
      const url = String(reader.result);
      updateOverride({ bgImage: url });
    };
    reader.readAsDataURL(file);
  };

  // ── Moderation ──
  const saveModeration = async (next: NsfwPref) => {
    const prev = nsfwPref;
    setNsfwPref(next);
    setSaving((s) => ({ ...s, moderation: true }));
    setErr('moderation', '');
    try {
      await settingsApi.updateModeration(next);
    } catch (e) {
      setNsfwPref(prev);
      setErr('moderation', apiErrorDisplay(e));
    } finally {
      setSaving((s) => ({ ...s, moderation: false }));
    }
  };

  const savePrivacy = async (locked: boolean) => {
    const prev = manuallyApprovesFollowers;
    setManuallyApprovesFollowers(locked);
    setSaving((s) => ({ ...s, moderation: true }));
    setErr('moderation', '');
    try {
      await settingsApi.updatePrivacy(locked);
    } catch (e) {
      setManuallyApprovesFollowers(prev);
      setErr('moderation', apiErrorDisplay(e));
    } finally {
      setSaving((s) => ({ ...s, moderation: false }));
    }
  };

  const removeMuted = async (ma: MutedAccount) => {
    if (!ma.muted_did) return;
    try {
      await muteApi.unmute(ma.muted_did);
      setMuted((prev) => prev.filter((a) => a.id !== ma.id));
      window.dispatchEvent(new CustomEvent('bytescape:refreshFeed'));
    } catch (e) {
      setErr('moderation', apiErrorDisplay(e));
    }
  };

  // ── Interests ──
  const toggleInterest = (id: string) => {
    setInterests((prev) =>
      prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id].slice(0, MAX_INTERESTS),
    );
    setErr('interests', '');
  };

  const saveInterests = async () => {
    const next = interests;
    setSaving((s) => ({ ...s, interests: true }));
    setErr('interests', '');
    try {
      await settingsApi.updateInterests(next);
      cacheInterests(next);
    } catch (e) {
      setErr('interests', apiErrorDisplay(e));
    } finally {
      setSaving((s) => ({ ...s, interests: false }));
    }
  };

  // ── Feed & sources ──
  const saveFeed = async (patch: { sort?: FeedSort; source_mix?: SourceMix }) => {
    const merged = { sort, source_mix: sourceMix, ...patch };
    if (patch.sort) setSort(patch.sort);
    if (patch.source_mix) setSourceMix(patch.source_mix);
    if (patch.sort) localStorage.setItem('bytescape_feed_sort', patch.sort);
    setSaving((s) => ({ ...s, feed: true }));
    setErr('feed', '');
    try {
      await settingsApi.updateFeed(patch);
    } catch (e) {
      if (patch.sort) setSort(sort);
      if (patch.source_mix) setSourceMix(sourceMix);
      setErr('feed', apiErrorDisplay(e));
    } finally {
      setSaving((s) => ({ ...s, feed: false }));
    }
  };

  // ── Section bodies (shared across all layout modes) ──
  const renderAppearance = () => (
    <div className="settings-section-body">
      <p style={{ color: 'var(--text-muted)', fontSize: 13, marginBottom: 16 }}>
        Choose how Bytescape looks. Themes restyle the whole app and are saved to your account.
      </p>
      <div className="settings-theme-grid">
        {APP_THEMES.map((t) => (
          <button
            key={t.id}
            className={`settings-theme-card${theme === t.id ? ' selected' : ''}`}
            onClick={() => saveAppearance(t.id)}
            disabled={saving.appearance}
          >
            <div className="settings-theme-swatch">
              {t.swatch.map((c, i) => (
                <span key={i} style={{ background: c }} />
              ))}
            </div>
            <span className="settings-theme-label">{t.label}</span>
          </button>
        ))}
      </div>

      {/* ── Theme customization: user overrides layered on the preset ── */}
      <div className="settings-custom">
        <div className="settings-custom-head">
          <h3 className="settings-subhead" style={{ marginBottom: 0 }}>Customize</h3>
          <button className="btn btn-small" onClick={resetAllOverrides}>
            Reset to preset
          </button>
        </div>
        <p style={{ color: 'var(--text-muted)', fontSize: 12, marginBottom: 12 }}>
          Tweak the preset. Changes apply live. Saved on this device only (no cross-device sync).
        </p>

        {/* Registry-generated controls — EVERY theme token has a control.
            Effective = override ?? active preset; ↺ resets to preset. */}
        {(['colors', 'type', 'metrics', 'effects', 'image'] as const).map((group) => (
          <div key={group}>
            <h4 className="settings-subhead" style={{ marginTop: 16 }}>{TOKEN_GROUP_LABELS[group]}</h4>
            {THEME_TOKEN_REGISTRY.filter((spec) => spec.group === group).map((spec) => {
              const k = spec.key as string;
              const value = effective[k];
              const isSet = (overrides as Record<string, unknown>)[k] !== undefined;
              return (
                <div className="settings-custom-row" key={k}>
                  <label>{spec.label}</label>
                  {spec.kind === 'color' && (
                    <input
                      type="color"
                      value={String(value)}
                      onChange={(e) => updateOverride({ [k]: e.target.value } as Partial<ThemeOverrides>)}
                    />
                  )}
                  {spec.kind === 'font' && (
                    <select
                      value={String(value)}
                      onChange={(e) => updateOverride({ [k]: e.target.value } as Partial<ThemeOverrides>)}
                    >
                      {FONT_OPTIONS.map((f) => (
                        <option key={f.value} value={f.value}>{f.label}</option>
                      ))}
                    </select>
                  )}
                  {spec.kind === 'slider' && (
                    <>
                      <input
                        type="range"
                        min={spec.min}
                        max={spec.max}
                        step={spec.step}
                        value={Math.round(Number(value) * (spec.scale ?? 1))}
                        onChange={(e) => updateOverride({ [k]: Number(e.target.value) / (spec.scale ?? 1) } as Partial<ThemeOverrides>)}
                      />
                      <span className="settings-custom-val">
                        {Math.round(Number(value) * (spec.scale ?? 1))}{spec.unit ?? ''}
                      </span>
                    </>
                  )}
                  {spec.kind === 'toggle' && (
                    <button
                      className={`settings-toggle${value ? ' on' : ''}`}
                      onClick={() => updateOverride({ [k]: !value } as Partial<ThemeOverrides>)}
                    >
                      {value ? 'On' : 'Off'}
                    </button>
                  )}
                  {spec.kind === 'file' && (
                    <input
                      type="file"
                      accept="image/*"
                      onChange={(e) => handleBgImage(e.target.files?.[0] ?? null)}
                    />
                  )}
                  <button
                    className="btn btn-small"
                    disabled={!isSet}
                    title={isSet ? 'Reset to preset' : 'Using preset value'}
                    onClick={() => resetOverrideGroup([spec.key])}
                  >
                    ↺
                  </button>
                </div>
              );
            })}
          </div>
        ))}
      </div>

      {sectionError.appearance && <div className="error-text" style={{ marginTop: 10 }}>{sectionError.appearance}</div>}
    </div>
  );

  const saveGraphVisibility = async (next: 'public' | 'private') => {
    const prev = graphVisibility;
    setGraphVisibility(next);
    setSaving((s) => ({ ...s, moderation: true }));
    setErr('moderation', '');
    try {
      await settingsApi.visibility.patch({ graph_visibility: next });
    } catch (e) {
      setGraphVisibility(prev);
      setErr('moderation', apiErrorDisplay(e));
    } finally {
      setSaving((s) => ({ ...s, moderation: false }));
    }
  };

  const saveHomeHide = async (hide: boolean) => {
    setSaving((s) => ({ ...s, moderation: true }));
    setErr('moderation', '');
    try {
      const res = await settingsApi.visibility.patch({ hide_collections: hide });
      setHomeState((h) => (h ? { ...h, hide_collections: res.home_hide_collections } : h));
    } catch (e) {
      // Stale grants fail with the instruction-focused re-link message —
      // surface it (the home instance's answer is authoritative).
      setErr('moderation', apiErrorDisplay(e));
    } finally {
      setSaving((s) => ({ ...s, moderation: false }));
    }
  };

  const renderModeration = () => (
    <div className="settings-section-body">
      <h3 className="settings-subhead">NSFW content</h3>
      <div className="settings-segmented" role="radiogroup" aria-label="NSFW preference">
        {NSFW_OPTIONS.map((opt) => (
          <button
            key={opt.value}
            role="radio"
            aria-checked={nsfwPref === opt.value}
            className={`settings-segment${nsfwPref === opt.value ? ' active' : ''}`}
            onClick={() => saveModeration(opt.value)}
            disabled={saving.moderation}
            title={opt.desc}
          >
            {opt.label}
          </button>
        ))}
      </div>
      <p style={{ color: 'var(--text-muted)', fontSize: 12, marginTop: 8 }}>
        {NSFW_OPTIONS.find((o) => o.value === nsfwPref)?.desc}
      </p>
      {sectionError.moderation && <div className="error-text" style={{ marginTop: 10 }}>{sectionError.moderation}</div>}

      <h3 className="settings-subhead" style={{ marginTop: 28 }}>Follow approvals</h3>
      <p style={{ color: 'var(--text-muted)', fontSize: 13, marginBottom: 12 }}>
        New follows require your approval by default — every account starts locked, and your
        choice here is saved per account from the moment you change it. On the fediverse this is
        enforced (your actor advertises <code>manuallyApprovesFollowers</code>); on Bluesky the
        AT Protocol has no locked accounts, so requests there are UI-only and not enforceable on
        the network.
      </p>
      <div className="settings-segmented" role="radiogroup" aria-label="Follow approval">
        <button
          role="radio"
          aria-checked={!manuallyApprovesFollowers}
          className={`settings-segment${!manuallyApprovesFollowers ? ' active' : ''}`}
          onClick={() => savePrivacy(false)}
          disabled={saving.moderation}
        >
          Approve followers automatically
        </button>
        <button
          role="radio"
          aria-checked={manuallyApprovesFollowers}
          className={`settings-segment${manuallyApprovesFollowers ? ' active' : ''}`}
          onClick={() => savePrivacy(true)}
          disabled={saving.moderation}
        >
          Review follow requests
        </button>
      </div>

      <h3 className="settings-subhead" style={{ marginTop: 28 }}>Follow list visibility</h3>
      <p style={{ color: 'var(--text-muted)', fontSize: 13, marginBottom: 12 }}>
        Who can see who you follow and who follows you. On Bytescape (the fediverse) this is
        enforced on your actor's collections. {visibilityCaveat}
        {homeState?.linked && (
          <>
            {' '}Your follow lists on <b>{homeState.instance}</b> are managed there (
            {homeState.hide_collections ? 'currently hidden' : homeState.hide_collections === false ? 'currently visible' : 'state unknown'}
            {homeState.settings_url && (
              <>
                {' — '}
                <a href={homeState.settings_url} target="_blank" rel="noreferrer" style={{ color: 'var(--accent)' }}>
                  manage on {homeState.instance}
                </a>
              </>
            )}
            ). Changes made here are sent to {homeState.instance} and its answer is authoritative.
          </>
        )}
      </p>
      <div className="settings-segmented" role="radiogroup" aria-label="Follow list visibility">
        <button
          role="radio"
          aria-checked={graphVisibility === 'public'}
          className={`settings-segment${graphVisibility === 'public' ? ' active' : ''}`}
          onClick={() => saveGraphVisibility('public')}
          disabled={saving.moderation}
        >
          Public follow lists
        </button>
        <button
          role="radio"
          aria-checked={graphVisibility === 'private'}
          className={`settings-segment${graphVisibility === 'private' ? ' active' : ''}`}
          onClick={() => saveGraphVisibility('private')}
          disabled={saving.moderation}
        >
          Hide my follow lists
        </button>
      </div>
      {homeState?.linked && (
        <div className="settings-segmented" role="radiogroup" aria-label="Home instance visibility" style={{ marginTop: 10 }}>
          <button
            role="radio"
            aria-checked={homeState.hide_collections === false}
            className={`settings-segment${homeState.hide_collections === false ? ' active' : ''}`}
            onClick={() => saveHomeHide(false)}
            disabled={saving.moderation || !homeState.can_proxy}
          >
            Visible on {homeState.instance}
          </button>
          <button
            role="radio"
            aria-checked={homeState.hide_collections === true}
            className={`settings-segment${homeState.hide_collections === true ? ' active' : ''}`}
            onClick={() => saveHomeHide(true)}
            disabled={saving.moderation || !homeState.can_proxy}
          >
            Hidden on {homeState.instance}
          </button>
        </div>
      )}
      {homeState?.linked && !homeState.can_proxy && (
        <p style={{ color: 'var(--text-muted)', fontSize: 12, marginTop: 8 }}>
          Managing the {homeState.instance} setting from here needs a re-link with profile
          settings permission — follow the prompt when you change it, or manage it on{' '}
          {homeState.instance} directly.
        </p>
      )}

      <h3 className="settings-subhead" style={{ marginTop: 28 }}>Muted & blocked accounts</h3>
      {loadingMuted ? (
        <p style={{ color: 'var(--text-muted)', fontSize: 12 }}>Loading...</p>
      ) : muted.length === 0 ? (
        <p style={{ color: 'var(--text-muted)', fontSize: 12 }}>No muted or blocked accounts.</p>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          {muted.map((ma) => (
            <div key={ma.id} className="settings-muted-row">
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 13, color: 'var(--text-primary)', fontWeight: 500, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {ma.muted_handle || ma.muted_did || 'Unknown'}
                </div>
                <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>
                  {ma.muted_type === 'block' ? 'Blocked' : 'Muted'} · {new Date(ma.muted_at).toLocaleDateString()}
                </div>
              </div>
              <button className="btn" onClick={() => removeMuted(ma)} style={{ fontSize: 11, padding: '4px 10px' }}>
                {ma.muted_type === 'block' ? 'Unblock' : 'Unmute'}
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );

  const renderInterests = () => (
    <div className="settings-section-body">
      <p style={{ color: 'var(--text-muted)', fontSize: 13, marginBottom: 16 }}>
        Pick the topics you want in your feed. Changes apply across your account and devices.
      </p>
      <div>
        {INTEREST_CATEGORIES.map((cat) => (
          <div key={cat.id} style={{ marginBottom: 4 }}>
            <div
              onClick={() => setExpandedCat(expandedCat === cat.id ? null : cat.id)}
              className="settings-interest-cat"
            >
              <span style={{ fontFamily: 'var(--font-retro)', fontSize: 14, flex: 1 }}>{cat.label}</span>
              <span style={{ color: 'var(--text-muted)', fontSize: 12 }}>
                {interests.filter((id) => cat.interests.some((i) => i.id === id)).length}/{cat.interests.length}
              </span>
              <span style={{ color: 'var(--text-muted)', fontSize: 10, transform: expandedCat === cat.id ? 'rotate(90deg)' : 'none', transition: 'transform 0.2s' }}>▶</span>
            </div>
            {expandedCat === cat.id && (
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, padding: '8px 12px' }}>
                {cat.interests.map((interest) => {
                  const isSelected = interests.includes(interest.id);
                  return (
                    <span
                      key={interest.id}
                      onClick={() => toggleInterest(interest.id)}
                      className={`chip ${isSelected ? 'selected' : ''}`}
                      style={{ cursor: 'pointer' }}
                    >
                      {interest.emoji} {interest.label}
                    </span>
                  );
                })}
              </div>
            )}
          </div>
        ))}
      </div>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginTop: 16, borderTop: '1px solid var(--border-dim)', paddingTop: 12 }}>
        <span style={{ color: 'var(--text-muted)', fontSize: 12 }}>{interests.length} of {MAX_INTERESTS} selected</span>
        <button className="btn" onClick={saveInterests} disabled={saving.interests}>
          {saving.interests ? 'Saving...' : 'Save interests'}
        </button>
      </div>
      {sectionError.interests && <div className="error-text" style={{ marginTop: 10 }}>{sectionError.interests}</div>}
    </div>
  );

  const renderFeed = () => (
    <div className="settings-section-body">
      <h3 className="settings-subhead">Default sort</h3>
      <div className="settings-segmented" role="radiogroup" aria-label="Default feed sort">
        {([['new', 'New'], ['popular', 'Popular']] as Array<[FeedSort, string]>).map(([value, label]) => (
          <button
            key={value}
            role="radio"
            aria-checked={sort === value}
            className={`settings-segment${sort === value ? ' active' : ''}`}
            onClick={() => saveFeed({ sort: value })}
            disabled={saving.feed}
          >
            {label}
          </button>
        ))}
      </div>

      <h3 className="settings-subhead" style={{ marginTop: 28 }}>Source mix</h3>
      <div className="settings-segmented" role="radiogroup" aria-label="Source mix">
        {([['all', 'All'], ['bluesky', 'Bluesky'], ['mastodon', 'Mastodon']] as Array<[SourceMix, string]>).map(([value, label]) => (
          <button
            key={value}
            role="radio"
            aria-checked={sourceMix === value}
            className={`settings-segment${sourceMix === value ? ' active' : ''}`}
            onClick={() => saveFeed({ source_mix: value })}
            disabled={saving.feed}
          >
            {label}
          </button>
        ))}
      </div>
      <p style={{ color: 'var(--text-muted)', fontSize: 12, marginTop: 8 }}>
        Controls which sources feed your stream by default.
      </p>
      {sectionError.feed && <div className="error-text" style={{ marginTop: 10 }}>{sectionError.feed}</div>}
    </div>
  );

  const sectionBody: Record<SectionId, () => ReactElement> = {
    appearance: renderAppearance,
    moderation: renderModeration,
    interests: renderInterests,
    feed: renderFeed,
  };

  if (loading) {
    return <div className="screen settings-screen"><div className="settings-body"><div className="empty-feed"><p>Loading...</p></div></div></div>;
  }

  return (
    <div className="screen settings-screen">
      {/* Top horizontal tab bar — sticky, scrollable if 4 tabs don't fit. */}
      <div className="settings-topbar">
        <div className="settings-topbar-inner">
          {SECTIONS.map((s) => (
            <button
              key={s.id}
              className={`settings-top-tab${active === s.id ? ' active' : ''}`}
              onClick={() => setActive(s.id)}
            >
              <span className="settings-top-tab-icon">{s.icon}</span>
              <span className="settings-top-tab-label">{s.label}</span>
            </button>
          ))}
        </div>
      </div>

      <div className="settings-body">
        {loadError && <div className="error-text" style={{ margin: '8px 0' }}>{loadError}</div>}
        <h2 className="section-title">{SECTIONS.find((s) => s.id === active)?.label}</h2>
        {sectionBody[active]()}
      </div>
    </div>
  );
}
