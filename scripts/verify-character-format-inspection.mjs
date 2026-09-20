import path from "node:path";
import { readFile } from "node:fs/promises";

const destination = path.resolve(process.argv[2] ?? "web/generated-assets/characters");
const manifest = JSON.parse(await readFile(path.join(destination, "manifest.json"), "utf8"));

if (manifest.schemaVersion !== 1 || manifest.recipeVersion !== "1") {
  throw new Error("unsupported medieval character geometry manifest");
}

const evidence = [];
for (const entry of manifest.assets) {
  const archetype = entry.id.replace(/^medieval\./, "");
  const inspectionPath = path.join(destination, `${archetype}.inspect.json`);
  const inspection = JSON.parse(await readFile(inspectionPath, "utf8"));
  if (inspection.schemaVersion !== 1 || inspection.format !== "obj") {
    throw new Error(`3d-lab inspection for '${archetype}' has an unsupported contract`);
  }
  if (!Number.isInteger(inspection.meshCount) || inspection.meshCount < 1) {
    throw new Error(`3d-lab inspection for '${archetype}' did not produce any meshes`);
  }
  if (!Number.isInteger(inspection.primitiveCount) || inspection.primitiveCount < 1) {
    throw new Error(`3d-lab inspection for '${archetype}' did not produce any primitives`);
  }
  if (inspection.vertexCount !== entry.vertexCount) {
    throw new Error(
      `3d-lab vertex count for '${archetype}' (${inspection.vertexCount}) does not match asset-tooling (${entry.vertexCount})`,
    );
  }
  if (inspection.triangleCount !== entry.triangleCount) {
    throw new Error(
      `3d-lab triangle count for '${archetype}' (${inspection.triangleCount}) does not match asset-tooling (${entry.triangleCount})`,
    );
  }
  evidence.push({
    id: entry.id,
    meshCount: inspection.meshCount,
    primitiveCount: inspection.primitiveCount,
    vertexCount: inspection.vertexCount,
    triangleCount: inspection.triangleCount,
  });
}

console.log(JSON.stringify({ status: "verified", authority: "moritzbrantner/3d-lab", assets: evidence }));
