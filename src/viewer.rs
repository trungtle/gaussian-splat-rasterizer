//! Interactive windowed viewer (winit + swapchain).
//!
//! Controls: left-drag orbit, right-drag pan, wheel zoom,
//! N / P cycle dataset cameras, R reset view, Esc quit.

use crate::camera::Orbit;
use crate::renderer::{Gpu, SplatRenderer};
use crate::{Args, Framing, ply::Scene};
use anyhow::Result;
use glam::Vec3;
use std::sync::Arc;
use std::time::Instant;
use vulkano::{
    Validated, VulkanError,
    format::Format,
    image::{ImageUsage, view::ImageView},
    swapchain::{self, Surface, Swapchain, SwapchainCreateInfo, SwapchainPresentInfo},
    sync::{self, GpuFuture},
};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

pub fn run(args: Args, scene: Scene, framing: Framing) -> Result<()> {
    let event_loop = EventLoop::new()?;
    let exts = Surface::required_extensions(&event_loop)?;
    let el = &event_loop;
    let present = move |p: &Arc<vulkano::device::physical::PhysicalDevice>, i: u32| {
        p.presentation_support(i, el).unwrap_or(false)
    };
    let gpu = Gpu::new(exts, Some(&present))?;
    let orbit = framing.orbit(args.cam);
    let mut app = App { gpu, scene, framing, args, orbit, rcx: None, error: None };
    event_loop.run_app(&mut app)?;
    match app.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

struct App {
    gpu: Gpu,
    scene: Scene,
    framing: Framing,
    args: Args,
    orbit: Orbit,
    rcx: Option<RenderContext>,
    error: Option<anyhow::Error>,
}

struct RenderContext {
    window: Arc<Window>,
    swapchain: Arc<Swapchain>,
    views: Vec<Arc<ImageView>>,
    renderer: SplatRenderer,
    recreate: bool,
    prev: Option<Box<dyn GpuFuture>>,
    cam_index: usize,
    left: bool,
    right: bool,
    cursor: Option<(f64, f64)>,
    fps_timer: Instant,
    frames: u32,
}

impl App {
    fn init(&mut self, el: &ActiveEventLoop) -> Result<RenderContext> {
        let (w, h) = self.args.size;
        let window = Arc::new(el.create_window(
            Window::default_attributes()
                .with_title("gaussian-splat-rasterizer")
                .with_inner_size(winit::dpi::PhysicalSize::new(w, h)),
        )?);
        let surface = Surface::from_window(self.gpu.instance.clone(), window.clone())?;
        let pd = self.gpu.device.physical_device();
        let caps = pd.surface_capabilities(&surface, Default::default())?;
        let formats = pd.surface_formats(&surface, Default::default())?;
        // Prefer UNORM: 3DGS colors are already in display space.
        let format = [Format::B8G8R8A8_UNORM, Format::R8G8B8A8_UNORM]
            .into_iter()
            .find(|f| formats.iter().any(|(sf, _)| sf == f))
            .unwrap_or(formats[0].0);
        let (swapchain, images) = Swapchain::new(
            self.gpu.device.clone(),
            surface,
            SwapchainCreateInfo {
                min_image_count: caps.min_image_count.max(2),
                image_format: format,
                image_extent: window.inner_size().into(),
                image_usage: ImageUsage::COLOR_ATTACHMENT,
                composite_alpha: caps.supported_composite_alpha.into_iter().next().unwrap(),
                ..Default::default()
            },
        )?;
        let views = images.into_iter().map(ImageView::new_default).collect::<Result<_, _>>()?;
        let mut renderer = SplatRenderer::new(&self.gpu, &self.scene, format)?;
        let bg = self.args.bg;
        renderer.background = [bg[0], bg[1], bg[2], 1.0];
        Ok(RenderContext {
            window,
            swapchain,
            views,
            renderer,
            recreate: false,
            prev: Some(sync::now(self.gpu.device.clone()).boxed()),
            cam_index: self.args.cam,
            left: false,
            right: false,
            cursor: None,
            fps_timer: Instant::now(),
            frames: 0,
        })
    }

    fn redraw(&mut self) -> Result<()> {
        let gpu = &self.gpu;
        let rcx = self.rcx.as_mut().unwrap();
        let size = rcx.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        rcx.prev.as_mut().unwrap().cleanup_finished();
        if rcx.recreate {
            let (sc, images) = rcx.swapchain.recreate(SwapchainCreateInfo {
                image_extent: size.into(),
                ..rcx.swapchain.create_info()
            })?;
            rcx.swapchain = sc;
            rcx.views = images.into_iter().map(ImageView::new_default).collect::<Result<_, _>>()?;
            rcx.recreate = false;
        }
        let (index, suboptimal, acquire) =
            match swapchain::acquire_next_image(rcx.swapchain.clone(), None).map_err(Validated::unwrap) {
                Ok(r) => r,
                Err(VulkanError::OutOfDate) => {
                    rcx.recreate = true;
                    return Ok(());
                }
                Err(e) => return Err(e.into()),
            };
        rcx.recreate |= suboptimal;

        let extent = rcx.swapchain.image_extent();
        let cam = self.orbit.camera(extent[1]);
        let mut b = gpu.builder()?;
        rcx.renderer.record(gpu, &mut b, rcx.views[index as usize].clone(), &cam)?;
        let cb = b.build()?;
        let future = rcx
            .prev
            .take()
            .unwrap()
            .join(acquire)
            .then_execute(gpu.queue.clone(), cb)?
            .then_swapchain_present(
                gpu.queue.clone(),
                SwapchainPresentInfo::swapchain_image_index(rcx.swapchain.clone(), index),
            )
            .then_signal_fence_and_flush();
        rcx.prev = Some(match future.map_err(Validated::unwrap) {
            Ok(f) => f.boxed(),
            Err(VulkanError::OutOfDate) => {
                rcx.recreate = true;
                sync::now(gpu.device.clone()).boxed()
            }
            Err(e) => return Err(e.into()),
        });

        rcx.frames += 1;
        let dt = rcx.fps_timer.elapsed().as_secs_f32();
        if dt > 0.5 {
            rcx.window.set_title(&format!(
                "gaussian-splat-rasterizer — {:.1} fps — cam {}",
                rcx.frames as f32 / dt,
                rcx.cam_index
            ));
            rcx.frames = 0;
            rcx.fps_timer = Instant::now();
        }
        Ok(())
    }

    fn handle(&mut self, el: &ActiveEventLoop, event: WindowEvent) -> Result<()> {
        let Some(rcx) = self.rcx.as_mut() else { return Ok(()) };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(_) => rcx.recreate = true,
            WindowEvent::RedrawRequested => self.redraw()?,
            WindowEvent::MouseInput { state, button, .. } => {
                let down = state == ElementState::Pressed;
                match button {
                    MouseButton::Left => rcx.left = down,
                    MouseButton::Right | MouseButton::Middle => rcx.right = down,
                    _ => {}
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some((x0, y0)) = rcx.cursor {
                    let (dx, dy) = ((position.x - x0) as f32, (position.y - y0) as f32);
                    if rcx.left {
                        self.orbit.yaw -= dx * 0.005;
                        self.orbit.pitch = (self.orbit.pitch + dy * 0.005).clamp(-1.55, 1.55);
                    } else if rcx.right {
                        let cam = self.orbit.camera(rcx.window.inner_size().height);
                        let right = Vec3::new(cam.view.x_axis.x, cam.view.y_axis.x, cam.view.z_axis.x);
                        let down = Vec3::new(cam.view.x_axis.y, cam.view.y_axis.y, cam.view.z_axis.y);
                        let k = self.orbit.distance / cam.fy;
                        self.orbit.target -= (right * dx + down * dy) * k;
                    }
                }
                rcx.cursor = Some((position.x, position.y));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let s = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 50.0,
                };
                self.orbit.distance *= 0.9f32.powf(s);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let n = self.framing.dataset.len();
                match event.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) => el.exit(),
                    Key::Character("n") | Key::Character("N") if n > 0 => {
                        rcx.cam_index = (rcx.cam_index + 1) % n;
                        self.orbit = self.framing.orbit(rcx.cam_index);
                    }
                    Key::Character("p") | Key::Character("P") if n > 0 => {
                        rcx.cam_index = (rcx.cam_index + n - 1) % n;
                        self.orbit = self.framing.orbit(rcx.cam_index);
                    }
                    Key::Character("r") | Key::Character("R") => {
                        self.orbit = self.framing.orbit(rcx.cam_index);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.rcx.is_none() {
            match self.init(el) {
                Ok(r) => self.rcx = Some(r),
                Err(e) => {
                    self.error = Some(e);
                    el.exit();
                }
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Err(e) = self.handle(el, event) {
            self.error = Some(e);
            el.exit();
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(rcx) = &self.rcx {
            rcx.window.request_redraw();
        }
    }
}
