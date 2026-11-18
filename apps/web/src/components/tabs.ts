export type TabName =
  | 'Feed'
  | 'Groups'
  | 'Forums'
  | 'Exchange'
  | 'Events'
  | 'Account'
  | 'Settings'
  | 'Profile'
  | 'Chat'
  | 'Vault'
  | 'Import'
  | 'Notifications';

export const TAB_ICONS: Record<TabName, string> = {
  'Feed': '📰',
  'Groups': '👥',
  'Forums': '💬',
  'Exchange': '🏷️',
  'Events': '📅',
  'Account': '🔗',
  'Settings': '⚙️',
  'Profile': '👤',
  'Chat': '💬',
  'Vault': '🖼️',
  'Import': '📦',
  'Notifications': '🔔',
};
