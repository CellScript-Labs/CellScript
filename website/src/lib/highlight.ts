/**
 * CellScript syntax highlighter for the playground editor overlay.
 *
 * This is the same tokenizer used by the landing page hero code
 * panels, extracted into a shared module so the playground textarea
 * overlay can reuse it. The token classes map 1:1 to the CSS
 * `.token-*` rules in global.css, so the two surfaces stay visually
 * consistent.
 */

const escapeHtml = (value: string) =>
  value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");

const tokenGroups = {
  keyword: new Set([
    "module", "use", "has", "action", "lock", "fn", "where", "const",
    "struct", "enum", "invariant",
  ]),
  cellKind: new Set(["resource", "shared", "receipt", "flow"]),
  cellEffect: new Set([
    "consume", "create", "destroy", "preserve", "create_unique",
    "destroy_unique", "destroy_singleton_type", "claim", "settle",
    "launch", "read_ref",
  ]),
  assertion: new Set([
    "assert", "assert_invariant", "assert_sum", "assert_delta",
    "assert_unique", "require",
  ]),
  control: new Set([
    "if", "else", "for", "in", "while", "match", "return", "let",
    "mut", "ref", "transition", "read", "protected", "witness",
    "lock_args",
  ]),
  builtin: new Set([
    "u8", "u16", "u32", "u64", "u128", "bool", "Address", "Hash",
    "String", "Vec", "env", "witness",
  ]),
  capability: new Set([
    "store", "create", "consume", "replace", "burn", "relock",
    "read_ref", "transfer", "destroy", "retarget_type",
  ]),
};

export const classifyCellToken = (token: string): string => {
  if (token.startsWith("//")) return "comment";
  if (token.startsWith("\"")) return "string";
  if (/^\d+$/.test(token)) return "number";
  if (token === "->" || token === "=>") return "arrow";
  if (/^[{}()[\];:,.<>+\-*/=&|!]$/.test(token) || ["==", "!=", "<=", ">=", "&&", "||"].includes(token))
    return "operator";
  const base = token.includes("::") ? (token.split("::").at(-1) ?? token) : token;
  if (tokenGroups.cellKind.has(base)) return "cell-kind";
  if (tokenGroups.cellEffect.has(base)) return "cell-effect";
  if (tokenGroups.assertion.has(base)) return "assert";
  if (tokenGroups.control.has(base)) return "control";
  if (tokenGroups.builtin.has(base)) return "builtin-type";
  if (tokenGroups.capability.has(base)) return "capability";
  if (tokenGroups.keyword.has(base)) return "keyword";
  return "";
};

/** Highlight a single source line into HTML with token spans. */
export const renderLine = (line: string): string => {
  const tokenPattern =
    /\/\/.*$|"(?:[^"\\]|\\.)*"|->|=>|==|!=|<=|>=|&&|\|\||[A-Za-z_][A-Za-z0-9_:]*|\d+|[{}()[\];:,.<>+\-*/=&|!]/g;
  let rendered = "";
  let cursor = 0;

  for (const match of line.matchAll(tokenPattern)) {
    const token = match[0];
    const index = match.index ?? 0;
    rendered += escapeHtml(line.slice(cursor, index));
    const tokenClass = classifyCellToken(token);
    rendered += tokenClass
      ? `<span class="token-${tokenClass}">${escapeHtml(token)}</span>`
      : escapeHtml(token);
    cursor = index + token.length;
  }

  return rendered + escapeHtml(line.slice(cursor));
};

/** Highlight a full source string (multiple lines) into HTML. */
export const renderSource = (source: string): string =>
  source.split("\n").map(renderLine).join("\n");
