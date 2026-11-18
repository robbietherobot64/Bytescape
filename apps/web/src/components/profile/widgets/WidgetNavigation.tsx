import ProfileNav from '../ProfileNav';
import type { NavItem } from '../ProfileNav';

interface Props {
  navItems: NavItem[];
  activeNav: string;
  onNavSelect: (id: string) => void;
  onAddPage: (name: string, type: string) => void;
  onRemovePage: (id: string) => void;
  isOwnProfile: boolean;
}

export default function WidgetNavigation({
  navItems,
  activeNav,
  onNavSelect,
  onAddPage,
  onRemovePage,
  isOwnProfile,
}: Props) {
  return (
    <div className="widget-navigation" style={{ height: '100%' }}>
      <ProfileNav
        items={navItems}
        activeItem={activeNav}
        onSelect={onNavSelect}
        onAddPage={onAddPage}
        onRemovePage={onRemovePage}
        isOwnProfile={isOwnProfile}
      />
    </div>
  );
}
