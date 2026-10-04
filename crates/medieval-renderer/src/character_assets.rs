use std::collections::BTreeMap;

use bytemuck::{Pod, Zeroable};
use serde::Deserialize;

const SOLDIER_OBJ: &str = include_str!("../assets/characters/soldier.obj");
const ARCHER_OBJ: &str = include_str!("../assets/characters/archer.obj");
const KNIGHT_OBJ: &str = include_str!("../assets/characters/knight.obj");
const MATERIALS_JSON: &str = include_str!("../assets/characters/materials.json");
const MATERIAL_ROLES: [&str; 7] = [
    "cloth-primary",
    "cloth-secondary",
    "skin",
    "leather",
    "wood",
    "steel",
    "string",
];

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct CharacterVertex {
    pub(crate) position_role: [f32; 4],
    pub(crate) normal_padding: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct CharacterPaletteUniform {
    pub(crate) colors: [[f32; 4]; 14],
}

pub(crate) struct CharacterMeshAsset {
    pub(crate) vertices: Vec<CharacterVertex>,
}

pub(crate) struct CharacterAssetPack {
    pub(crate) soldier: CharacterMeshAsset,
    pub(crate) archer: CharacterMeshAsset,
    pub(crate) knight: CharacterMeshAsset,
    pub(crate) palette: CharacterPaletteUniform,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MaterialManifest {
    schema_version: u32,
    recipe_version: String,
    palettes: Vec<MaterialPalette>,
    bindings: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MaterialPalette {
    id: String,
    materials: Vec<MaterialEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MaterialEntry {
    id: String,
    base_color_srgb8: [u8; 4],
}

impl CharacterAssetPack {
    pub(crate) fn load() -> Result<Self, String> {
        let manifest: MaterialManifest = serde_json::from_str(MATERIALS_JSON)
            .map_err(|error| format!("invalid packaged character material manifest: {error}"))?;
        if manifest.schema_version != 1 || manifest.recipe_version != "1" {
            return Err("unsupported packaged character material contract".to_owned());
        }
        let palette = CharacterPaletteUniform {
            colors: palette_colors(&manifest)?,
        };
        let soldier = load_character_mesh("soldier", SOLDIER_OBJ, binding(&manifest, "soldier")?)?;
        let archer = load_character_mesh("archer", ARCHER_OBJ, binding(&manifest, "archer")?)?;
        let knight = load_character_mesh("knight", KNIGHT_OBJ, binding(&manifest, "knight")?)?;
        Ok(Self {
            soldier,
            archer,
            knight,
            palette,
        })
    }
}

fn binding<'a>(
    manifest: &'a MaterialManifest,
    archetype: &str,
) -> Result<&'a BTreeMap<String, String>, String> {
    manifest
        .bindings
        .get(archetype)
        .ok_or_else(|| format!("character material manifest is missing {archetype} bindings"))
}

fn palette_colors(manifest: &MaterialManifest) -> Result<[[f32; 4]; 14], String> {
    let mut colors = [[0.0; 4]; 14];
    for (side_index, palette_id) in ["burgundy", "azure"].iter().enumerate() {
        let palette = manifest
            .palettes
            .iter()
            .find(|palette| palette.id == *palette_id)
            .ok_or_else(|| {
                format!("character material manifest is missing {palette_id} palette")
            })?;
        for (role_index, role) in MATERIAL_ROLES.iter().enumerate() {
            let material = palette
                .materials
                .iter()
                .find(|material| material.id == *role)
                .ok_or_else(|| {
                    format!("character palette {palette_id} is missing material role {role}")
                })?;
            colors[side_index * MATERIAL_ROLES.len() + role_index] = material
                .base_color_srgb8
                .map(|channel| f32::from(channel) / 255.0);
        }
    }
    Ok(colors)
}

fn load_character_mesh(
    archetype: &str,
    source: &str,
    bindings: &BTreeMap<String, String>,
) -> Result<CharacterMeshAsset, String> {
    let asset = three_d_formats::load_obj(source.as_bytes())
        .map_err(|error| format!("invalid packaged {archetype} geometry: {error}"))?;
    let mut vertices = Vec::new();
    for asset_mesh in asset.meshes() {
        let group = asset_mesh
            .name()
            .ok_or_else(|| format!("{archetype} mesh has no material-bound name"))?;
        let material = bindings
            .get(group)
            .ok_or_else(|| format!("{archetype} mesh {group} has no material binding"))?;
        let role = material_role(material)?;
        for primitive in asset_mesh.primitives() {
            let mesh = primitive.mesh();
            for triangle_index in 0..mesh.triangle_count() {
                let triangle = mesh
                    .triangle(triangle_index)
                    .expect("validated mesh triangle exists");
                let normal = mesh.triangle_normal(triangle_index).ok_or_else(|| {
                    format!("{archetype} mesh {group} contains a degenerate triangle")
                })?;
                for index in triangle.0 {
                    let point = mesh.vertices()[index as usize];
                    vertices.push(CharacterVertex {
                        position_role: [point.x, point.y, point.z, role as f32],
                        normal_padding: [normal.x, normal.y, normal.z, 0.0],
                    });
                }
            }
        }
    }
    if vertices.is_empty() {
        return Err(format!("{archetype} packaged asset contains no triangles"));
    }
    Ok(CharacterMeshAsset { vertices })
}

fn material_role(material: &str) -> Result<usize, String> {
    MATERIAL_ROLES
        .iter()
        .position(|candidate| *candidate == material)
        .ok_or_else(|| format!("unsupported character material role {material}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_character_assets_are_valid_and_distinct() {
        let assets = CharacterAssetPack::load().unwrap();
        assert_eq!(assets.soldier.vertices.len(), 328 * 3);
        assert_eq!(assets.archer.vertices.len(), 320 * 3);
        assert_eq!(assets.knight.vertices.len(), 244 * 3);
        assert_ne!(assets.soldier.vertices, assets.archer.vertices);
        assert_ne!(assets.soldier.vertices, assets.knight.vertices);
    }

    #[test]
    fn faction_palettes_come_from_packaged_material_contract() {
        let assets = CharacterAssetPack::load().unwrap();
        assert_eq!(
            assets.palette.colors[0],
            [116.0 / 255.0, 34.0 / 255.0, 48.0 / 255.0, 1.0]
        );
        assert_eq!(
            assets.palette.colors[7],
            [43.0 / 255.0, 78.0 / 255.0, 132.0 / 255.0, 1.0]
        );
    }
}
