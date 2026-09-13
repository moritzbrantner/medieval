import { createHash } from "node:crypto";
import path from "node:path";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";

const toolModule = path.resolve(
  process.argv[2] ?? ".consumer-tools/asset-tooling/src/medieval-character-kit.js",
);
const destination = path.resolve(process.argv[3] ?? "web/generated-assets/characters");

const {
  buildMedievalCharacterKitManifest,
  generateMedievalCharacterKit,
} = await import(pathToFileURL(toolModule).href);

await mkdir(destination, { recursive: true });
const generated = generateMedievalCharacterKit();
const manifest = buildMedievalCharacterKitManifest();

if (generated.length !== 3) throw new Error("expected exactly three initial medieval character assets");
if (manifest.recipeVersion !== "1") throw new Error(`unsupported character recipe version '${manifest.recipeVersion}'`);

for (const asset of generated) {
  const entry = manifest.assets.find((candidate) => candidate.id === `medieval.${asset.archetype}`);
  if (!entry) throw new Error(`manifest is missing '${asset.archetype}'`);
  const outputPath = path.join(destination, entry.fileName);
  await writeFile(outputPath, asset.bytes);
  const written = await readFile(outputPath);
  const writtenSha256 = createHash("sha256").update(written).digest("hex");
  if (writtenSha256 !== entry.sha256) {
    throw new Error(`generated '${asset.archetype}' hash does not match the asset-tooling manifest`);
  }
}

await writeFile(path.join(destination, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`, "utf8");

console.log(
  JSON.stringify({
    status: "generated",
    recipeVersion: manifest.recipeVersion,
    assets: manifest.assets.map(({ id, fileName, sha256 }) => ({ id, fileName, sha256 })),
  }),
);
