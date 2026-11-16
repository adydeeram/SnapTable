/**
 * Small fuzzy matcher for the command palette: every query character must appear in
 * order. Scores favour consecutive runs, word starts and early matches. Returns null
 * when there is no match.
 */
export function fuzzyScore(query: string, text: string): number | null {
  const q = query.toLowerCase().replace(/\s+/g, "");
  if (!q) return 0;
  const t = text.toLowerCase();
  let score = 0;
  let ti = 0;
  let prev = -2;
  for (const ch of q) {
    const found = t.indexOf(ch, ti);
    if (found < 0) return null;
    const wordStart = found === 0 || /[\s._\-/:]/.test(t[found - 1]);
    score += 1 + (found === prev + 1 ? 3 : 0) + (wordStart ? 2 : 0);
    prev = found;
    ti = found + 1;
  }
  // Prefer shorter texts and matches that start early.
  return score - t.length * 0.01 - (t.indexOf(q[0]) || 0) * 0.05;
}
