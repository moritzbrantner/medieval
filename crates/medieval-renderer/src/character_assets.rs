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
        let soldier = parse_obj("soldier", SOLDIER_OBJ, binding(&manifest, "soldier")?)?;
        let archer = parse_obj("archer", ARCHER_OBJ, binding(&manifest, "archer")?)?;
        let knight = parse_obj("knight", KNIGHT_OBJ, binding(&manifest, "knight")?)?;
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
            .ok_or_else(|| format!("character material manifest is missing {palette_id} palette"))?;
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

fn parse_obj(
    archetype: &str,
    source: &str,
    bindings: &BTreeMap<String, String>,
) -> Result<CharacterMeshAsset, String> {
    let mut positions = Vec::<[f32; 3]>::new();
    let mut vertices = Vec::<CharacterVertex>::new();
    let mut active_role = None::<usize>;

    for (line_number, line) in source.lines().enumerate() {
        if let Some(group) = line.strip_prefix("g ") {
            let material = bindings.get(group).ok_or_else(|| {
                format!(
                    "{archetype} OBJ group {group} has no material binding at line {}",
                    line_number + 1
                )
            })?;
            active_role = Some(material_role(material)?);
        } else if let Some(position) = line.strip_prefix("v ") {
            let values = position
                .split_ascii_whitespace()
                .map(str::parse::<f32>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| {
                    format!(
                        "invalid {archetype} OBJ vertex at line {}: {error}",
                        line_number + 1
                    )
                })?;
            let [x, y, z] = values.as_slice() else {
                return Err(format!(
                    "{archetype} OBJ vertex at line {} must have three coordinates",
                    line_number + 1
                ));
            };
            positions.push([*x, *y, *z]);
        } else if let Some(face) = line.strip_prefix("f ") {
            let role = active_role.ok_or_else(|| {
                format!(
                    "{archetype} OBJ face precedes a material-bound group at line {}",
                    line_number + 1
                )
            })?;
            let indices = face
                .split_ascii_whitespace()
                .map(|token| {
                    token
                        .split('/')
                        .next()
                        .ok_or_else(|| "missing OBJ vertex index".to_owned())?
                        .parse::<usize>()
                        .map_err(|error| format!("invalid OBJ vertex index: {error}"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let [a, b, c] = indices.as_slice() else {
                return Err(format!(
                    "{archetype} OBJ face at line {} must be a triangle",
                    line_number + 1
                ));
            };
            let triangle = [*a, *b, *c].map(|index| {
                positions.get(index.saturating_sub(1)).copied().ok_or_else(|| {
                    format!(
                        "{archetype} OBJ face at line {} references vertex {index} out of range",
                        line_number + 1
                    )
                })
            });
            let [a, b, c] = [
                triangle[0].clone()?,
                triangle[1].clone()?,
                triangle[2].clone()?,
            ];
            let normal = triangle_normal(a, b, c)?;
            for position in [a, b, c] {
                vertices.push(CharacterVertex {
                    position_role: [
                        position[0],
                        position[1],
                        position[2],
                        role as f32,
                    ],
                    normal_padding: [normal[0], normal[1], normal[2], 0.0],
                });
            }
        }
    }

    if vertices.is_empty() {
        return Err(format!("{archetype} packaged OBJ contains no triangles"));
    }
    Ok(CharacterMeshAsset { vertices })
}

fn material_role(material: &str) -> Result<usize, String> {
    MATERIAL_ROLES
        .iter()
        .position(|candidate| *candidate == material)
        .ok_or_else(|| format!("unsupported character material role {material}"))
}

fn triangle_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Result<[f32; 3], String> {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let normal = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    if !length.is_finite() || length <= f32::EPSILON {
        return Err("packaged character OBJ contains a degenerate triangle".to_owned());
    }
    Ok([
        normal[0] / length,
        normal[1] / length,
        normal[2] / length,
    ])
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
