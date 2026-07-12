/**
 * Build a short label for collapsed sidebar project chips.
 *
 * Rules:
 * - CJK names: first two non-space code points
 * - Multi-token (separators / camelCase): first letter of first two tokens
 * - Single token: first two letters, uppercased for Latin
 */
export function projectAbbreviation(name: string): string {
  const trimmed = name.trim();
  if (!trimmed) return "?";

  const nonSpace = Array.from(trimmed).filter((char) => !/\s/u.test(char));
  if (nonSpace.length === 0) return "?";

  if (nonSpace.some(isCjkCodePoint)) {
    return nonSpace.slice(0, 2).join("");
  }

  const tokens = splitNameTokens(trimmed);
  if (tokens.length >= 2) {
    return tokens
      .slice(0, 2)
      .map((token) => firstLetter(token))
      .join("")
      .toUpperCase();
  }

  const word = tokens[0] ?? nonSpace.join("");
  return Array.from(word).slice(0, 2).join("").toUpperCase();
}

function isCjkCodePoint(char: string): boolean {
  const code = char.codePointAt(0);
  if (code === undefined) return false;
  return (
    (code >= 0x3040 && code <= 0x30ff) ||
    (code >= 0x3400 && code <= 0x4dbf) ||
    (code >= 0x4e00 && code <= 0x9fff) ||
    (code >= 0xf900 && code <= 0xfaff)
  );
}

function splitNameTokens(name: string): string[] {
  return name
    .replace(/([a-z\d])([A-Z])/g, "$1 $2")
    .split(/[\s\-_.+/\\]+/u)
    .map((token) => token.trim())
    .filter(Boolean);
}

function firstLetter(token: string): string {
  const [first] = Array.from(token);
  return first ?? "";
}
