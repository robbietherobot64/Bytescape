import { useState } from 'react';
import type { Widget, ProfileData, WidgetTheme } from '../../utils/api';
import { renderWidget } from './ProfileGrid';

interface Props {
  widgets: Widget[];
  themes: WidgetTheme[];
  profile: ProfileData;
  isOwn: boolean;
  isFollowing: boolean;
  onViewProfile?: (username: string) => void;
  onOpenThemeEditor?: (widgetId: string) => void;
  onWidgetDataChange?: (widgetId: string, data: Record<string, unknown>) => void;
  editForm?: Record<string, unknown>;
  onEditFormChange?: (form: Record<string, unknown>) => void;
}

const GROUP_ORDER = ['about', 'posts', 'connections', 'more'];

const GROUP_MAP: Record<string, string> = {
  profile_header: 'about',
  avatar_name: 'about',
  stats: 'about',
  about: 'about',
  posts: 'posts',
  photo_gallery: 'posts',
  navigation: 'posts',
  wall: 'more',
  friends: 'connections',
  following: 'connections',
  music_player: 'more',
  custom_links: 'more',
  status: 'more',
};

const GROUP_LABELS: Record<string, string> = {
  about: 'About Me',
  posts: 'My Posts',
  connections: 'Connections',
  more: 'More',
};

function resolveTheme(themeId: string, themes: WidgetTheme[]): WidgetTheme | undefined {
  return themes.find(t => t.id === themeId);
}

export default function MobileProfileRender({
  widgets,
  themes,
  profile,
  onViewProfile,
  onWidgetDataChange,
  editForm,
  onEditFormChange,
}: Props) {
  const [openGroups, setOpenGroups] = useState<Set<string>>(new Set(['about']));

  const grouped: Record<string, Widget[]> = {};
  for (const widget of widgets) {
    const group = GROUP_MAP[widget.widget_type] || 'more';
    if (!grouped[group]) grouped[group] = [];
    grouped[group].push(widget);
  }

  const toggleGroup = (group: string) => {
    setOpenGroups(prev => {
      const next = new Set(prev);
      if (next.has(group)) {
        next.delete(group);
      } else {
        next.add(group);
      }
      return next;
    });
  };

  return (
    <div className="profile-mobile">
      {GROUP_ORDER.map(group => {
        const groupWidgets = grouped[group] || [];
        if (groupWidgets.length === 0) return null;
        const isOpen = openGroups.has(group);

        return (
          <div className="profile-mobile-group" key={group}>
            <div
              className={`profile-mobile-group-header${isOpen ? ' active' : ''}`}
              onClick={() => toggleGroup(group)}
            >
              <span>{GROUP_LABELS[group] || group}</span>
              <span>{isOpen ? '−' : '+'}</span>
            </div>
            <div className={`profile-mobile-group-body${isOpen ? '' : ' collapsed'}`}>
              {groupWidgets.map(widget => {
                const theme = resolveTheme(widget.theme_id, themes);
                const style: Record<string, string> = {};
                if (theme?.background) style['--widget-bg' as string] = theme.background;
                if (theme?.text_color) style['--widget-text' as string] = theme.text_color;
                if (theme?.font_family) style['--widget-font' as string] = theme.font_family;
                if (theme?.padding) style['--widget-padding' as string] = theme.padding;
                if (theme?.shadow) style['--widget-shadow' as string] = theme.shadow;

                return (
                  <div key={widget.id} className="profile-mobile-widget" style={style}>
                    {renderWidget(widget, profile, false, {
                      onViewProfile,
                      onWidgetDataChange,
                      editForm,
                      onEditFormChange,
                    })}
                  </div>
                );
              })}
            </div>
          </div>
        );
      })}
    </div>
  );
}
