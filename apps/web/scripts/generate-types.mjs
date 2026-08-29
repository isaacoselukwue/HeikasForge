import { readdir, readFile, writeFile, mkdir } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { compile } from "json-schema-to-typescript";

const here = dirname(fileURLToPath(import.meta.url));
const workspaceRoot = resolve(here, "..", "..", "..");
const schemaDirectory = join(workspaceRoot, "schemas");
const outputFile = join(here, "..", "src", "generated", "api-types.ts");

const banner = [
  "/* eslint-disable */",
  "/**",
  " * This file is generated from the Rust JSON schemas by `pnpm generate:types`.",
  " * Do not edit it by hand and do not add a duplicate hand written transport model.",
  " */",
  "",
].join("\n");

async function main() {
  const entries = await readdir(schemaDirectory);
  const schemas = entries.filter((entry) => entry.endsWith(".schema.json")).sort();
  if (schemas.length === 0) {
    throw new Error(`no schema documents were found in ${schemaDirectory}`);
  }
  const sections = [];
  for (const entry of schemas) {
    const raw = await readFile(join(schemaDirectory, entry), "utf8");
    const document = JSON.parse(raw);
    const generated = await compile(document, document.title ?? entry, {
      bannerComment: "",
      additionalProperties: false,
      declareExternallyReferenced: true,
      unreachableDefinitions: false,
      style: { singleQuote: false, printWidth: 100 },
    });
    sections.push(generated.trim());
  }
  const merged = deduplicate(sections.join("\n\n"));
  await mkdir(dirname(outputFile), { recursive: true });
  await writeFile(outputFile, `${banner}\n${merged}\n`, "utf8");
  console.log(`wrote ${outputFile}`);
}

function deduplicate(source) {
  const blocks = source.split(/\n(?=export (?:interface|type) )/g);
  const seen = new Set();
  const kept = [];
  for (const block of blocks) {
    const match = /^export (?:interface|type) ([A-Za-z0-9_]+)/.exec(block.trim());
    if (!match) {
      kept.push(block);
      continue;
    }
    const name = match[1];
    if (seen.has(name)) {
      continue;
    }
    seen.add(name);
    kept.push(block);
  }
  return kept.join("\n").replace(/\n{3,}/g, "\n\n");
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
