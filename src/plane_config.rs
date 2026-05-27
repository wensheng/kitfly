use std::{fs, path::Path};

pub const PLANE_CONFIG_FILE: &str = "planes.cfg";

#[derive(Debug, Clone, PartialEq)]
pub struct PlaneCatalog {
    pub planes: Vec<PlaneDefinition>,
}

impl PlaneCatalog {
    pub fn load_from_assets(asset_path: &Path) -> Result<Self, String> {
        let config_path = asset_path.join(PLANE_CONFIG_FILE);
        let contents = fs::read_to_string(&config_path)
            .map_err(|error| format!("{}: {error}", config_path.display()))?;
        Self::parse(&contents)
    }

    pub fn parse(contents: &str) -> Result<Self, String> {
        let mut planes = Vec::new();
        let mut current: Option<PlaneBuilder> = None;

        for (line_index, raw_line) in contents.lines().enumerate() {
            let line_number = line_index + 1;
            let line = raw_line
                .split_once('#')
                .map_or(raw_line, |(before_comment, _)| before_comment)
                .trim();
            if line.is_empty() {
                continue;
            }

            if line == "[plane]" {
                if let Some(builder) = current.take() {
                    push_plane(&mut planes, builder.build(line_number)?)?;
                }
                current = Some(PlaneBuilder::default());
                continue;
            }

            let Some(builder) = current.as_mut() else {
                return Err(format!(
                    "line {line_number}: expected [plane] before settings"
                ));
            };
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| format!("line {line_number}: expected key = value"))?;
            builder.set(key.trim(), clean_value(value), line_number)?;
        }

        if let Some(builder) = current {
            push_plane(&mut planes, builder.build(contents.lines().count())?)?;
        }
        if planes.is_empty() {
            return Err("plane config did not define any [plane] blocks".to_string());
        }

        Ok(Self { planes })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaneDefinition {
    pub name: String,
    pub asset: String,
    pub scale: f32,
    pub rotation_xyz: [f32; 3],
    pub translation: [f32; 3],
    pub propeller: PlanePropeller,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlanePropeller {
    None,
    Node {
        name: String,
        radians_per_second: f32,
    },
    Overlay {
        translation: [f32; 3],
        radians_per_second: f32,
    },
}

#[derive(Debug, Default)]
struct PlaneBuilder {
    name: Option<String>,
    asset: Option<String>,
    scale: Option<f32>,
    translation: Option<[f32; 3]>,
    pitch_degrees: Option<f32>,
    direction_degrees: Option<f32>,
    roll_degrees: Option<f32>,
    propeller: Option<PropellerSpec>,
    propeller_translation: Option<[f32; 3]>,
    propeller_speed: Option<f32>,
}

impl PlaneBuilder {
    fn set(&mut self, key: &str, value: &str, line_number: usize) -> Result<(), String> {
        match key {
            "name" => self.name = Some(required_string(value, line_number, key)?),
            "asset" => self.asset = Some(required_string(value, line_number, key)?),
            "scale" => self.scale = Some(parse_f32(value, line_number, key)?),
            "translation" => self.translation = Some(parse_vec3(value, line_number, key)?),
            "pitch_degrees" => self.pitch_degrees = Some(parse_f32(value, line_number, key)?),
            "direction" | "direction_degrees" => {
                self.direction_degrees = Some(parse_f32(value, line_number, key)?);
            }
            "roll_degrees" => self.roll_degrees = Some(parse_f32(value, line_number, key)?),
            "propeller" => self.propeller = Some(parse_propeller(value, line_number)?),
            "propeller_translation" => {
                self.propeller_translation = Some(parse_vec3(value, line_number, key)?);
            }
            "propeller_speed" => {
                self.propeller_speed = Some(parse_f32(value, line_number, key)?);
            }
            _ => return Err(format!("line {line_number}: unknown plane setting `{key}`")),
        }
        Ok(())
    }

    fn build(self, line_number: usize) -> Result<PlaneDefinition, String> {
        let name = self
            .name
            .ok_or_else(|| format!("line {line_number}: plane is missing `name`"))?;
        let asset = self
            .asset
            .ok_or_else(|| format!("line {line_number}: plane `{name}` is missing `asset`"))?;
        let scale = self
            .scale
            .ok_or_else(|| format!("line {line_number}: plane `{name}` is missing `scale`"))?;
        let translation = self.translation.ok_or_else(|| {
            format!("line {line_number}: plane `{name}` is missing `translation`")
        })?;
        let pitch = self.pitch_degrees.unwrap_or(0.0).to_radians();
        let direction = self.direction_degrees.unwrap_or(0.0).to_radians();
        let roll = self.roll_degrees.unwrap_or(0.0).to_radians();
        let propeller_speed = self.propeller_speed.unwrap_or(42.0);
        let propeller = match self.propeller.unwrap_or(PropellerSpec::None) {
            PropellerSpec::None => PlanePropeller::None,
            PropellerSpec::Node(name) => PlanePropeller::Node {
                name,
                radians_per_second: propeller_speed,
            },
            PropellerSpec::Overlay => PlanePropeller::Overlay {
                translation: self.propeller_translation.unwrap_or([0.0, 0.0, 0.0]),
                radians_per_second: propeller_speed,
            },
        };

        Ok(PlaneDefinition {
            name,
            asset,
            scale,
            rotation_xyz: [pitch, direction, roll],
            translation,
            propeller,
        })
    }
}

#[derive(Debug)]
enum PropellerSpec {
    None,
    Node(String),
    Overlay,
}

fn push_plane(planes: &mut Vec<PlaneDefinition>, plane: PlaneDefinition) -> Result<(), String> {
    if planes.iter().any(|existing| existing.name == plane.name) {
        return Err(format!("duplicate plane name `{}`", plane.name));
    }
    planes.push(plane);
    Ok(())
}

fn clean_value(value: &str) -> &str {
    value
        .trim()
        .trim_matches(|ch| matches!(ch, '"' | '\''))
        .trim()
}

fn required_string(value: &str, line_number: usize, key: &str) -> Result<String, String> {
    if value.is_empty() {
        return Err(format!("line {line_number}: `{key}` must not be empty"));
    }
    Ok(value.to_string())
}

fn parse_f32(value: &str, line_number: usize, key: &str) -> Result<f32, String> {
    value
        .parse::<f32>()
        .map_err(|error| format!("line {line_number}: invalid `{key}` value `{value}`: {error}"))
}

fn parse_vec3(value: &str, line_number: usize, key: &str) -> Result<[f32; 3], String> {
    let parts: Vec<_> = value.split(',').map(str::trim).collect();
    if parts.len() != 3 {
        return Err(format!(
            "line {line_number}: `{key}` must have three comma-separated numbers"
        ));
    }

    Ok([
        parse_f32(parts[0], line_number, key)?,
        parse_f32(parts[1], line_number, key)?,
        parse_f32(parts[2], line_number, key)?,
    ])
}

fn parse_propeller(value: &str, line_number: usize) -> Result<PropellerSpec, String> {
    if value == "none" {
        return Ok(PropellerSpec::None);
    }
    if value == "overlay" {
        return Ok(PropellerSpec::Overlay);
    }
    if let Some(name) = value.strip_prefix("node:") {
        let name = name.trim();
        if name.is_empty() {
            return Err(format!("line {line_number}: propeller node name is empty"));
        }
        return Ok(PropellerSpec::Node(name.to_string()));
    }
    Err(format!(
        "line {line_number}: propeller must be `none`, `overlay`, or `node:<name>`"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plane_config() {
        let catalog = PlaneCatalog::parse(include_str!("../assets/planes.cfg")).unwrap();

        assert!(!catalog.planes.is_empty());
        assert_eq!(catalog.planes[0].name, "plane0");
        assert_eq!(catalog.planes[0].asset, "plane0/scene.gltf");
        assert_eq!(catalog.planes[0].rotation_xyz[1], 0.0);

        for plane in &catalog.planes {
            assert!(!plane.name.is_empty());
            assert!(plane.asset.ends_with(".gltf"));
            assert!(plane.scale.is_finite() && plane.scale > 0.0);
        }
    }

    #[test]
    fn parses_propeller_variants() {
        let catalog = PlaneCatalog::parse(
            r#"
            [plane]
            name = node-plane
            asset = node/scene.gltf
            scale = 1.0
            translation = 0.0, 0.0, 0.0
            propeller = node:prop
            propeller_speed = 12.0

            [plane]
            name = overlay-plane
            asset = overlay/scene.gltf
            scale = 1.0
            translation = 0.0, 0.0, 0.0
            propeller = overlay
            propeller_translation = 1.0, 2.0, 3.0
            "#,
        )
        .unwrap();

        assert!(matches!(
            &catalog.planes[0].propeller,
            PlanePropeller::Node { name, .. } if name == "prop"
        ));
        assert!(matches!(
            catalog.planes[1].propeller,
            PlanePropeller::Overlay {
                translation,
                radians_per_second,
            } if translation == [1.0, 2.0, 3.0] && radians_per_second == 42.0
        ));
    }
}
