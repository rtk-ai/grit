/**
 * Format a pi value for display with a given number of decimal digits.
 */
export function formatPiValue(value: number, digits: number): string {
  const str = value.toFixed(digits);
  const parts = str.split(".");
  if (parts.length !== 2) return str;

  const intPart = parts[0];
  const decPart = parts[1].slice(0, digits);
  const grouped = groupDigits(decPart, 5);

  return `${intPart}.${grouped}`;
}

/**
 * Insert spaces between groups of digits for readability.
 * e.g., "14159265" with groupSize=4 becomes "1415 9265"
 */
export function groupDigits(str: string, groupSize: number): string {
  const groups: string[] = [];
  for (let i = 0; i < str.length; i += groupSize) {
    groups.push(str.slice(i, i + groupSize));
  }
  return groups.join(" ");
}

/**
 * Convert a duration in milliseconds to a human-readable string.
 */
export function formatDuration(ms: number): string {
  if (ms < 0.001) {
    return `${(ms * 1_000_000).toFixed(0)}ns`;
  }
  if (ms < 1) {
    return `${(ms * 1000).toFixed(1)}us`;
  }
  if (ms < 1000) {
    return `${ms.toFixed(2)}ms`;
  }
  if (ms < 60_000) {
    return `${(ms / 1000).toFixed(2)}s`;
  }
  const minutes = Math.floor(ms / 60_000);
  const seconds = ((ms % 60_000) / 1000).toFixed(1);
  return `${minutes}m ${seconds}s`;
}

/**
 * Format a number with locale-aware thousand separators.
 */
export function formatNumber(n: number): string {
  if (Number.isInteger(n)) {
    return n.toLocaleString("en-US");
  }
  const fixed = n.toFixed(2);
  const [intPart, decPart] = fixed.split(".");
  const formattedInt = parseInt(intPart, 10).toLocaleString("en-US");
  return `${formattedInt}.${decPart}`;
}

/**
 * Calculate accuracy as a percentage based on the ratio of calculated vs known pi.
 */
export function calculateAccuracy(calculated: number, known: number): number {
  if (known === 0) return 0;
  const error = Math.abs(calculated - known) / Math.abs(known);
  const accuracy = (1 - error) * 100;
  return Math.max(0, Math.min(100, accuracy));
}
