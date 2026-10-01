//! Vulkan context and the splat rasterization pipeline.
//!
//! Milestone-1 design: splats are sorted back-to-front on the CPU each time the
//! camera moves (16-bit counting sort over view depth), the sorted index list is
//! uploaded, and one instanced draw expands every Gaussian into an
//! eigen-aligned quad in the vertex shader. Blending is premultiplied alpha.

use crate::camera::Camera;
use crate::ply::{SH_COEFFS, Scene, Splat};
use anyhow::{Context, Result, anyhow};
use glam::{Mat4, Vec3};
use std::sync::Arc;
use vulkano::{
    VulkanLibrary,
    buffer::{
        Buffer, BufferContents, BufferCreateInfo, BufferUsage, Subbuffer,
        allocator::{SubbufferAllocator, SubbufferAllocatorCreateInfo},
    },
    command_buffer::{
        AutoCommandBufferBuilder, CommandBufferUsage, CopyBufferInfo, CopyImageToBufferInfo,
        PrimaryAutoCommandBuffer, PrimaryCommandBufferAbstract, RenderingAttachmentInfo, RenderingInfo,
        allocator::StandardCommandBufferAllocator,
    },
    descriptor_set::{DescriptorSet, WriteDescriptorSet, allocator::StandardDescriptorSetAllocator},
    device::{
        Device, DeviceCreateInfo, DeviceExtensions, DeviceFeatures, Queue, QueueCreateInfo, QueueFlags,
        physical::{PhysicalDevice, PhysicalDeviceType},
    },
    format::Format,
    image::{Image, ImageCreateInfo, ImageType, ImageUsage, view::ImageView},
    instance::{Instance, InstanceCreateFlags, InstanceCreateInfo, InstanceExtensions},
    memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator},
    pipeline::{
        GraphicsPipeline, Pipeline, PipelineBindPoint, PipelineLayout, PipelineShaderStageCreateInfo,
        graphics::{
            GraphicsPipelineCreateInfo,
            color_blend::{AttachmentBlend, BlendFactor, BlendOp, ColorBlendAttachmentState, ColorBlendState},
            input_assembly::InputAssemblyState,
            multisample::MultisampleState,
            rasterization::RasterizationState,
            subpass::PipelineRenderingCreateInfo,
            vertex_input::VertexInputState,
            viewport::{Viewport, ViewportState},
        },
        layout::PipelineDescriptorSetLayoutCreateInfo,
    },
    render_pass::{AttachmentLoadOp, AttachmentStoreOp},
    sync::GpuFuture,
};

mod vs {
    vulkano_shaders::shader! { ty: "vertex", path: "src/shaders/splat.vert" }
}
mod fs {
    vulkano_shaders::shader! { ty: "fragment", path: "src/shaders/splat.frag" }
}

pub struct Gpu {
    pub instance: Arc<Instance>,
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    pub mem: Arc<StandardMemoryAllocator>,
    pub cmd: Arc<StandardCommandBufferAllocator>,
    pub desc: Arc<StandardDescriptorSetAllocator>,
}

impl Gpu {
    /// `present` filters physical devices/queues by presentation support when windowed.
    pub fn new(
        instance_exts: InstanceExtensions,
        present: Option<&dyn Fn(&Arc<PhysicalDevice>, u32) -> bool>,
    ) -> Result<Self> {
        let library = VulkanLibrary::new().context("loading Vulkan library")?;
        let instance = Instance::new(
            library,
            InstanceCreateInfo {
                flags: InstanceCreateFlags::ENUMERATE_PORTABILITY,
                enabled_extensions: instance_exts,
                ..Default::default()
            },
        )?;

        let mut device_exts = DeviceExtensions::default();
        if present.is_some() {
            device_exts.khr_swapchain = true;
        }
        let (physical, queue_family) = instance
            .enumerate_physical_devices()?
            .filter(|p| p.api_version() >= vulkano::Version::V1_3 || p.supported_extensions().khr_dynamic_rendering)
            .filter(|p| p.supported_extensions().contains(&device_exts))
            .filter_map(|p| {
                p.queue_family_properties()
                    .iter()
                    .enumerate()
                    .position(|(i, q)| {
                        q.queue_flags.intersects(QueueFlags::GRAPHICS)
                            && present.is_none_or(|f| f(&p, i as u32))
                    })
                    .map(|i| (p, i as u32))
            })
            .min_by_key(|(p, _)| match p.properties().device_type {
                PhysicalDeviceType::DiscreteGpu => 0,
                PhysicalDeviceType::IntegratedGpu => 1,
                PhysicalDeviceType::VirtualGpu => 2,
                PhysicalDeviceType::Cpu => 3,
                _ => 4,
            })
            .ok_or_else(|| anyhow!("no suitable Vulkan device"))?;
        eprintln!("GPU: {} ({:?})", physical.properties().device_name, physical.properties().device_type);

        if physical.api_version() < vulkano::Version::V1_3 {
            device_exts.khr_dynamic_rendering = true;
        }
        let (device, mut queues) = Device::new(
            physical,
            DeviceCreateInfo {
                queue_create_infos: vec![QueueCreateInfo { queue_family_index: queue_family, ..Default::default() }],
                enabled_extensions: device_exts,
                enabled_features: DeviceFeatures { dynamic_rendering: true, ..Default::default() },
                ..Default::default()
            },
        )?;
        let queue = queues.next().unwrap();
        let mem = Arc::new(StandardMemoryAllocator::new_default(device.clone()));
        let cmd = Arc::new(StandardCommandBufferAllocator::new(device.clone(), Default::default()));
        let desc = Arc::new(StandardDescriptorSetAllocator::new(device.clone(), Default::default()));
        Ok(Self { instance, device, queue, mem, cmd, desc })
    }

    pub fn builder(&self) -> Result<AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>> {
        Ok(AutoCommandBufferBuilder::primary(
            self.cmd.clone(),
            self.queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )?)
    }

    /// Upload `data` into a device-local buffer through a staging copy.
    fn upload<T: BufferContents + Copy>(&self, data: &[T], usage: BufferUsage) -> Result<Subbuffer<[T]>> {
        let staging = Buffer::from_iter(
            self.mem.clone(),
            BufferCreateInfo { usage: BufferUsage::TRANSFER_SRC, ..Default::default() },
            AllocationCreateInfo {
                memory_type_filter: MemoryTypeFilter::PREFER_HOST | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                ..Default::default()
            },
            data.iter().copied(),
        )?;
        let dst = Buffer::new_slice::<T>(
            self.mem.clone(),
            BufferCreateInfo { usage: usage | BufferUsage::TRANSFER_DST, ..Default::default() },
            AllocationCreateInfo { memory_type_filter: MemoryTypeFilter::PREFER_DEVICE, ..Default::default() },
            data.len() as u64,
        )?;
        let mut b = self.builder()?;
        b.copy_buffer(CopyBufferInfo::buffers(staging, dst.clone()))?;
        b.build()?.execute(self.queue.clone())?.then_signal_fence_and_flush()?.wait(None)?;
        Ok(dst)
    }
}

pub struct SplatRenderer {
    pipeline: Arc<GraphicsPipeline>,
    splats: Subbuffer<[Splat]>,
    sh: Subbuffer<[f32]>,
    index_alloc: SubbufferAllocator,
    positions: Vec<Vec3>,
    count: u32,
    sh_degree: u32,
    // Sort state, reused across frames.
    keys: Vec<u16>,
    sorted: Vec<u32>,
    last_view: Option<Mat4>,
    last_indices: Option<Subbuffer<[u32]>>,
    pub background: [f32; 4],
}

impl SplatRenderer {
    pub fn new(gpu: &Gpu, scene: &Scene, format: Format) -> Result<Self> {
        let device = &gpu.device;
        let vs = vs::load(device.clone())?.entry_point("main").unwrap();
        let fs = fs::load(device.clone())?.entry_point("main").unwrap();
        let stages = [PipelineShaderStageCreateInfo::new(vs), PipelineShaderStageCreateInfo::new(fs)];
        let layout = PipelineLayout::new(
            device.clone(),
            PipelineDescriptorSetLayoutCreateInfo::from_stages(&stages)
                .into_pipeline_layout_create_info(device.clone())?,
        )?;
        let blend = AttachmentBlend {
            src_color_blend_factor: BlendFactor::One,
            dst_color_blend_factor: BlendFactor::OneMinusSrcAlpha,
            color_blend_op: BlendOp::Add,
            src_alpha_blend_factor: BlendFactor::One,
            dst_alpha_blend_factor: BlendFactor::OneMinusSrcAlpha,
            alpha_blend_op: BlendOp::Add,
        };
        let pipeline = GraphicsPipeline::new(
            device.clone(),
            None,
            GraphicsPipelineCreateInfo {
                stages: stages.into_iter().collect(),
                vertex_input_state: Some(VertexInputState::default()),
                input_assembly_state: Some(InputAssemblyState::default()),
                viewport_state: Some(ViewportState::default()),
                rasterization_state: Some(RasterizationState::default()),
                multisample_state: Some(MultisampleState::default()),
                color_blend_state: Some(ColorBlendState::with_attachment_states(
                    1,
                    ColorBlendAttachmentState { blend: Some(blend), ..Default::default() },
                )),
                dynamic_state: [vulkano::pipeline::DynamicState::Viewport].into_iter().collect(),
                subpass: Some(
                    PipelineRenderingCreateInfo { color_attachment_formats: vec![Some(format)], ..Default::default() }
                        .into(),
                ),
                ..GraphicsPipelineCreateInfo::layout(layout)
            },
        )?;

        let count = scene.splats.len() as u32;
        let splats = gpu.upload(&scene.splats, BufferUsage::STORAGE_BUFFER)?;
        debug_assert_eq!(scene.sh.len(), scene.splats.len() * SH_COEFFS * 3);
        let sh = gpu.upload(&scene.sh, BufferUsage::STORAGE_BUFFER)?;
        let index_alloc = SubbufferAllocator::new(
            gpu.mem.clone(),
            SubbufferAllocatorCreateInfo {
                buffer_usage: BufferUsage::STORAGE_BUFFER,
                memory_type_filter: MemoryTypeFilter::PREFER_DEVICE | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                ..Default::default()
            },
        );
        let positions = scene.splats.iter().map(|s| Vec3::from_slice(&s.pos_opacity[..3])).collect();

        Ok(Self {
            pipeline,
            splats,
            sh,
            index_alloc,
            positions,
            count,
            sh_degree: scene.sh_degree,
            keys: Vec::new(),
            sorted: Vec::new(),
            last_view: None,
            last_indices: None,
            background: [0.0, 0.0, 0.0, 1.0],
        })
    }

    /// Back-to-front order via a 16-bit counting sort on view-space depth.
    fn sort(&mut self, cam: &Camera) {
        let row = cam.view.row(2);
        let (rz, tz) = (row.truncate(), row.w);
        let n = self.positions.len();
        let (mut zmin, mut zmax) = (f32::MAX, f32::MIN);
        let depths: Vec<f32> = self
            .positions
            .iter()
            .map(|p| {
                let z = rz.dot(*p) + tz;
                zmin = zmin.min(z);
                zmax = zmax.max(z);
                z
            })
            .collect();
        let scale = 65535.0 / (zmax - zmin).max(1e-6);
        self.keys.clear();
        // Far splats get small keys so ascending order is back-to-front.
        self.keys.extend(depths.iter().map(|z| ((zmax - z) * scale) as u16));

        let mut counts = vec![0u32; 65537];
        for &k in &self.keys {
            counts[k as usize + 1] += 1;
        }
        for i in 1..counts.len() {
            counts[i] += counts[i - 1];
        }
        self.sorted.resize(n, 0);
        for (i, &k) in self.keys.iter().enumerate() {
            let slot = &mut counts[k as usize];
            self.sorted[*slot as usize] = i as u32;
            *slot += 1;
        }
    }

    fn indices(&mut self, cam: &Camera) -> Result<Subbuffer<[u32]>> {
        if self.last_view == Some(cam.view) {
            if let Some(buf) = &self.last_indices {
                return Ok(buf.clone());
            }
        }
        self.sort(cam);
        let buf = self.index_alloc.allocate_slice::<u32>(self.count.max(1) as u64)?;
        buf.write()?[..self.sorted.len()].copy_from_slice(&self.sorted);
        self.last_view = Some(cam.view);
        self.last_indices = Some(buf.clone());
        Ok(buf)
    }

    /// Record the splat pass into `target`, clearing it first.
    pub fn record(
        &mut self,
        gpu: &Gpu,
        b: &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
        target: Arc<ImageView>,
        cam: &Camera,
    ) -> Result<()> {
        let [w, h, _] = target.image().extent();
        let indices = self.indices(cam)?;
        let layout = self.pipeline.layout().clone();
        let set = DescriptorSet::new(
            gpu.desc.clone(),
            layout.set_layouts()[0].clone(),
            [
                WriteDescriptorSet::buffer(0, self.splats.clone()),
                WriteDescriptorSet::buffer(1, self.sh.clone()),
                WriteDescriptorSet::buffer(2, indices),
            ],
            [],
        )?;
        let push = vs::Push {
            view: cam.view.to_cols_array_2d(),
            cam_pos: cam.position.extend(self.sh_degree as f32).to_array(),
            intrinsics: [cam.fx, cam.fy, w as f32, h as f32],
        };
        b.begin_rendering(RenderingInfo {
            color_attachments: vec![Some(RenderingAttachmentInfo {
                load_op: AttachmentLoadOp::Clear,
                store_op: AttachmentStoreOp::Store,
                clear_value: Some(self.background.into()),
                ..RenderingAttachmentInfo::image_view(target)
            })],
            ..Default::default()
        })?
        .set_viewport(
            0,
            [Viewport { offset: [0.0, 0.0], extent: [w as f32, h as f32], depth_range: 0.0..=1.0 }]
                .into_iter()
                .collect(),
        )?
        .bind_pipeline_graphics(self.pipeline.clone())?
        .bind_descriptor_sets(PipelineBindPoint::Graphics, layout.clone(), 0, set)?
        .push_constants(layout, 0, push)?;
        unsafe { b.draw(6, self.count, 0, 0)? };
        b.end_rendering()?;
        Ok(())
    }
}

/// Render one frame offscreen and return tightly packed RGBA8 pixels.
pub fn render_offscreen(
    gpu: &Gpu,
    renderer: &mut SplatRenderer,
    cam: &Camera,
    width: u32,
    height: u32,
) -> Result<Vec<u8>> {
    let image = Image::new(
        gpu.mem.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: OFFSCREEN_FORMAT,
            extent: [width, height, 1],
            usage: ImageUsage::COLOR_ATTACHMENT | ImageUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo::default(),
    )?;
    let readback = Buffer::new_slice::<u8>(
        gpu.mem.clone(),
        BufferCreateInfo { usage: BufferUsage::TRANSFER_DST, ..Default::default() },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_HOST | MemoryTypeFilter::HOST_RANDOM_ACCESS,
            ..Default::default()
        },
        (width * height * 4) as u64,
    )?;
    let mut b = gpu.builder()?;
    renderer.record(gpu, &mut b, ImageView::new_default(image.clone())?, cam)?;
    b.copy_image_to_buffer(CopyImageToBufferInfo::image_buffer(image, readback.clone()))?;
    b.build()?.execute(gpu.queue.clone())?.then_signal_fence_and_flush()?.wait(None)?;
    Ok(readback.read()?.to_vec())
}

/// UNORM (not sRGB): 3DGS colors are blended in display space, as in the reference rasterizer.
pub const OFFSCREEN_FORMAT: Format = Format::R8G8B8A8_UNORM;
