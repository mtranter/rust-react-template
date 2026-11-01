// Spec §8: features never import each other, and code outside a feature imports
// it only through its `index.ts` (`@/features/notes`, never a file inside it).
// A script because Biome 1.9's `noRestrictedImports` matches exact names only.
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, posix, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const src = join(dirname(fileURLToPath(import.meta.url)), "..", "src");
const specifier = /(?:\bfrom\s*|\bimport\s*\(?\s*)["']([^"']+)["']/g;

const toSrc = (path) => relative(src, path).split(sep).join("/");

function resolve(from, spec) {
  if (spec.startsWith("@/")) return posix.normalize(spec.slice(2));
  if (spec.startsWith(".")) return posix.normalize(posix.join(posix.dirname(from), spec));
  return null;
}

const featureOf = (path) => path.match(/^features\/([^/]+)(?:\/(.*))?$/);

const failures = [];
for (const entry of readdirSync(src, { recursive: true })) {
  const file = toSrc(join(src, entry));
  if (!/\.tsx?$/.test(file) || file.startsWith("api/")) continue;
  const own = featureOf(file)?.[1];
  for (const [, spec] of readFileSync(join(src, file), "utf8").matchAll(specifier)) {
    const target = featureOf(resolve(file, spec) ?? "");
    if (!target || target[1] === own) continue;
    const [, feature, inner] = target;
    if (own) {
      failures.push(`src/${file}: imports "${spec}"; features never import each other`);
    } else if (inner && !/^index(\.tsx?)?$/.test(inner)) {
      failures.push(`src/${file}: imports "${spec}"; import "@/features/${feature}" instead`);
    }
  }
}

if (failures.length > 0) {
  console.error(failures.join("\n"));
  console.error("feature import check failed (spec §8)");
  process.exit(1);
}
console.log("feature imports ok");
