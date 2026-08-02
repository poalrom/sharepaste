#!/usr/bin/env node
// Refuses to let a release go out on three FUI palettes that disagree.
//
//   node .github/scripts/check-tokens.mjs [repo-root]
//
// Exits 1 when a token one client declares has a different value on another,
// and prints the token, all three values, and what this check does not cover.
//
// `docs/android-redesign.md` §8 recorded the risk with two copies: "Nothing
// checks that they agree… a token changed on one client and not the other is a
// silent divergence." A third copy is where 45 tokens stop being reviewable by
// eye, and ADR 0010 chose three hand-written copies over a generated one — so
// this gate is what keeps that choice honest rather than merely cheap.
//
// **Colours only.** See the failure output, which says so where it will be
// read.

import { readFileSync } from "node:fs";
import { join } from "node:path";

const CSS = ["clients", "desktop", "ui", "src", "styles.css"];
const KOTLIN = [
  "clients", "mobile", "android", "app", "src", "main", "kotlin",
  "com", "sharepaste", "android", "ui", "Fui.kt",
];
const SWIFT = ["clients", "mobile", "ios", "Sources", "Sharepaste", "Fui.swift"];

/**
 * The name three differently-spelled declarations share.
 *
 * `--text-body`, `TextBody` and `textBody` are one token, and the only thing
 * they have in common is their letters and digits in order. So that is the key:
 * everything else stripped, lower-cased. It reads as crude and it is exactly
 * strong enough — `--cyan-a08` and `CyanA08` land on `cyana08`, `--void-1000`
 * and `Void1000` on `void1000`, without a per-client table of spellings that
 * would itself need keeping in step.
 *
 * What it cannot do is notice a **rename**: Android's `CyanA24` and the CSS
 * `--cyan-a20` are two names, so they are two tokens, and neither is compared
 * against anything. That is deliberate — a client is allowed chrome the others
 * do not have — but it is why the count of uncompared declarations is part of
 * the output rather than a detail.
 */
function key(name) {
  return name.replace(/[^A-Za-z0-9]/g, "").toLowerCase();
}

/**
 * `#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb(…)` and `rgba(…)` as one `AARRGGBB`.
 *
 * Everything is normalised onto the eight-digit form Compose and SwiftUI use,
 * alpha first, because that is the only shape all three can be compared in:
 * CSS carries alpha last when it carries it at all, and carries it as a
 * fraction inside `rgba()`.
 *
 * `Math.round` on that fraction, never `Math.floor`: `rgba(…, 0.12)` is `0x1F`
 * — 30.6 rounds to 31 — and that is the byte Android's `CyanA12` holds.
 * Flooring would put every alpha token one off and fail the whole ramp on the
 * first run.
 *
 * Answers `null` for anything that is not a colour at all — a font stack, a
 * duration, a `polygon()` — which is how the CSS file's ninety-odd non-colour
 * custom properties stay out of this.
 */
function cssColour(value) {
  const hex = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.exec(value);
  if (hex) {
    const digits = hex[1];
    if (digits.length === 3) {
      const [r, g, b] = digits;
      return `FF${r}${r}${g}${g}${b}${b}`.toUpperCase();
    }
    if (digits.length === 6) return `FF${digits}`.toUpperCase();
    return `${digits.slice(6)}${digits.slice(0, 6)}`.toUpperCase();
  }

  const rgb = /^rgba?\(([^)]*)\)$/.exec(value);
  if (!rgb) return null;
  const parts = rgb[1].split(",").map((part) => part.trim());
  if (parts.length < 3 || parts.length > 4) return null;
  const channels = parts.slice(0, 3).map(Number);
  if (channels.some((channel) => !Number.isInteger(channel) || channel < 0 || channel > 255)) {
    return null;
  }
  const alpha = parts.length === 4 ? Number(parts[3]) : 1;
  if (!Number.isFinite(alpha) || alpha < 0 || alpha > 1) return null;
  const byte = (n) => n.toString(16).padStart(2, "0").toUpperCase();
  return `${byte(Math.round(alpha * 255))}${channels.map(byte).join("")}`;
}

/**
 * The `Color(0xAARRGGBB)` literal Kotlin and Swift both write.
 *
 * Eight digits or nothing. A six-digit literal is a real bug rather than a
 * shorthand — `Color(0x04080C)` is a fully *transparent* colour in Compose and
 * in the `Color(_ argb:)` initialiser this project's Swift uses — so it is
 * reported rather than quietly padded to opaque.
 */
function argbLiteral(value, where) {
  const match = /^Color\(0x([0-9a-fA-F]+)\)$/.exec(value);
  if (!match) return null;
  if (match[1].length !== 8) {
    fail(
      `${where} is 0x${match[1]}: an ARGB literal is eight digits, and a six-digit one is\n` +
        "  a fully transparent colour rather than an opaque shorthand.",
    );
  }
  return match[1].toUpperCase();
}

/**
 * One client's declarations, with every alias followed to the colour it means.
 *
 * All three files alias — `--text-emitter: var(--cyan-300)`,
 * `val TextEmitter = Cyan300` — and an alias left unresolved would drop a token
 * out of the comparison silently rather than showing up as a disagreement,
 * which is the one failure mode this whole script exists to prevent.
 *
 * An alias whose target is not a colour resolves to nothing and is dropped: it
 * is not a palette entry.
 */
function resolvePalette(client, declarations, spellings) {
  const resolved = new Map();
  for (const [name] of declarations) {
    const seen = new Set();
    let at = name;
    let value = null;
    while (at !== undefined) {
      if (seen.has(at)) {
        fail(`${client}: ${spellings.get(at)} is defined in terms of itself.`);
      }
      seen.add(at);
      const entry = declarations.get(at);
      if (entry === undefined) break; // An alias of something that is not a colour.
      if (entry.colour !== null) {
        value = entry.colour;
        break;
      }
      at = entry.alias;
    }
    if (value !== null) resolved.set(name, value);
  }
  return resolved;
}

/**
 * The CSS custom properties, from the one `:root` block.
 *
 * Bounded to `:root` rather than swept from the whole file: the components
 * layer below sets custom properties inside rules, and a value that only
 * applies to one hovered element is not a palette token.
 *
 * A name declared twice is fatal, for the reason `check-versions.mjs` treats
 * two `versionName` matches as fatal: which one ships is then a cascade
 * question, and this gate would be reading a coin toss.
 */
function cssPalette(source) {
  const root = /:root\s*\{([\s\S]*?)\n\}/.exec(source);
  if (!root) fail("clients/desktop/ui/src/styles.css has no :root block to read a palette from.");

  const declarations = new Map();
  const spellings = new Map();
  for (const [, name, raw] of root[1].matchAll(/^\s*--([a-zA-Z0-9-]+)\s*:\s*([^;]+);/gm)) {
    const value = raw.trim();
    const literal = value.startsWith("#") || value.startsWith("rgb");
    const alias = /^var\(--([a-zA-Z0-9-]+)\)$/.exec(value);
    if (!literal && !alias) continue; // A font stack, a duration, a clip path.

    const colour = literal ? cssColour(value) : null;
    if (literal && colour === null) {
      fail(
        `clients/desktop/ui/src/styles.css: --${name} looks like a colour and does not parse: ${value}`,
      );
    }
    const at = key(name);
    if (declarations.has(at)) {
      fail(
        `clients/desktop/ui/src/styles.css declares --${name} twice; which one ships is a cascade question.`,
      );
    }
    declarations.set(at, { colour, alias: alias ? key(alias[1]) : undefined });
    spellings.set(at, `--${name}`);
  }
  return {
    palette: resolvePalette("clients/desktop/ui/src/styles.css", declarations, spellings),
    spellings,
  };
}

/**
 * The declarations inside one brace-delimited block, for the two files that
 * keep their palette in one.
 *
 * `object Fui {` … and `enum Fui {` … are sliced out first because both files
 * carry same-shaped declarations outside it — Compose reads
 * `val step = 32.dp.toPx()` inside a `drawBehind`, and a `static let` in a view
 * is not a token. The block ends at the first `}` in column zero, which is what
 * both files' formatting guarantees and what neither file's palette contains.
 */
function blockPalette(path, source, opener, declaration) {
  const start = source.indexOf(opener);
  if (start < 0) fail(`${path} has no \`${opener.trim()}\` to read a palette from.`);
  const end = source.indexOf("\n}", start);
  const block = source.slice(start, end < 0 ? source.length : end);

  const declarations = new Map();
  const spellings = new Map();
  for (const [, name, raw] of block.matchAll(declaration)) {
    const value = raw.trim();
    const alias = /^[A-Za-z_][A-Za-z0-9_]*$/.test(value);
    if (!value.startsWith("Color(") && !alias) continue; // A Dp, a TextStyle, a Font.

    const colour = alias ? null : argbLiteral(value, `${path}: ${name}`);
    if (!alias && colour === null) {
      fail(
        `${path}: ${name} is a Color this gate cannot read: ${value}\n` +
          "  Every palette entry is a literal `Color(0xAARRGGBB)` or the name of another entry.",
      );
    }
    const at = key(name);
    if (declarations.has(at)) fail(`${path} declares ${name} twice.`);
    declarations.set(at, { colour, alias: alias ? key(value) : undefined });
    spellings.set(at, name);
  }
  return { palette: resolvePalette(path, declarations, spellings), spellings };
}

/**
 * One palette file, or a failure that says why a missing one is not survivable.
 *
 * Not skipped when absent. A client whose colours nothing is checking is the
 * state this gate exists to end, and a check that quietly compared two of the
 * three palettes would report exactly the same green as one that compared all
 * three.
 */
function read(root, parts) {
  const path = parts.join("/");
  try {
    return readFileSync(join(root, ...parts), "utf8");
  } catch (error) {
    fail(
      `${path} could not be read (${error.code}).\n` +
        "  All three palettes are hand-written and all three are required: a missing one is\n" +
        "  a client whose colours nothing is checking.",
    );
  }
}

function fail(message) {
  console.error(message);
  process.exit(1);
}

export function collectPalettes(root) {
  return {
    "clients/desktop/ui/src/styles.css": cssPalette(read(root, CSS)),
    "clients/mobile/android/…/ui/Fui.kt": blockPalette(
      KOTLIN.join("/"),
      read(root, KOTLIN),
      "object Fui {",
      /^\s*val\s+([A-Za-z0-9_]+)\s*=\s*(.+?)\s*$/gm,
    ),
    "clients/mobile/ios/…/Sharepaste/Fui.swift": blockPalette(
      SWIFT.join("/"),
      read(root, SWIFT),
      "enum Fui {",
      /^\s*static let\s+([A-Za-z0-9_]+)\s*=\s*(.+?)\s*$/gm,
    ),
  };
}

if (import.meta.filename === process.argv[1]) {
  const root = process.argv[2] ?? ".";
  const clients = collectPalettes(root);
  const names = Object.keys(clients);
  const column = Math.max(...names.map((name) => name.length));

  const everyToken = [
    ...new Set(names.flatMap((name) => [...clients[name].palette.keys()])),
  ].sort();

  const disagreements = [];
  // Keyed by client, because that is how the list reads: "these are the tokens
  // only the desktop declares" is a sentence a reader can act on, where a flat
  // list of names is not.
  const uncompared = Object.fromEntries(names.map((name) => [name, []]));
  let compared = 0;
  for (const token of everyToken) {
    const declaring = names.filter((name) => clients[name].palette.has(token));
    if (declaring.length === 1) {
      const only = declaring[0];
      uncompared[only].push(clients[only].spellings.get(token) ?? token);
      continue;
    }
    compared += 1;
    const values = new Set(declaring.map((name) => clients[name].palette.get(token)));
    if (values.size > 1) disagreements.push(token);
  }

  /**
   * The tokens nothing compares, named.
   *
   * Printed on the way out of both paths, because absence is the hole this gate
   * cannot close and a reader has to be able to see it. A token one client
   * *stops* declaring does not fail anything — it simply drops out of the
   * comparison and lands in this list — and neither does a token one client
   * renames, which becomes two entries here rather than one disagreement.
   *
   * It cannot be a gate. Ten of these are legitimate today: a client is allowed
   * chrome the others do not have, and there is nothing in three text files
   * that distinguishes a deliberate one from an accidental one. So the answer
   * is to print them rather than to guess, and the list is short enough to read.
   */
  function reportUncompared(write) {
    const total = Object.values(uncompared).reduce((sum, list) => sum + list.length, 0);
    if (total === 0) return;
    write("");
    write(`${total} declarations are made by one client only, so nothing compares them:`);
    for (const name of names) {
      if (uncompared[name].length === 0) continue;
      write(`  ${name.padEnd(column)}  ${uncompared[name].join(", ")}`);
    }
    write("This list is not a gate and cannot be one: a client is allowed chrome the others");
    write("do not have, and nothing here tells a deliberate omission from a token somebody");
    write("stopped declaring. Read it — an entry that has a near-twin on another client is");
    write("either a rename or a divergence, and only a person can say which.");
  }

  // Nothing in common is a parser that has stopped working, not three palettes
  // that happen to be disjoint. Without this the gate would go green on the day
  // one file's formatting changed under it.
  if (compared === 0) {
    fail(
      "No token is declared by more than one client, so nothing was compared.\n" +
        "  That is a parsing failure rather than a palette that happens to be disjoint.",
    );
  }

  if (disagreements.length > 0) {
    console.error("The FUI palettes disagree; refusing to publish.");
    console.error("");
    for (const token of disagreements) {
      console.error(`  ${token}`);
      for (const name of names) {
        const spelling = clients[name].spellings.get(token);
        const value = clients[name].palette.get(token);
        console.error(
          `    ${name.padEnd(column)}  ${value ? `0x${value}` : "(not declared here)"}` +
            `${spelling ? `  as ${spelling}` : ""}`,
        );
      }
      console.error("");
    }
    // Said here rather than in a comment somebody would have to go and find,
    // because this is the moment a reader forms a belief about what the check
    // covers.
    console.error("This gate compares colours and nothing else.");
    console.error("A panel with a different corner radius, a chrome band a few points taller, a");
    console.error("row height that drifted — all of those pass it cleanly. Geometry drift is");
    console.error("caught by looking at the device; a wrong hex digit never is. A green check");
    console.error("does not mean the three clients match.");
    reportUncompared(console.error);
    process.exit(1);
  }

  console.log(`${compared} tokens agree across the clients that declare them.`);
  console.log("Colours only — geometry is not covered by this check.");
  reportUncompared(console.log);
}
