/**
 * PURPOSE
 * Minimal date formatting helper for Unix timestamps used in UI labels.
 *
 * HOW IT WORKS
 * Converts seconds-based Unix timestamps to a localized en-US date/time string.
 *
 * RELATED FILES
 * - screens/components that display item timestamps
 */
/**
 * Formats a Unix timestamp (seconds) into an en-US long date/time string.
 */
export function formatDate(ts: number) {
    return new Date(ts * 1000).toLocaleDateString("en-US", {
        year: "numeric",
        month: "long",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
    });
}
