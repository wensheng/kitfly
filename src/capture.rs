use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use bevy::{
    app::SubApps,
    asset::RenderAssetUsages,
    camera::RenderTarget,
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
        render_asset::RenderAssets,
        render_resource::{
            Buffer, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, MapMode,
            PollType, TexelCopyBufferInfo, TexelCopyBufferLayout, TextureFormat, TextureUsages,
        },
        renderer::{RenderContext, RenderDevice, RenderGraph, RenderQueue},
    },
};
use crossbeam_channel::{Receiver, Sender};

#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Resource)]
pub struct MainWorldReceiver(pub Receiver<CapturedFrame>);

#[derive(Resource)]
struct RenderWorldSender(Sender<CapturedFrame>);

pub struct FrameCapturePlugin;

impl Plugin for FrameCapturePlugin {
    fn build(&self, app: &mut App) {
        let (sender, receiver) = crossbeam_channel::unbounded();
        let render_app = app
            .insert_resource(MainWorldReceiver(receiver))
            .sub_app_mut(RenderApp);

        render_app
            .insert_resource(RenderWorldSender(sender))
            .add_systems(ExtractSchedule, image_copy_extract)
            .add_systems(
                Render,
                receive_image_from_buffer.after(RenderSystems::Render),
            )
            .add_systems(RenderGraph, image_copy_driver);
    }
}

#[derive(Clone, Default, Resource, Deref, DerefMut)]
struct ImageCopiers(Vec<ImageCopier>);

#[derive(Clone, Component)]
struct ImageCopier {
    buffer: Buffer,
    enabled: Arc<AtomicBool>,
    src_image: Handle<Image>,
    width: u32,
    height: u32,
    row_bytes: usize,
    padded_row_bytes: usize,
}

impl ImageCopier {
    fn new(
        src_image: Handle<Image>,
        width: u32,
        height: u32,
        render_device: &RenderDevice,
    ) -> Self {
        let row_bytes = width.max(1) as usize * 4;
        let padded_row_bytes = RenderDevice::align_copy_bytes_per_row(row_bytes);
        let cpu_buffer = render_device.create_buffer(&BufferDescriptor {
            label: Some("kitfly_frame_readback"),
            size: padded_row_bytes as u64 * height.max(1) as u64,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            buffer: cpu_buffer,
            enabled: Arc::new(AtomicBool::new(true)),
            src_image,
            width,
            height,
            row_bytes,
            padded_row_bytes,
        }
    }

    fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
}

pub fn create_render_target(
    world: &mut World,
    width: u32,
    height: u32,
) -> (RenderTarget, Handle<Image>) {
    let width = width.max(1);
    let height = height.max(1);
    let mut target = Image::new_target_texture(width, height, TextureFormat::Rgba8UnormSrgb, None);
    target.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    target.asset_usage = RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD;
    let handle = world.resource_mut::<Assets<Image>>().add(target);

    let render_device = world.resource::<RenderDevice>();
    let copier = ImageCopier::new(handle.clone(), width, height, render_device);
    world.spawn(copier);

    (RenderTarget::Image(handle.clone().into()), handle)
}

pub fn latest_frame(world: &World) -> Option<CapturedFrame> {
    let receiver = world.resource::<MainWorldReceiver>();
    let mut latest = None;
    while let Ok(frame) = receiver.0.try_recv() {
        latest = Some(frame);
    }
    latest
}

pub fn wait_for_render_device(world: &World) {
    let _ = world
        .resource::<RenderDevice>()
        .wgpu_device()
        .poll(PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_millis(100)),
        });
}

pub fn finish_for_external_loop(app: &mut App) -> SubApps {
    app.finish();
    app.cleanup();
    std::mem::take(app.sub_apps_mut())
}

fn image_copy_extract(mut commands: Commands, image_copiers: Extract<Query<&ImageCopier>>) {
    commands.insert_resource(ImageCopiers(image_copiers.iter().cloned().collect()));
}

fn image_copy_driver(
    render_context: RenderContext,
    image_copiers: Res<ImageCopiers>,
    render_queue: Res<RenderQueue>,
    gpu_images: Res<RenderAssets<bevy::render::texture::GpuImage>>,
) {
    for image_copier in image_copiers.iter() {
        if !image_copier.enabled() {
            continue;
        }

        let Some(src_image) = gpu_images.get(&image_copier.src_image) else {
            continue;
        };

        let mut encoder = render_context
            .render_device()
            .create_command_encoder(&CommandEncoderDescriptor::default());

        encoder.copy_texture_to_buffer(
            src_image.texture.as_image_copy(),
            TexelCopyBufferInfo {
                buffer: &image_copier.buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(
                        std::num::NonZero::<u32>::new(image_copier.padded_row_bytes as u32)
                            .expect("padded row bytes are non-zero")
                            .into(),
                    ),
                    rows_per_image: None,
                },
            },
            Extent3d {
                width: image_copier.width,
                height: image_copier.height,
                depth_or_array_layers: 1,
            },
        );

        render_queue.submit(std::iter::once(encoder.finish()));
    }
}

fn receive_image_from_buffer(
    image_copiers: Res<ImageCopiers>,
    render_device: Res<RenderDevice>,
    sender: Res<RenderWorldSender>,
) {
    for image_copier in image_copiers.iter() {
        if !image_copier.enabled() {
            continue;
        }

        let buffer_slice = image_copier.buffer.slice(..);
        let (map_sender, map_receiver) = crossbeam_channel::bounded(1);
        buffer_slice.map_async(MapMode::Read, move |result| {
            let _ = map_sender.send(result);
        });
        if render_device.poll(PollType::wait_indefinitely()).is_err() {
            continue;
        }
        if !matches!(map_receiver.recv(), Ok(Ok(()))) {
            image_copier.buffer.unmap();
            continue;
        }

        let mapped = buffer_slice.get_mapped_range();
        let pixels = if image_copier.row_bytes == image_copier.padded_row_bytes {
            mapped.to_vec()
        } else {
            mapped
                .chunks(image_copier.padded_row_bytes)
                .take(image_copier.height as usize)
                .flat_map(|row| row[..image_copier.row_bytes.min(row.len())].iter().copied())
                .collect()
        };
        drop(mapped);
        image_copier.buffer.unmap();

        let _ = sender.0.send(CapturedFrame {
            width: image_copier.width,
            height: image_copier.height,
            pixels,
        });
    }
}
