/** Compact game counts for tight spots: 842, 12.4k, 1.2M. */
export function fmtCompact(n: number): string {
	if (n < 1000) return String(n);
	if (n < 10_000) return (n / 1000).toFixed(1).replace(/\.0$/, '') + 'k';
	if (n < 1_000_000) return Math.round(n / 1000) + 'k';
	if (n < 10_000_000) return (n / 1_000_000).toFixed(1).replace(/\.0$/, '') + 'M';
	if (n < 1_000_000_000) return Math.round(n / 1_000_000) + 'M';
	return (n / 1_000_000_000).toFixed(1) + 'B';
}
