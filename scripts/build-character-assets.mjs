import { createHash } from "node:crypto";
import path from "node:path";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";

const geometryModule = path.resolve(
  process.argv[2] ?? ".consumer-tools/asset-tooling/src/medieval-character-kit.js",
);
const materialModule = path.resolve(
  process.argv[3] ?? ".consumer-tools/asset-tooling/src/medieval-character-materials.js",
);
const destination = path.resolve(process.argv[4] ?? "web/generated-assets/characters");

const {
  buildMedievalCharacterKitManifest,
  generateMedievalCharacterKit,
} = await import(pathToFileURL(geometryModule).href);
const {
  buildMedievalCharacterMaterialManifest,
  buildMedievalCharacterPackageManifest,
} = await import(pathToFileURL(materialModule).href);

await mkdir(destination, { recursive: true });
const generated = generateMedievalCharacterKit();
const manifest = buildMedievalCharacterKitManifest();
const materials = buildMedievalCharacterMaterialManifest();
const packageManifest = buildMedievalCharacterPackageManifest();

if (generated.length !== 3) throw new Error("expected exactly three initial medieval character assets");
if (manifest.recipeVersion !== "1") throw new Error(`unsupported character recipe version '${manifest.recipeVersion}'`);
if (materials.recipeVersion !== "1") throw new Error(`unsupported material recipe version '${materials.recipeVersion}'`);
if (packageManifest.packageVersion !== "1") throw new Error(`unsupported character package version '${packageManifest.packageVersion}'`);
if (JSON.stringify(packageManifest.geometry) !== JSON.stringify(manifest)) {
  throw new Error("character package geometry must exactly match the generated geometry manifest");
}
if (JSON.stringify(packageManifest.materials) !== JSON.stringify(materials)) {
  throw new Error("character package materials must exactly match the generated material manifest");
}
if (packageManifest.processingPlan.status !== "intent-only") {
  throw new Error("character package must not claim unproduced processing evidence");
}
if (packageManifest.processingPlan.authority !== "moritzbrantner/3d-lab") {
  throw new Error("character package processing authority must remain moritzbrantner/3d-lab");
}

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

for (const [fileName, document] of [
  ["manifest.json", manifest],
  ["materials.json", materials],
  ["package.json", packageManifest],
]) {
  await writeFile(path.join(destination, fileName), `${JSON.stringify(document, null, 2)}\n`, "utf8");
}

console.log(
  JSON.stringify({
    status: "generated",
    recipeVersion: manifest.recipeVersion,
    materialRecipeVersion: materials.recipeVersion,
    packageVersion: packageManifest.packageVersion,
    palettes: materials.palettes.map(({ id }) => id),
    assets: manifest.assets.map(({ id, fileName, sha256 }) => ({ id, fileName, sha256 })),
  }),
);
