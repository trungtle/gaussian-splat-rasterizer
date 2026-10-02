mod camera;
mod debug;
mod ply;
mod renderer;
mod viewer;

use anyhow::{Context, Result, bail};
use camera::{Camera, DatasetCamera, Orbit};
use debug::{DebugSettings, ViewMode};
use glam::{Mat3, Vec3};
use std::path::PathBuf;

const USAGE: &str = "\
usage: gaussian-splat-rasterizer <scene.ply> [options]

options:
  --cameras <cameras.json>  dataset cameras (default: cameras.json next to the ply, or one dir up)
  --cam <N>                 start from dataset camera N (default 0)
  --out <file.png>          render one frame headless and exit
  --size <W>x<H>            output / window size (default 1280x720; headless with
                            dataset camera keeps its aspect at width W)
  --bg <r,g,b>              background color in 0..1 (default 0,0,0)
  --up <x,y,z>              world up axis (default: from cameras, else 0,1,0)
  --orbit <deg>             headless: rotate the start view around the scene by deg
  --turntable <N>           headless: write N frames of a full orbit as <out-stem>-000.png ...

debug / visualization:
  --view <mode|all>         color, base-color, view-dependent, depth, opacity, ellipses,
                            centers, anisotropy, overdraw, quads; `all` (headless) writes
                            one <out-stem>-<mode>.png per mode
  --splat-scale <s>         multiply every Gaussian's scale (default 1)
  --min-opacity <a>         hide splats with opacity below a (default 0)
  --sh <deg>                cap the SH degree used for color (0..3)";

pub struct Args {
    pub ply: PathBuf,
    pub cameras: Option<PathBuf>,
    pub cam: usize,
    pub out: Option<PathBuf>,
    pub size: (u32, u32),
    pub bg: [f32; 3],
    pub up: Option<Vec3>,
    pub orbit_deg: f32,
    pub debug: DebugSettings,
    /// Headless: render every view mode.
    pub all_views: bool,
    /// Headless: number of frames for a full orbit (0 = single frame).
    pub turntable: u32,
}

fn parse_vec3(s: &str) -> Result<[f32; 3]> {
    let v: Vec<f32> = s.split(',').map(|x| x.trim().parse()).collect::<Result<_, _>>()?;
    if v.len() != 3 {
        bail!("expected 3 comma-separated numbers, got {s}");
    }
    Ok([v[0], v[1], v[2]])
}

fn parse_args() -> Result<Args> {
    let mut it = std::env::args().skip(1);
    let mut a = Args {
        ply: PathBuf::new(),
        cameras: None,
        cam: 0,
        out: None,
        size: (1280, 720),
        bg: [0.0; 3],
        up: None,
        orbit_deg: 0.0,
        debug: DebugSettings::default(),
        all_views: false,
        turntable: 0,
    };
    let mut ply = None;
    while let Some(arg) = it.next() {
        let mut val = || it.next().with_context(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            "--cameras" => a.cameras = Some(val()?.into()),
            "--cam" => a.cam = val()?.parse()?,
            "--out" => a.out = Some(val()?.into()),
            "--size" => {
                let v = val()?;
                let (w, h) = v.split_once('x').context("--size expects WxH")?;
                a.size = (w.parse()?, h.parse()?);
            }
            "--bg" => a.bg = parse_vec3(&val()?)?,
            "--up" => a.up = Some(Vec3::from(parse_vec3(&val()?)?)),
            "--orbit" => a.orbit_deg = val()?.parse()?,
            "--turntable" => a.turntable = val()?.parse()?,
            "--view" => match val()?.as_str() {
                "all" => a.all_views = true,
                v => a.debug.view = ViewMode::from_name(v).with_context(|| format!("unknown view {v}
{USAGE}"))?,
            },
            "--splat-scale" => a.debug.splat_scale = val()?.parse()?,
            "--min-opacity" => a.debug.min_opacity = val()?.parse()?,
            "--sh" => a.debug.sh_max = val()?.parse()?,
            s if s.starts_with('-') => bail!("unknown option {s}\n{USAGE}"),
            s => ply = Some(PathBuf::from(s)),
        }
    }
    a.ply = ply.context(USAGE)?;
    if a.cameras.is_none() {
        let dir = a.ply.parent().unwrap_or(".".as_ref());
        a.cameras = [dir.join("cameras.json"), dir.join("../cameras.json"), dir.join("../../../cameras.json")]
            .into_iter()
            .find(|p| p.exists());
    }
    Ok(a)
}

/// Scene framing shared by the headless and windowed paths.
pub struct Framing {
    pub dataset: Vec<DatasetCamera>,
    pub up: Vec3,
    pub target: Vec3,
    pub radius: f32,
}

impl Framing {
    fn new(args: &Args, scene: &ply::Scene) -> Result<Self> {
        let dataset = match &args.cameras {
            Some(p) => camera::load_cameras(p)?,
            None => Vec::new(),
        };
        // Robust scene center/radius: trimmed statistics over splat positions.
        let mut pos: Vec<Vec3> =
            scene.splats.iter().map(|s| Vec3::from_slice(&s.pos_opacity[..3])).collect();
        let n = pos.len().max(1) as f32;
        let centroid = pos.iter().copied().sum::<Vec3>() / n;
        pos.sort_by(|a, b| a.distance_squared(centroid).total_cmp(&b.distance_squared(centroid)));
        let radius = pos.get(pos.len() * 9 / 10).map_or(1.0, |p| p.distance(centroid)).max(1e-3);

        let up = args.up.unwrap_or_else(|| {
            if dataset.is_empty() {
                Vec3::Y
            } else {
                // Camera +y is "down" in image space; average it for the world up.
                -dataset.iter().map(|c| c.camera.view.row(1).truncate()).sum::<Vec3>().normalize()
            }
        });

        // Target: the point closest (least squares) to all dataset camera view rays.
        let target = if dataset.len() >= 2 {
            let mut a = Mat3::ZERO;
            let mut b = Vec3::ZERO;
            for c in &dataset {
                let d = c.camera.forward();
                let m = Mat3::IDENTITY - Mat3::from_cols(d * d.x, d * d.y, d * d.z);
                a += m;
                b += m * c.camera.position;
            }
            if a.determinant().abs() > 1e-6 { a.inverse() * b } else { centroid }
        } else {
            centroid
        };
        Ok(Self { dataset, up, target, radius })
    }

    /// Initial orbit for the viewer / headless renders.
    pub fn orbit(&self, cam: usize) -> Orbit {
        let fov_y = 50f32.to_radians();
        match self.dataset.get(cam) {
            Some(c) => {
                let fov = 2.0 * (c.height as f32 * 0.5 / c.camera.fy).atan();
                Orbit::from_camera(&c.camera, self.target, self.up, fov)
            }
            None => Orbit {
                target: self.target,
                distance: self.radius * 2.5,
                yaw: 0.0,
                pitch: 0.3,
                up: self.up,
                fov_y,
            },
        }
    }
}

fn main() -> Result<()> {
    let args = parse_args()?;
    let t0 = std::time::Instant::now();
    let scene = ply::load(&args.ply)?;
    eprintln!(
        "loaded {} splats (SH degree {}) in {:.2?}",
        scene.splats.len(),
        scene.sh_degree,
        t0.elapsed()
    );
    let framing = Framing::new(&args, &scene)?;

    match &args.out {
        Some(out) => headless(&args, &scene, &framing, out),
        None => viewer::run(args, scene, framing),
    }
}

fn headless(args: &Args, scene: &ply::Scene, framing: &Framing, out: &PathBuf) -> Result<()> {
    let gpu = renderer::Gpu::new(Default::default(), None)?;
    let mut r = renderer::SplatRenderer::new(&gpu, scene, renderer::OFFSCREEN_FORMAT)?;
    r.background = [args.bg[0], args.bg[1], args.bg[2], 1.0];

    let (w, mut h) = args.size;
    if args.turntable > 0 {
        r.debug = args.debug;
        let stem = out.file_stem().unwrap_or_default().to_string_lossy();
        let base = framing.orbit(args.cam);
        let t = std::time::Instant::now();
        for i in 0..args.turntable {
            let mut o = base.clone();
            o.yaw += args.orbit_deg.to_radians() + std::f32::consts::TAU * i as f32 / args.turntable as f32;
            let pixels = renderer::render_offscreen(&gpu, &mut r, &o.camera(h), w, h)?;
            let path = out.with_file_name(format!("{stem}-{i:03}.png"));
            image::save_buffer(&path, &pixels, w, h, image::ExtendedColorType::Rgba8)?;
        }
        eprintln!("wrote {} turntable frames ({w}x{h}) in {:.2?}", args.turntable, t.elapsed());
        return Ok(());
    }
    let cam: Camera = match framing.dataset.get(args.cam) {
        Some(dc) if args.orbit_deg == 0.0 => {
            h = (w as f32 * dc.height as f32 / dc.width as f32).round() as u32;
            dc.camera.scaled(dc.width, w)
        }
        _ => {
            let mut o = framing.orbit(args.cam);
            o.yaw += args.orbit_deg.to_radians();
            o.camera(h)
        }
    };
    let jobs: Vec<(ViewMode, PathBuf)> = if args.all_views {
        let stem = out.file_stem().unwrap_or_default().to_string_lossy();
        ViewMode::ALL
            .into_iter()
            .map(|m| (m, out.with_file_name(format!("{stem}-{}.png", m.name()))))
            .collect()
    } else {
        vec![(args.debug.view, out.clone())]
    };
    for (view, path) in jobs {
        r.debug = DebugSettings { view, ..args.debug };
        let t = std::time::Instant::now();
        let pixels = renderer::render_offscreen(&gpu, &mut r, &cam, w, h)?;
        eprintln!("rendered {w}x{h} [{}] in {:.2?}", r.debug.summary(), t.elapsed());
        image::save_buffer(&path, &pixels, w, h, image::ExtendedColorType::Rgba8)?;
        eprintln!("wrote {}", path.display());
    }
    Ok(())
}
