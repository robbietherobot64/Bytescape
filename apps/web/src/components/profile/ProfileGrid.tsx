import { useCallback } from 'react';
import { Responsive, WidthProvider } from 'react-grid-layout/legacy';
import type { Layout } from 'react-grid-layout/legacy';
import type { ProfileData, Widget, WidgetTheme } from '../../utils/api';
import WidgetProfileHeader from './widgets/WidgetProfileHeader';
import WidgetNavigation from './widgets/WidgetNavigation';
import WidgetFriends from './widgets/WidgetFriends';
import WidgetFollowing from './widgets/WidgetFollowing';
import WidgetFollowers from './widgets/WidgetFollowers';
import WidgetMusicPlayer from './widgets/WidgetMusicPlayer';
import WidgetPhotoGallery from './widgets/WidgetPhotoGallery';
import WidgetCustomLinks from './widgets/WidgetCustomLinks';
import WidgetStatus from './widgets/WidgetStatus';
import WidgetInterests from './widgets/WidgetInterests';
import WidgetPosts from './widgets/WidgetPosts';
import type { NavItem } from './ProfileNav';

const GridLayout = WidthProvider(Responsive);

interface Props {
  widgets: Widget[];
  themes: WidgetTheme[];
  profile: ProfileData;
  editing: boolean;
  onViewProfile?: (username: string) => void;
  onDeleteWidget?: (id: string) => void;
  onOpenThemeEditor?: (widgetId: string) => void;
  onWidgetDataChange?: (widgetId: string, data: Record<string, unknown>) => void;
  editForm?: Record<string, unknown>;
  onEditFormChange?: (form: Record<string, unknown>) => void;
  onLayoutChange?: (newLayout: Widget[]) => void;
  navItems?: NavItem[];
  activeNav?: string;
  onNavSelect?: (id: string) => void;
  onAddPage?: (name: string, type: string) => void;
  onRemovePage?: (id: string) => void;
}

function resolveTheme(themeId: string, themes: WidgetTheme[]): WidgetTheme | undefined {
  return themes.find(t => t.id === themeId);
}

export default function ProfileGrid({
  widgets,
  themes,
  profile,
  editing,
  onViewProfile,
  onDeleteWidget,
  onOpenThemeEditor,
  onWidgetDataChange,
  editForm,
  onEditFormChange,
  onLayoutChange,
  navItems,
  activeNav,
  onNavSelect,
  onAddPage,
  onRemovePage,
}: Props) {
  const mkLayout = (widgets: Widget[], cols: number) =>
    widgets.map(w => cols === 1 ? { i: w.id, x: 0, y: w.y, w: 1, h: w.h } : { i: w.id, x: w.x, y: w.y, w: w.w, h: w.h });
  const layouts = {
    lg: mkLayout(widgets, 20),
    md: mkLayout(widgets, 20),
    sm: widgets.map(w => ({ i: w.id, x: 0, y: w.y, w: 1, h: w.h })),
    xs: widgets.map(w => ({ i: w.id, x: 0, y: w.y, w: 1, h: w.h })),
    xxs: widgets.map(w => ({ i: w.id, x: 0, y: w.y, w: 1, h: w.h })),
  };

  const handleLayoutChange = useCallback(
    (currentLayout: Layout) => {
      if (!onLayoutChange) return;
      const updated = widgets.map(w => {
        const found = currentLayout.find(l => l.i === w.id);
        if (found) {
          return { ...w, x: found.x, y: found.y, w: found.w, h: found.h };
        }
        return w;
      });
      onLayoutChange(updated);
    },
    [widgets, onLayoutChange]
  );

  return (
    <GridLayout
      className="profile-grid"
      layouts={layouts}
      breakpoints={{ lg: 1200, md: 996, sm: 768, xs: 480, xxs: 0 }}
      cols={{ lg: 20, md: 20, sm: 1, xs: 1, xxs: 1 }}
      rowHeight={10}
      compactType={null}
      preventCollision={true}
      margin={[16, 16]}
      isDraggable={editing}
      isResizable={editing}
      onLayoutChange={handleLayoutChange}
    >
      {widgets.map((widget, idx) => {
        const theme = resolveTheme(widget.theme_id, themes);
        const style: Record<string, string> = {};
        if (theme?.background) style['--widget-bg' as string] = theme.background;
        if (theme?.border_color) style['--widget-border' as string] = `1px solid ${theme.border_color}`;
        if (theme?.border_radius) style['--widget-radius' as string] = theme.border_radius;
        if (theme?.text_color) style['--widget-text' as string] = theme.text_color;
        if (theme?.font_family) style['--widget-font' as string] = theme.font_family;
        if (theme?.padding) style['--widget-padding' as string] = theme.padding;
        if (theme?.shadow) style['--widget-shadow' as string] = theme.shadow;

        return (
          <div
            key={widget.id}
            className={`widget-card ${editing ? 'widget-editing' : ''}`}
            style={style}
            data-tour={widget.widget_type === 'interests' ? 'interests' : widget.widget_type === 'posts' ? 'import' : undefined}
          >
            {editing && (
              <div className="widget-controls">
                {widget.widget_type === 'profile_header' && profile.is_own_profile && (
                  <button
                    className="widget-control-btn widget-control-config"
                    onClick={(e) => {
                      const rect = e.currentTarget.getBoundingClientRect();
                      window.dispatchEvent(new CustomEvent('profile:toggle-config', { detail: { widgetId: widget.id, btnRect: rect } }));
                    }}
                    title="Configure sections"
                  >
                    📝
                  </button>
                )}
                <button
                  className="widget-control-btn"
                  onClick={() => onOpenThemeEditor?.(widget.id)}
                  title="Theme settings"
                >
                  ⚙
                </button>
                <button
                  className="widget-control-btn widget-control-delete"
                  onClick={() => onDeleteWidget?.(widget.id)}
                  title="Delete widget"
                >
                  ×
                </button>
              </div>
            )}

            {renderWidget(widget, profile, editing, {
              onViewProfile,
              onWidgetDataChange,
              editForm,
              onEditFormChange,
              navItems,
              activeNav,
              onNavSelect,
              onAddPage,
              onRemovePage,
            })}
          </div>
        );
      })}
    </GridLayout>
  );
}

export function renderWidget(
  widget: Widget,
  profile: ProfileData,
  editing: boolean,
  opts: {
    onViewProfile?: (username: string) => void;
    onWidgetDataChange?: (widgetId: string, data: Record<string, unknown>) => void;
    editForm?: Record<string, unknown>;
    onEditFormChange?: (form: Record<string, unknown>) => void;
    navItems?: NavItem[];
    activeNav?: string;
    onNavSelect?: (id: string) => void;
    onAddPage?: (name: string, type: string) => void;
    onRemovePage?: (id: string) => void;
  }
) {
  switch (widget.widget_type) {
    case 'profile_header':
      return <WidgetProfileHeader widget={widget} profile={profile} editing={editing} editForm={opts.editForm} onEditFormChange={opts.onEditFormChange} onDataChange={(data) => opts.onWidgetDataChange?.(widget.id, data)} />;
    case 'navigation':
      return (
        <WidgetNavigation
          navItems={opts.navItems || []}
          activeNav={opts.activeNav || 'profile'}
          onNavSelect={opts.onNavSelect || (() => {})}
          onAddPage={opts.onAddPage || (() => {})}
          onRemovePage={opts.onRemovePage || (() => {})}
          isOwnProfile={profile.is_own_profile}
        />
      );
    case 'friends':
      return <WidgetFriends username={profile.username} onViewProfile={opts.onViewProfile} />;
    case 'following':
      return <WidgetFollowing username={profile.username} onViewProfile={opts.onViewProfile} />;
    case 'followers':
      return (
        <WidgetFollowers
          username={profile.username}
          isOwn={profile.is_own_profile}
          onViewProfile={opts.onViewProfile}
        />
      );
    case 'music_player':
      return <WidgetMusicPlayer widget={widget} editing={editing} onDataChange={(data) => opts.onWidgetDataChange?.(widget.id, data)} />;
    case 'photo_gallery':
      return <WidgetPhotoGallery widget={widget} editing={editing} onDataChange={(data) => opts.onWidgetDataChange?.(widget.id, data)} />;
    case 'custom_links':
      return <WidgetCustomLinks widget={widget} editing={editing} onDataChange={(data) => opts.onWidgetDataChange?.(widget.id, data)} />;
    case 'status':
      return <WidgetStatus widget={widget} editing={editing} onDataChange={(data) => opts.onWidgetDataChange?.(widget.id, data)} />;
    case 'interests':
      return <WidgetInterests profile={profile} isOwnProfile={profile.is_own_profile} />;
    case 'posts':
      return <WidgetPosts username={profile.username} />;
    default:
      return <div style={{ color: 'var(--text-muted)' }}>Unknown widget: {widget.widget_type}</div>;
  }
}
