use std::{
    collections::{HashMap, HashSet},
    f32::consts::PI,
    path::Path,
};

use bevy::{
    asset::AssetPlugin, camera::RenderTarget, core_pipeline::tonemapping::Tonemapping,
    ecs::hierarchy::ChildSpawnerCommands, prelude::*, render::RenderPlugin, window::ExitCondition,
    winit::WinitPlugin, world_serialization::WorldInstanceReady,
};

use crate::{
    controls::FlightState,
    plane_config::{PLANE_CONFIG_FILE, PlaneCatalog, PlaneDefinition, PlanePropeller},
};

#[derive(Component)]
pub struct KitflyCamera;

#[derive(Component)]
struct AirplaneRig;

#[derive(Component)]
struct AirplaneModel;

#[derive(Component)]
struct OverlayPropeller;

#[derive(Component)]
struct Propeller {
    radians_per_second: f32,
}

const CHUNK_SIZE: f32 = 32.0;
const CHUNK_RADIUS: i32 = 3;
const TERRAIN_CELLS_PER_CHUNK: i32 = 4;
const TERRAIN_CELL_SIZE: f32 = CHUNK_SIZE / TERRAIN_CELLS_PER_CHUNK as f32;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TerrainChunkCoord {
    x: i32,
    z: i32,
}

#[derive(Resource, Default)]
struct TerrainWorld {
    chunks: HashMap<TerrainChunkCoord, Entity>,
    next_seed: u32,
}

#[derive(Resource, Clone)]
struct TerrainAssets {
    block_mesh: Handle<Mesh>,
    tree_trunk_mesh: Handle<Mesh>,
    tree_leaf_mesh: Handle<Mesh>,
    cloud_mesh: Handle<Mesh>,
    grass_materials: [Handle<StandardMaterial>; 3],
    mountain_material: Handle<StandardMaterial>,
    snow_material: Handle<StandardMaterial>,
    trunk_material: Handle<StandardMaterial>,
    leaves_material: Handle<StandardMaterial>,
    cloud_material: Handle<StandardMaterial>,
}

#[derive(Debug, Clone, Resource, PartialEq)]
pub struct PlaneSelection {
    index: usize,
    catalog: PlaneCatalog,
}

#[derive(Debug, Clone, Resource, PartialEq, Eq)]
struct AppliedPlaneSelection {
    index: usize,
}

impl PlaneSelection {
    fn load(asset_path: &Path) -> Self {
        let catalog = PlaneCatalog::load_from_assets(asset_path).unwrap_or_else(|error| {
            panic!("failed to load {PLANE_CONFIG_FILE}: {error}");
        });
        Self::from_catalog(catalog)
    }

    fn from_catalog(catalog: PlaneCatalog) -> Self {
        assert!(
            !catalog.planes.is_empty(),
            "plane catalog must contain at least one plane"
        );
        Self { index: 0, catalog }
    }

    pub fn next(&mut self) {
        self.index = (self.index + 1) % self.catalog.planes.len();
    }

    pub fn current_name(&self) -> &str {
        &self.definition().name
    }

    fn definition(&self) -> &PlaneDefinition {
        &self.catalog.planes[self.index % self.catalog.planes.len()]
    }
}

impl AppliedPlaneSelection {
    fn from_selection(selection: &PlaneSelection) -> Self {
        Self {
            index: selection.index,
        }
    }

    fn is_current(&self, selection: &PlaneSelection) -> bool {
        self.index == selection.index
    }

    fn apply(&mut self, selection: &PlaneSelection) {
        self.index = selection.index;
    }
}

pub fn configure_app(app: &mut App) {
    let asset_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let plane_selection = PlaneSelection::load(&asset_path);
    let applied_plane_selection = AppliedPlaneSelection::from_selection(&plane_selection);
    app.insert_resource(ClearColor(Color::srgb_u8(107, 182, 232)))
        .insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.78, 0.86, 1.0),
            brightness: 450.0,
            affects_lightmapped_meshes: true,
        })
        .insert_resource(FlightState::default())
        .insert_resource(plane_selection)
        .insert_resource(applied_plane_selection)
        .add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                })
                .set(RenderPlugin {
                    synchronous_pipeline_compilation: true,
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: asset_path.to_string_lossy().into_owned(),
                    ..default()
                })
                .disable::<WinitPlugin>(),
        )
        .add_systems(Startup, setup_scene)
        .add_systems(
            Update,
            (
                drive_flight,
                update_terrain,
                apply_plane_selection,
                spin_propellers,
            )
                .chain(),
        )
        .add_observer(tag_configured_propeller_node);
}

pub fn spawn_camera(world: &mut World, render_target: RenderTarget) {
    let camera_transform = world.resource::<FlightState>().pose().camera;
    world.spawn((
        Camera3d::default(),
        Camera {
            clear_color: Color::srgb_u8(107, 182, 232).into(),
            ..default()
        },
        render_target,
        Tonemapping::None,
        camera_transform,
        KitflyCamera,
    ));
}

fn setup_scene(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    selection: Res<PlaneSelection>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let meadow_grass = materials.add(StandardMaterial {
        base_color: Color::srgb_u8(77, 154, 72),
        perceptual_roughness: 0.92,
        ..default()
    });
    let bright_grass = materials.add(StandardMaterial {
        base_color: Color::srgb_u8(93, 171, 80),
        perceptual_roughness: 0.9,
        ..default()
    });
    let dark_grass = materials.add(StandardMaterial {
        base_color: Color::srgb_u8(50, 128, 73),
        perceptual_roughness: 0.94,
        ..default()
    });
    let mountain_material = materials.add(StandardMaterial {
        base_color: Color::srgb_u8(91, 103, 91),
        perceptual_roughness: 0.96,
        ..default()
    });
    let snow_material = materials.add(StandardMaterial {
        base_color: Color::srgb_u8(222, 229, 226),
        perceptual_roughness: 0.86,
        ..default()
    });
    let trunk_material = materials.add(StandardMaterial {
        base_color: Color::srgb_u8(99, 69, 42),
        perceptual_roughness: 0.9,
        ..default()
    });
    let leaves_material = materials.add(StandardMaterial {
        base_color: Color::srgb_u8(37, 108, 63),
        perceptual_roughness: 0.85,
        ..default()
    });
    let cloud_material = materials.add(StandardMaterial {
        base_color: Color::srgba_u8(248, 250, 255, 220),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    let terrain_assets = TerrainAssets {
        block_mesh: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        tree_trunk_mesh: meshes.add(Cuboid::new(0.7, 2.4, 0.7)),
        tree_leaf_mesh: meshes.add(Cuboid::new(2.6, 1.8, 2.6)),
        cloud_mesh: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        grass_materials: [meadow_grass, bright_grass, dark_grass],
        mountain_material,
        snow_material,
        trunk_material,
        leaves_material,
        cloud_material,
    };
    let mut terrain_world = TerrainWorld::default();
    sync_terrain_chunks(
        &mut commands,
        &terrain_assets,
        &mut terrain_world,
        FlightState::default().position,
    );
    commands.insert_resource(terrain_assets);
    commands.insert_resource(terrain_world);

    commands.spawn((
        DirectionalLight {
            illuminance: 9000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::ZYX,
            -PI / 5.0,
            -PI / 5.5,
            -PI / 4.0,
        )),
    ));

    let propeller_material = materials.add(StandardMaterial {
        base_color: Color::srgba_u8(28, 34, 42, 210),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let propeller_blade = meshes.add(Cuboid::new(1.1, 0.07, 0.03));

    let airplane_pose = FlightState::default().pose().airplane;
    let plane = selection.definition();
    let (overlay_transform, overlay_visibility, overlay_speed) = overlay_propeller_state(plane);
    commands
        .spawn((
            airplane_pose,
            Visibility::Inherited,
            AirplaneRig,
            Name::new("Airplane Rig"),
        ))
        .with_children(|rig| {
            rig.spawn((
                WorldAssetRoot(
                    asset_server.load(GltfAssetLabel::Scene(0).from_asset(plane.asset.clone())),
                ),
                plane_model_transform(plane),
                AirplaneModel,
                Name::new("Airplane Model"),
            ));
            rig.spawn((
                overlay_transform,
                overlay_visibility,
                OverlayPropeller,
                Propeller {
                    radians_per_second: overlay_speed,
                },
                Name::new("Overlay Propeller"),
            ))
            .with_children(|propeller| {
                propeller.spawn((
                    Mesh3d(propeller_blade.clone()),
                    MeshMaterial3d(propeller_material.clone()),
                    Transform::default(),
                ));
                propeller.spawn((
                    Mesh3d(propeller_blade.clone()),
                    MeshMaterial3d(propeller_material.clone()),
                    Transform::from_rotation(Quat::from_rotation_z(PI / 2.0)),
                ));
            });
        });
}

fn update_terrain(
    mut commands: Commands,
    state: Res<FlightState>,
    assets: Res<TerrainAssets>,
    mut terrain: ResMut<TerrainWorld>,
) {
    sync_terrain_chunks(&mut commands, &assets, &mut terrain, state.position);
}

fn sync_terrain_chunks(
    commands: &mut Commands,
    assets: &TerrainAssets,
    terrain: &mut TerrainWorld,
    position: Vec3,
) {
    let center = terrain_chunk_coord(position);
    let desired = desired_terrain_chunks(center);

    terrain.chunks.retain(|coord, entity| {
        if desired.contains(coord) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });

    for coord in desired {
        if terrain.chunks.contains_key(&coord) {
            continue;
        }
        let entity = spawn_terrain_chunk(commands, assets, terrain, coord, coord == center);
        terrain.chunks.insert(coord, entity);
    }
}

fn spawn_terrain_chunk(
    commands: &mut Commands,
    assets: &TerrainAssets,
    terrain: &mut TerrainWorld,
    coord: TerrainChunkCoord,
    is_center_chunk: bool,
) -> Entity {
    let seed = terrain.next_seed ^ terrain_chunk_hash(coord);
    terrain.next_seed = terrain.next_seed.wrapping_add(0x9e37_79b9);
    let origin = Vec3::new(
        coord.x as f32 * CHUNK_SIZE,
        0.0,
        coord.z as f32 * CHUNK_SIZE,
    );

    let entity = commands
        .spawn((
            Transform::from_translation(origin),
            Visibility::Inherited,
            coord,
            Name::new(format!("Terrain Chunk {},{}", coord.x, coord.z)),
        ))
        .id();

    commands.entity(entity).with_children(|chunk| {
        spawn_ground_blocks(chunk, assets, seed);
        spawn_chunk_trees(chunk, assets, seed);
        if !is_center_chunk && random_u32(seed, 800) % 100 < 18 {
            spawn_block_mound(chunk, assets, seed);
        }
        if random_u32(seed, 900) % 100 < 32 {
            spawn_block_cloud(chunk, assets, seed);
        }
    });

    entity
}

fn spawn_ground_blocks(chunk: &mut ChildSpawnerCommands<'_>, assets: &TerrainAssets, seed: u32) {
    for ix in 0..TERRAIN_CELLS_PER_CHUNK {
        for iz in 0..TERRAIN_CELLS_PER_CHUNK {
            let cell_id = (ix * TERRAIN_CELLS_PER_CHUNK + iz) as u32;
            let top_y = terrain_cell_top(seed, ix, iz);
            let bottom_y = -0.35;
            let height = top_y - bottom_y;
            let local_x = ix as f32 * TERRAIN_CELL_SIZE + TERRAIN_CELL_SIZE * 0.5;
            let local_z = iz as f32 * TERRAIN_CELL_SIZE + TERRAIN_CELL_SIZE * 0.5;
            let material_index =
                (random_u32(seed, 1100 + cell_id) as usize) % assets.grass_materials.len();

            chunk.spawn((
                Mesh3d(assets.block_mesh.clone()),
                MeshMaterial3d(assets.grass_materials[material_index].clone()),
                Transform::from_xyz(local_x, bottom_y + height * 0.5, local_z).with_scale(
                    Vec3::new(TERRAIN_CELL_SIZE + 0.04, height, TERRAIN_CELL_SIZE + 0.04),
                ),
            ));
        }
    }
}

fn spawn_chunk_trees(chunk: &mut ChildSpawnerCommands<'_>, assets: &TerrainAssets, seed: u32) {
    let tree_count = (random_u32(seed, 2100) % 3) as i32;
    for tree_index in 0..tree_count {
        let salt = 2200 + tree_index as u32 * 17;
        let ix = (random_u32(seed, salt) % TERRAIN_CELLS_PER_CHUNK as u32) as i32;
        let iz = (random_u32(seed, salt + 1) % TERRAIN_CELLS_PER_CHUNK as u32) as i32;
        let local_x = ix as f32 * TERRAIN_CELL_SIZE
            + TERRAIN_CELL_SIZE * 0.5
            + random_range(seed, salt + 2, -2.2, 2.2);
        let local_z = iz as f32 * TERRAIN_CELL_SIZE
            + TERRAIN_CELL_SIZE * 0.5
            + random_range(seed, salt + 3, -2.2, 2.2);
        let ground_y = terrain_cell_top(seed, ix, iz);
        let scale = random_range(seed, salt + 4, 0.85, 1.25);

        chunk.spawn((
            Mesh3d(assets.tree_trunk_mesh.clone()),
            MeshMaterial3d(assets.trunk_material.clone()),
            Transform::from_xyz(local_x, ground_y + 1.2 * scale, local_z)
                .with_scale(Vec3::splat(scale)),
        ));
        chunk.spawn((
            Mesh3d(assets.tree_leaf_mesh.clone()),
            MeshMaterial3d(assets.leaves_material.clone()),
            Transform::from_xyz(local_x, ground_y + 2.8 * scale, local_z)
                .with_scale(Vec3::splat(scale)),
        ));
        chunk.spawn((
            Mesh3d(assets.tree_leaf_mesh.clone()),
            MeshMaterial3d(assets.leaves_material.clone()),
            Transform::from_xyz(local_x, ground_y + 3.75 * scale, local_z)
                .with_scale(Vec3::splat(scale * 0.74)),
        ));
    }
}

fn spawn_block_mound(chunk: &mut ChildSpawnerCommands<'_>, assets: &TerrainAssets, seed: u32) {
    let center_x = random_range(seed, 3100, 9.0, CHUNK_SIZE - 9.0);
    let center_z = random_range(seed, 3101, 9.0, CHUNK_SIZE - 9.0);
    let levels = 2 + (random_u32(seed, 3102) % 3) as i32;
    let block_size = 2.4;

    for level in 0..levels {
        let radius = levels - level;
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                if dx.abs() + dz.abs() > radius + 1 {
                    continue;
                }
                let local_x = center_x + dx as f32 * block_size;
                let local_z = center_z + dz as f32 * block_size;
                if !(1.0..=CHUNK_SIZE - 1.0).contains(&local_x)
                    || !(1.0..=CHUNK_SIZE - 1.0).contains(&local_z)
                {
                    continue;
                }

                let material = if level + 1 == levels && levels >= 4 {
                    assets.snow_material.clone()
                } else {
                    assets.mountain_material.clone()
                };
                chunk.spawn((
                    Mesh3d(assets.block_mesh.clone()),
                    MeshMaterial3d(material),
                    Transform::from_xyz(
                        local_x,
                        0.8 + level as f32 * block_size + block_size * 0.5,
                        local_z,
                    )
                    .with_scale(Vec3::splat(block_size)),
                ));
            }
        }
    }
}

fn spawn_block_cloud(chunk: &mut ChildSpawnerCommands<'_>, assets: &TerrainAssets, seed: u32) {
    let local_x = random_range(seed, 4100, 6.0, CHUNK_SIZE - 6.0);
    let local_z = random_range(seed, 4101, 6.0, CHUNK_SIZE - 6.0);
    let local_y = random_range(seed, 4102, 20.0, 30.0);
    for (offset_x, offset_z, scale_x, scale_z) in [
        (0.0, 0.0, 6.2, 2.3),
        (-2.7, 1.2, 3.8, 2.0),
        (2.9, -0.8, 4.5, 2.1),
    ] {
        chunk.spawn((
            Mesh3d(assets.cloud_mesh.clone()),
            MeshMaterial3d(assets.cloud_material.clone()),
            Transform::from_xyz(local_x + offset_x, local_y, local_z + offset_z)
                .with_scale(Vec3::new(scale_x, 0.85, scale_z)),
        ));
    }
}

fn terrain_chunk_coord(position: Vec3) -> TerrainChunkCoord {
    TerrainChunkCoord {
        x: (position.x / CHUNK_SIZE).floor() as i32,
        z: (position.z / CHUNK_SIZE).floor() as i32,
    }
}

fn desired_terrain_chunks(center: TerrainChunkCoord) -> HashSet<TerrainChunkCoord> {
    let mut desired = HashSet::with_capacity(((CHUNK_RADIUS * 2 + 1).pow(2)) as usize);
    for x in center.x - CHUNK_RADIUS..=center.x + CHUNK_RADIUS {
        for z in center.z - CHUNK_RADIUS..=center.z + CHUNK_RADIUS {
            desired.insert(TerrainChunkCoord { x, z });
        }
    }
    desired
}

fn terrain_cell_top(seed: u32, ix: i32, iz: i32) -> f32 {
    let salt = 5100 + (ix as u32).wrapping_mul(37) + (iz as u32).wrapping_mul(113);
    let base = 0.18 + random_unit(seed, salt) * 0.55;
    let ridge = if random_u32(seed, salt + 1) % 11 == 0 {
        0.55
    } else {
        0.0
    };
    base + ridge
}

fn terrain_chunk_hash(coord: TerrainChunkCoord) -> u32 {
    random_u32(
        coord.x as u32 ^ 0x6ac6_90c5,
        (coord.z as u32).wrapping_add(0xb529_7a4d),
    )
}

fn random_range(seed: u32, salt: u32, min: f32, max: f32) -> f32 {
    min + (max - min) * random_unit(seed, salt)
}

fn random_unit(seed: u32, salt: u32) -> f32 {
    random_u32(seed, salt) as f32 / u32::MAX as f32
}

fn random_u32(seed: u32, salt: u32) -> u32 {
    let mut value = seed ^ salt.wrapping_mul(0x9e37_79b9);
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^ (value >> 16)
}

fn drive_flight(
    time: Res<Time>,
    mut state: ResMut<FlightState>,
    mut cameras: Query<&mut Transform, (With<KitflyCamera>, Without<AirplaneRig>)>,
    mut airplanes: Query<&mut Transform, (With<AirplaneRig>, Without<KitflyCamera>)>,
) {
    let pose = state.advance(time.delta_secs());
    for mut camera in &mut cameras {
        *camera = pose.camera;
    }
    for mut airplane in &mut airplanes {
        *airplane = pose.airplane;
    }
}

fn apply_plane_selection(
    selection: Res<PlaneSelection>,
    mut applied_selection: ResMut<AppliedPlaneSelection>,
    asset_server: Res<AssetServer>,
    mut models: Query<
        (&mut WorldAssetRoot, &mut Transform),
        (With<AirplaneModel>, Without<OverlayPropeller>),
    >,
    mut overlay_propellers: Query<
        (&mut Visibility, &mut Transform, &mut Propeller),
        (With<OverlayPropeller>, Without<AirplaneModel>),
    >,
) {
    if applied_selection.is_current(&selection) {
        return;
    }

    let plane = selection.definition();
    for (mut root, mut transform) in &mut models {
        *root = WorldAssetRoot(
            asset_server.load(GltfAssetLabel::Scene(0).from_asset(plane.asset.clone())),
        );
        *transform = plane_model_transform(plane);
    }

    let (overlay_transform, overlay_visibility, overlay_speed) = overlay_propeller_state(plane);
    for (mut visibility, mut transform, mut propeller) in &mut overlay_propellers {
        *visibility = overlay_visibility;
        *transform = overlay_transform;
        propeller.radians_per_second = overlay_speed;
    }
    applied_selection.apply(&selection);
}

fn tag_configured_propeller_node(
    scene_ready: On<WorldInstanceReady>,
    mut commands: Commands,
    selection: Res<PlaneSelection>,
    children: Query<&Children>,
    names: Query<&Name>,
    models: Query<(), With<AirplaneModel>>,
    propellers: Query<(), With<Propeller>>,
) {
    let PlanePropeller::Node {
        name: propeller_name,
        radians_per_second,
    } = &selection.definition().propeller
    else {
        return;
    };
    if models.get(scene_ready.entity).is_err() {
        return;
    }

    for descendant in children.iter_descendants(scene_ready.entity) {
        if propellers.get(descendant).is_ok() {
            continue;
        }
        let Ok(name) = names.get(descendant) else {
            continue;
        };
        if name.as_str() == propeller_name {
            commands.entity(descendant).insert(Propeller {
                radians_per_second: *radians_per_second,
            });
        }
    }
}

fn plane_model_transform(plane: &PlaneDefinition) -> Transform {
    Transform::from_xyz(
        plane.translation[0],
        plane.translation[1],
        plane.translation[2],
    )
    .with_rotation(Quat::from_euler(
        EulerRot::XYZ,
        plane.rotation_xyz[0],
        plane.rotation_xyz[1],
        plane.rotation_xyz[2],
    ))
    .with_scale(Vec3::splat(plane.scale))
}

fn overlay_propeller_state(plane: &PlaneDefinition) -> (Transform, Visibility, f32) {
    match &plane.propeller {
        PlanePropeller::Overlay {
            translation,
            radians_per_second,
        } => (
            Transform::from_xyz(translation[0], translation[1], translation[2]),
            Visibility::Visible,
            *radians_per_second,
        ),
        PlanePropeller::None | PlanePropeller::Node { .. } => {
            (Transform::default(), Visibility::Hidden, 0.0)
        }
    }
}

fn spin_propellers(time: Res<Time>, mut propellers: Query<(&Propeller, &mut Transform)>) {
    let delta = time.delta_secs();
    for (propeller, mut transform) in &mut propellers {
        transform.rotate_local_z(propeller.radians_per_second * delta);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plane_selection_wraps_through_all_planes() {
        let catalog = PlaneCatalog::parse(include_str!("../assets/planes.cfg")).unwrap();
        let names: Vec<_> = catalog
            .planes
            .iter()
            .map(|plane| plane.name.clone())
            .collect();
        let mut selection = PlaneSelection::from_catalog(catalog);

        for name in &names {
            assert_eq!(selection.current_name(), name);
            selection.next();
        }
        assert_eq!(selection.current_name(), names[0]);
    }

    #[test]
    fn applied_plane_selection_starts_current() {
        let catalog = PlaneCatalog::parse(include_str!("../assets/planes.cfg")).unwrap();
        let mut selection = PlaneSelection::from_catalog(catalog);
        let mut applied = AppliedPlaneSelection::from_selection(&selection);

        assert!(applied.is_current(&selection));

        selection.next();
        assert!(!applied.is_current(&selection));

        applied.apply(&selection);
        assert!(applied.is_current(&selection));
    }

    #[test]
    fn terrain_chunk_coord_floors_negative_positions() {
        assert_eq!(
            terrain_chunk_coord(Vec3::new(-0.1, 0.0, -0.1)),
            TerrainChunkCoord { x: -1, z: -1 }
        );
        assert_eq!(
            terrain_chunk_coord(Vec3::new(0.0, 0.0, 31.9)),
            TerrainChunkCoord { x: 0, z: 0 }
        );
    }

    #[test]
    fn terrain_desired_set_stays_bounded_and_moves() {
        let start = desired_terrain_chunks(TerrainChunkCoord { x: 0, z: 0 });
        let moved = desired_terrain_chunks(TerrainChunkCoord { x: 8, z: 0 });
        let expected = ((CHUNK_RADIUS * 2 + 1).pow(2)) as usize;

        assert_eq!(start.len(), expected);
        assert_eq!(moved.len(), expected);
        assert!(start.contains(&TerrainChunkCoord { x: 0, z: 0 }));
        assert!(!moved.contains(&TerrainChunkCoord { x: 0, z: 0 }));
    }
}
