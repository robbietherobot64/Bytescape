import { useState, useEffect, useCallback } from 'react';
import { cacheInterests } from '../components/FeedColumn';
import { reactionErrorToastMessage, relinkToast } from '../components/PostCard';
import {
  profileApi,
  type ProfileData,
  type UpdateProfileRequest,
  type Widget,
  type WidgetTheme,
} from '../utils/api';
import { type NavItem } from '../components/profile/ProfileNav';
import WidgetFollowers from '../components/profile/widgets/WidgetFollowers';
import ProfileGrid from '../components/profile/ProfileGrid';
import ProfileToolbar from '../components/profile/ProfileToolbar';
import WidgetThemeEditor from '../components/profile/WidgetThemeEditor';
import type { WidgetType } from '../components/profile/widgetTypes';
import ImportStep from '../components/ImportStep';
import { createPortal } from 'react-dom';

interface Props {
  username?: string;
  onViewProfile?: (username: string) => void;
}

const DEFAULT_LAYOUT: Widget[] = [
  { id: 'header', widget_type: 'profile_header', x: 0, y: 0, w: 20, h: 15, data: {}, theme_id: 'default' },
  { id: 'feed', widget_type: 'posts', x: 0, y: 15, w: 20, h: 50, data: {}, theme_id: 'default' },
];

const DEFAULT_NAV_ITEMS: NavItem[] = [
  { id: 'profile', label: 'Profile', type: 'default', icon: '👤' },
  { id: 'followers', label: 'Followers', type: 'default', icon: '👥' },
  { id: 'following', label: 'Following', type: 'default', icon: '👤' },
];

const DEFAULT_WIDGET_THEME: WidgetTheme = {
  id: 'default',
  name: 'Default',
  background: 'var(--surface)',
  border_color: 'var(--border-color)',
  border_radius: '6px',
  text_color: 'var(--text-primary)',
  padding: '16px',
};

function BackButton() {
  return (
    <button
      className="profile-back-btn"
      onClick={() => window.dispatchEvent(new CustomEvent('bytescape:viewOwnProfile'))}
      style={{
        background: 'none',
        border: 'none',
        color: 'var(--accent)',
        cursor: 'pointer',
        fontFamily: 'var(--font-mono)',
        fontSize: 13,
        padding: '12px 0',
      }}
    >
      ← Back to my profile
    </button>
  );
}

function FollowButton({ profile, followLoading, handleFollow, isOwn, editMode }: {
  profile: ProfileData;
  followLoading: boolean;
  handleFollow: () => void;
  isOwn: boolean;
  editMode: boolean;
}) {
  if (isOwn || editMode) return null;
  return (
    <button
      className="btn"
      onClick={handleFollow}
      disabled={followLoading}
      style={{
        width: '100%',
        marginBottom: 12,
        background: profile.is_following || profile.follow_pending ? 'transparent' : 'var(--accent)',
        color: profile.is_following || profile.follow_pending ? 'var(--text-muted)' : '#000',
        border: profile.is_following || profile.follow_pending ? '1px solid var(--border-color)' : 'none',
        fontWeight: 600,
        fontSize: 13,
        padding: '10px',
      }}
    >
      {followLoading
        ? '...'
        : profile.follow_pending
          ? 'Requested — tap to cancel'
          : profile.is_following
            ? 'Following'
            : 'Follow'}
    </button>
  );
}

function useBreakpoint() {
  const [bp, setBp] = useState<'desktop' | 'tablet' | 'mobile'>(() => {
    const w = window.innerWidth;
    if (w > 1200) return 'desktop';
    if (w > 900) return 'tablet';
    return 'mobile';
  });
  useEffect(() => {
    const onResize = () => {
      const w = window.innerWidth;
      if (w > 1200) setBp('desktop');
      else if (w > 900) setBp('tablet');
      else setBp('mobile');
    };
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);
  return bp;
}

export default function ProfileScreen({ username, onViewProfile }: Props) {
  const [profile, setProfile] = useState<ProfileData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [activeNav, setActiveNav] = useState('profile');
  const [navItems, setNavItems] = useState<NavItem[]>(DEFAULT_NAV_ITEMS);
  const bp = useBreakpoint();

  // Widget editor state
  const [editMode, setEditMode] = useState(false);
  const [previewMode, setPreviewMode] = useState(false);
  const [layout, setLayout] = useState<Widget[]>(DEFAULT_LAYOUT);
  const [themes, setThemes] = useState<WidgetTheme[]>([]);
  const [editForm, setEditForm] = useState<Record<string, unknown>>({});
  const [saving, setSaving] = useState(false);
  const [savedLayout, setSavedLayout] = useState<Widget[]>([]);
  const [savedThemes, setSavedThemes] = useState<WidgetTheme[]>([]);
  const [themeEditorWidgetId, setThemeEditorWidgetId] = useState<string | null>(null);
  const [showImport, setShowImport] = useState(false);

  // Simple profile form state (for avatar/name/bio editing)
  const [showInfoForm, setShowInfoForm] = useState(false);
  const [infoForm, setInfoForm] = useState<UpdateProfileRequest>({});
  const [editSaving, setEditSaving] = useState(false);

  const [followLoading, setFollowLoading] = useState(false);
  const [hamburgerOpen, setHamburgerOpen] = useState(false);

  const loadProfile = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = username ? await profileApi.get(username) : await profileApi.me();
      setProfile(data);
      // Restore custom nav from localStorage
      const stored = localStorage.getItem('bytescape_nav_items');
      if (stored && !username) {
        try {
          const parsed = JSON.parse(stored);
          if (Array.isArray(parsed)) setNavItems(parsed);
        } catch {}
      }
    } catch (e: any) {
      setError(e.message || 'Failed to load profile');
    } finally {
      setLoading(false);
    }
  }, [username]);

  useEffect(() => {
    loadProfile();
    setEditMode(false);
    setPreviewMode(false);
    setShowInfoForm(false);
    setActiveNav('profile');
  }, [username]);

  const handleAddPage = (name: string, type: string) => {
    const newItem: NavItem = {
      id: `custom_${Date.now()}`,
      label: name,
      type: 'custom',
      icon: type === 'Blog' ? '📝' : type === 'Gallery' ? '🖼️' : '🔗',
    };
    const updated = [...navItems, newItem];
    setNavItems(updated);
    localStorage.setItem('bytescape_nav_items', JSON.stringify(updated));
  };

  const handleRemovePage = (id: string) => {
    const updated = navItems.filter(i => i.id !== id);
    setNavItems(updated);
    localStorage.setItem('bytescape_nav_items', JSON.stringify(updated));
    if (activeNav === id) setActiveNav('profile');
  };

  const handleFollow = async () => {
    if (!profile) return;
    setFollowLoading(true);
    try {
      if (profile.is_following || profile.follow_pending) {
        // Unfollow — for a pending request this is the cancel (the undo
        // withdraws the Follow on-network).
        await profileApi.unfollow(profile.username);
        setProfile(prev => prev ? {
          ...prev,
          is_following: false,
          follow_pending: false,
          followers_count: prev.followers_count - (prev.is_following ? 1 : 0),
        } : null);
      } else {
        const res = await profileApi.follow(profile.username);
        const pending = res.pending === true;
        setProfile(prev => prev ? {
          ...prev,
          is_following: true,
          follow_pending: pending,
          followers_count: prev.followers_count + (pending ? 0 : 1),
        } : null);
      }
    } catch (e) {
      // Stale Mastodon grants fail with the instruction-focused re-link
      // message — surface it as the prompt, not a generic error banner.
      const relink = reactionErrorToastMessage(e);
      if (relink) {
        relinkToast(relink);
      } else {
        setError(e instanceof Error ? e.message : 'Action failed');
      }
    } finally {
      setFollowLoading(false);
    }
  };

  // ── Widget grid editing ──
  const handleEditProfile = () => {
    if (!profile) return;
    setEditMode(true);
    setPreviewMode(false);
    const currentLayout = profile.widget_layout?.length ? profile.widget_layout : DEFAULT_LAYOUT;
    const currentThemes = profile.widget_themes || [];
    setLayout([...currentLayout]);
    setThemes([...currentThemes]);
    setSavedLayout([...currentLayout]);
    setSavedThemes([...currentThemes]);
    setThemeEditorWidgetId(null);
  };

  const handleSaveProfile = async () => {
    setSaving(true);
    try {
      const updated = await profileApi.update({
        widget_layout: layout,
        widget_themes: themes,
      });
      setProfile(updated);
      setSavedLayout([...layout]);
      setSavedThemes([...themes]);
      setEditMode(false);
      setPreviewMode(false);
      window.dispatchEvent(new Event('bytescape:refreshFeed'));
      window.dispatchEvent(new Event('bytescape:profileUpdated'));
    } catch (e: any) {
      setError(e.message || 'Save failed');
    } finally {
      setSaving(false);
    }
  };

  const handleCancelEdit = () => {
    setLayout([...savedLayout]);
    setThemes([...savedThemes]);
    setEditMode(false);
    setPreviewMode(false);
    setThemeEditorWidgetId(null);
  };

  const handleAddWidget = (type: WidgetType) => {
    const newWidget: Widget = {
      id: `widget_${Date.now()}_${Math.random().toString(36).slice(2, 7)}`,
      widget_type: type,
      x: 0,
      y: Infinity, // place at bottom
      w: 10,
      h: 6,
      data: {},
      theme_id: 'default',
    };
    setLayout(prev => [...prev, newWidget]);
  };

  const handleDeleteWidget = (widgetId: string) => {
    setLayout(prev => prev.filter(w => w.id !== widgetId));
  };

  const handleResetLayout = () => {
    setLayout([...savedLayout]);
    setThemes([...savedThemes]);
  };

  const handlePreview = () => {
    setPreviewMode(prev => !prev);
  };

  const handleWidgetDataChange = (widgetId: string, data: Record<string, unknown>) => {
    setLayout(prev => prev.map(w => w.id === widgetId ? { ...w, data: { ...w.data, ...data } } : w));
  };

  const handleLayoutChange = (newLayout: Widget[]) => {
    setLayout(newLayout);
  };

  // ── Preset theme (saved immediately on pick) ──
  const handleThemeChange = async (nextTheme: string) => {
    try {
      const updated = await profileApi.update({ theme: nextTheme });
      setProfile(updated);
    } catch (e) {
      console.error('Theme save failed:', e);
    }
  };

  // ── Simple info editing (avatar/name/bio) ──
  const handleOpenInfoForm = () => {
    if (!profile) return;
    setShowInfoForm(true);
    setInfoForm({
      display_name: profile.display_name,
      bio: profile.bio,
      avatar_url: profile.avatar_url || undefined,
      header_url: profile.header_url || undefined,
      location: profile.location,
      website: profile.website,
      pronouns: profile.pronouns,
      relationship_status: profile.relationship_status,
      birthday: profile.birthday,
      occupation: profile.occupation,
      interests: profile.interests,
      custom_profile_fields: profile.custom_profile_fields,
    });
  };

  const handleSaveInfo = async () => {
    setEditSaving(true);
    try {
      const updated = await profileApi.update(infoForm);
      setProfile(updated);
      setShowInfoForm(false);
      setInfoForm({});
      if (updated.interests) {
        cacheInterests(updated.interests);
      }
      window.dispatchEvent(new Event('bytescape:refreshFeed'));
      window.dispatchEvent(new Event('bytescape:profileUpdated'));
    } catch (e: any) {
      setError(e.message || 'Save failed');
    } finally {
      setEditSaving(false);
    }
  };

  const handleCancelInfo = () => {
    setShowInfoForm(false);
    setInfoForm({});
  };

  // Listen for tour + toolbar events (named handler: add/removeEventListener
  // pairs need a stable reference for cleanup)
  useEffect(() => {
    const handleProfileEvent = (e: Event) => {
      if (e.type === 'bytescape:editProfile') handleEditProfile();
      if (e.type === 'bytescape:openImport') setShowImport(true);
    };
    window.addEventListener('bytescape:editProfile', handleProfileEvent);
    window.addEventListener('bytescape:openImport', handleProfileEvent);
    return () => {
      window.removeEventListener('bytescape:editProfile', handleProfileEvent);
      window.removeEventListener('bytescape:openImport', handleProfileEvent);
    };
  }, [profile]);

  if (loading) return <div className="profile-screen"><div className="profile-loading">Loading...</div></div>;
  if (error) return <div className="profile-screen"><div className="profile-error">{error}</div></div>;
  if (!profile) return <div className="profile-screen"><div className="profile-error">Profile not found</div></div>;

  const isOwn = profile.is_own_profile;
  const themeClass = profile.theme && profile.theme !== 'default' ? `profile-theme theme-${profile.theme}` : '';
  const importModal = isOwn && showImport && createPortal(
    <ImportStep
      onComplete={() => { setShowImport(false); }}
      onSkip={() => setShowImport(false)}
    />,
    document.body
  );
  const isMobile = bp === 'mobile';
  const isTablet = bp === 'tablet';

  // Main content area — edit mode vs view mode
  const renderCenter = () => {
    // The Followers tab renders the merged follower widget instead of the
    // grid (the nav item used to render nothing).
    if (activeNav === 'followers' && !editMode) {
      return (
        <WidgetFollowers
          username={profile.username}
          isOwn={isOwn}
          onViewProfile={onViewProfile}
        />
      );
    }
    if (editMode) {
      return (
        <>
          <ProfileToolbar
            isOwn={isOwn}
            editMode={editMode}
            previewMode={previewMode}
            saving={saving}
            onEdit={() => {}}
            onPreview={handlePreview}
            onSave={handleSaveProfile}
            onCancel={handleCancelEdit}
            onAddWidget={handleAddWidget}
            theme={profile.theme}
            onThemeChange={handleThemeChange}
            onResetLayout={handleResetLayout}
          />
          <ProfileGrid
            widgets={layout}
            themes={themes}
            profile={profile}
            editing={!previewMode}
            onViewProfile={onViewProfile}
            onDeleteWidget={handleDeleteWidget}
            onOpenThemeEditor={(widgetId) => setThemeEditorWidgetId(widgetId)}
            onWidgetDataChange={handleWidgetDataChange}
            editForm={editForm}
            onEditFormChange={setEditForm}
            onLayoutChange={handleLayoutChange}
            navItems={navItems}
            activeNav={activeNav}
            onNavSelect={setActiveNav}
            onAddPage={handleAddPage}
            onRemovePage={handleRemovePage}
          />
          {themeEditorWidgetId && (() => {
            const widget = layout.find(w => w.id === themeEditorWidgetId);
            if (!widget) return null;
            const widgetTheme = themes.find(t => t.id === widget.theme_id);
            return (
              <WidgetThemeEditor
                widget={widget}
                theme={widgetTheme || DEFAULT_WIDGET_THEME}
                themes={themes}
                onSave={(newTheme) => {
                  setThemes(prev => {
                    const idx = prev.findIndex(t => t.id === newTheme.id);
                    if (idx >= 0) {
                      const updated = [...prev];
                      updated[idx] = newTheme;
                      return updated;
                    }
                    return [...prev, newTheme];
                  });
                  setLayout(prev => prev.map(w =>
                    w.id === widget.id ? { ...w, theme_id: newTheme.id } : w
                  ));
                  setThemeEditorWidgetId(null);
                }}
                onClose={() => setThemeEditorWidgetId(null)}
              />
            );
          })()}
        </>
      );
    }

    // View mode — show widget grid with toolbar
    return (
      <>
        <ProfileToolbar
          isOwn={isOwn}
          editMode={false}
          previewMode={false}
          saving={saving}
          onEdit={handleEditProfile}
          onPreview={handlePreview}
          onSave={handleSaveProfile}
          onCancel={handleCancelEdit}
          onAddWidget={handleAddWidget}
            theme={profile.theme}
            onThemeChange={handleThemeChange}
          onResetLayout={handleResetLayout}
        />
        <ProfileGrid
          widgets={layout}
          themes={themes}
          profile={profile}
          editing={false}
          onViewProfile={onViewProfile}
          onDeleteWidget={handleDeleteWidget}
          onOpenThemeEditor={(widgetId) => setThemeEditorWidgetId(widgetId)}
          onWidgetDataChange={handleWidgetDataChange}
          editForm={editForm}
          onEditFormChange={setEditForm}
          onLayoutChange={handleLayoutChange}
          navItems={navItems}
          activeNav={activeNav}
          onNavSelect={setActiveNav}
          onAddPage={handleAddPage}
          onRemovePage={handleRemovePage}
        />
      </>
    );
  };

  // Mobile layout — hamburger nav + widget grid
  if (isMobile) {
    return (
      <div className={`profile-screen profile-screen-widgets ${themeClass}`} style={{ padding: '0 12px 24px' }}>
        {importModal}
        {username && <BackButton />}

        {/* Hamburger nav */}
        <div style={{ position: 'relative', marginBottom: 12 }}>
          <button
            onClick={() => setHamburgerOpen(prev => !prev)}
            style={{
              background: 'var(--surface)',
              border: '1px solid var(--border-color)',
              borderRadius: 4,
              color: 'var(--text-secondary)',
              padding: '8px 12px',
              width: '100%',
              cursor: 'pointer',
              fontFamily: 'var(--font-mono)',
              fontSize: 13,
              textAlign: 'left',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
            }}
          >
            <span>☰ Navigation</span>
            <span style={{ fontSize: 10 }}>{hamburgerOpen ? '▲' : '▼'}</span>
          </button>
          {hamburgerOpen && (
            <div style={{
              position: 'absolute',
              top: '100%',
              left: 0,
              right: 0,
              zIndex: 40,
              background: 'var(--bg-secondary, #1a1a2e)',
              border: '1px solid var(--border-color)',
              borderRadius: 4,
              marginTop: 4,
              boxShadow: '0 4px 12px rgba(0,0,0,0.3)',
            }}>
              {navItems.map(item => (
                <button
                  key={item.id}
                  onClick={() => { setActiveNav(item.id); setHamburgerOpen(false); }}
                  style={{
                    display: 'block',
                    width: '100%',
                    padding: '10px 14px',
                    background: activeNav === item.id ? 'rgba(0, 255, 65, 0.06)' : 'none',
                    border: 'none',
                    borderBottom: '1px solid var(--border-color)',
                    color: activeNav === item.id ? 'var(--accent)' : 'var(--text-secondary)',
                    textAlign: 'left',
                    cursor: 'pointer',
                    fontFamily: 'var(--font-retro)',
                    fontSize: 15,
                  }}
                >
                  {item.icon && <span style={{ marginRight: 6 }}>{item.icon}</span>}
                  {item.label}
                </button>
              ))}
            </div>
          )}
        </div>

        {/* Follow button for other profiles */}
        <FollowButton profile={profile} followLoading={followLoading} handleFollow={handleFollow} isOwn={isOwn} editMode={editMode} />

        {/* Widget grid — same grid as desktop, responsive cols handle stacking */}
        {renderCenter()}
      </div>
    );
  }

  // Tablet layout — horizontal nav + widget grid
  if (isTablet) {
    return (
      <div className={`profile-screen profile-screen-widgets ${themeClass}`} style={{ padding: '0 16px 24px' }}>
        {importModal}
        {username && <BackButton />}

        {/* Top nav bar for tablet */}
        <div style={{
          display: 'flex',
          gap: 4,
          marginBottom: 16,
          overflowX: 'auto',
          paddingBottom: 4,
        }}>
          {navItems.map(item => (
            <button
              key={item.id}
              onClick={() => setActiveNav(item.id)}
              style={{
                padding: '6px 12px',
                background: activeNav === item.id ? 'rgba(0, 255, 65, 0.08)' : 'var(--surface)',
                border: `1px solid ${activeNav === item.id ? 'var(--accent)' : 'var(--border-color)'}`,
                borderRadius: 4,
                color: activeNav === item.id ? 'var(--accent)' : 'var(--text-muted)',
                cursor: 'pointer',
                fontFamily: 'var(--font-retro)',
                fontSize: 14,
                whiteSpace: 'nowrap',
              }}
            >
              {item.icon && <span style={{ marginRight: 4 }}>{item.icon}</span>}
              {item.label}
            </button>
          ))}
        </div>

        {/* Follow button */}
        <FollowButton profile={profile} followLoading={followLoading} handleFollow={handleFollow} isOwn={isOwn} editMode={editMode} />

        {/* Widget grid — same grid as desktop, responsive cols handle layout */}
        {renderCenter()}
      </div>
    );
  }

  // Desktop — just widget grid, no sidebar
  return (
    <div className={`profile-screen profile-screen-widgets ${themeClass}`}>
      {importModal}
      {username && <BackButton />}
      <ProfileToolbar
        isOwn={isOwn}
        editMode={editMode}
        previewMode={previewMode}
        saving={saving}
        onEdit={editMode ? () => {} : handleEditProfile}
        onPreview={handlePreview}
        onSave={handleSaveProfile}
        onCancel={handleCancelEdit}
        onAddWidget={handleAddWidget}
            theme={profile.theme}
            onThemeChange={handleThemeChange}
        onResetLayout={handleResetLayout}
      />
      {activeNav === 'followers' && !editMode ? (
        <WidgetFollowers username={profile.username} isOwn={isOwn} onViewProfile={onViewProfile} />
      ) : (
        <ProfileGrid
          widgets={layout}
          themes={themes}
          profile={profile}
          editing={editMode ? !previewMode : false}
          onViewProfile={onViewProfile}
          onDeleteWidget={handleDeleteWidget}
          onOpenThemeEditor={(widgetId) => setThemeEditorWidgetId(widgetId)}
          onWidgetDataChange={handleWidgetDataChange}
          editForm={editForm}
          onEditFormChange={setEditForm}
          onLayoutChange={handleLayoutChange}
          navItems={navItems}
          activeNav={activeNav}
          onNavSelect={setActiveNav}
          onAddPage={handleAddPage}
          onRemovePage={handleRemovePage}
        />
      )}
      {themeEditorWidgetId && (() => {
        const widget = layout.find(w => w.id === themeEditorWidgetId);
        if (!widget) return null;
        const widgetTheme = themes.find(t => t.id === widget.theme_id);
        return (
          <WidgetThemeEditor
            widget={widget}
            theme={widgetTheme || DEFAULT_WIDGET_THEME}
            themes={themes}
            onSave={(newTheme) => {
              setThemes(prev => {
                const idx = prev.findIndex(t => t.id === newTheme.id);
                if (idx >= 0) {
                  const updated = [...prev];
                  updated[idx] = newTheme;
                  return updated;
                }
                return [...prev, newTheme];
              });
              setLayout(prev => prev.map(w =>
                w.id === widget.id ? { ...w, theme_id: newTheme.id } : w
              ));
              setThemeEditorWidgetId(null);
            }}
            onClose={() => setThemeEditorWidgetId(null)}
          />
        );
      })()}
    </div>
  );
}
